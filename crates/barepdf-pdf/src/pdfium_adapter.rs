use crate::backend::{
    OutlineNode, PdfBackend, PdfDocument as CorePdfDocument, RawBitmap, TextSpan,
};
use crate::pdfium_lifetime::process_pdfium;
use barepdf_core::{
    PageCount, PageIndex, PdfError, Rotation, SecretPassword, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS,
};
use pdfium_render::prelude::*;
use std::ffi::CString;
use std::path::Path;
use std::sync::Arc;

mod glyph_limit_scope {
    const MAX_TEXT_GLYPHS_PER_PAGE: usize = 250_000;

    pub const fn get() -> usize {
        #[allow(unused_imports)]
        use barepdf_core::limits::*;
        MAX_TEXT_GLYPHS_PER_PAGE
    }
}

pub const MAX_TEXT_GLYPHS_PER_PAGE: usize = glyph_limit_scope::get();
const MAX_LINKS_PER_PAGE: usize = 2_000;

struct ZeroizingFfiPassword {
    secret: SecretPassword,
}

impl ZeroizingFfiPassword {
    fn new(password: Option<&str>) -> Result<Option<Self>, PdfError> {
        let Some(password) = password else {
            return Ok(None);
        };
        if password.as_bytes().contains(&0) {
            return Err(PdfError::IncorrectPassword);
        }
        let c_string = CString::new(password).map_err(|_| PdfError::IncorrectPassword)?;
        let owned = c_string
            .into_string()
            .map_err(|_| PdfError::IncorrectPassword)?;
        Ok(Some(Self {
            secret: SecretPassword::new(owned),
        }))
    }

    fn expose(&self) -> &str {
        self.secret.expose()
    }

    fn clear(&mut self) {
        self.secret.clear();
    }

    #[cfg(test)]
    fn bytes_for_test(&self) -> &[u8] {
        self.secret.bytes_for_test()
    }
}

impl Drop for ZeroizingFfiPassword {
    fn drop(&mut self) {
        self.secret.clear();
    }
}

pub struct PdfiumEngine {
    pdfium: &'static Pdfium,
}

impl PdfiumEngine {
    /// # Errors
    ///
    /// Returns a platform error when the sibling `PDFium` library cannot be located or bound.
    pub fn new() -> Result<Self, PdfError> {
        process_pdfium().map(|pdfium| Self { pdfium })
    }
}

pub struct PdfiumDocumentOwned {
    doc: PdfDocument<'static>,
}

impl PdfBackend for PdfiumEngine {
    fn open_path(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Box<dyn CorePdfDocument>, PdfError> {
        let mut ffi_password = ZeroizingFfiPassword::new(password)?;
        let load_result = self.pdfium.load_pdf_from_file(
            path,
            ffi_password.as_ref().map(ZeroizingFfiPassword::expose),
        );
        if let Some(ref mut secret) = ffi_password {
            secret.clear();
        }
        let doc = load_result.map_err(|error| map_load_error(error, password.is_some()))?;
        let _ = validate_loaded_page_count(doc.pages().len())?;
        Ok(Box::new(PdfiumDocumentOwned { doc }))
    }

    fn open_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<Box<dyn CorePdfDocument>, PdfError> {
        let mut ffi_password = ZeroizingFfiPassword::new(password)?;
        let load_result = self.pdfium.load_pdf_from_byte_vec(
            bytes,
            ffi_password.as_ref().map(ZeroizingFfiPassword::expose),
        );
        if let Some(ref mut secret) = ffi_password {
            secret.clear();
        }
        let doc = load_result.map_err(|error| map_load_error(error, password.is_some()))?;
        let _ = validate_loaded_page_count(doc.pages().len())?;
        Ok(Box::new(PdfiumDocumentOwned { doc }))
    }
}

impl CorePdfDocument for PdfiumDocumentOwned {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        validate_loaded_page_count(self.doc.pages().len())
    }

    fn page_dimensions(&self, page_index: PageIndex) -> Result<(f32, f32), PdfError> {
        let pages = self.doc.pages();
        let page =
            pages
                .get(to_pdfium_index(page_index)?)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let width = page.width().value;
        let height = page.height().value;
        Ok((width, height))
    }

    fn render_page(
        &self,
        page_index: PageIndex,
        target_width: u32,
        target_height: u32,
        rotation: Rotation,
    ) -> Result<RawBitmap, PdfError> {
        let target_width = i32::try_from(target_width).map_err(|_| PdfError::RenderingFailed {
            page_index: page_index.get(),
            reason: "target width exceeds PDFium's supported range".into(),
        })?;
        let target_height =
            i32::try_from(target_height).map_err(|_| PdfError::RenderingFailed {
                page_index: page_index.get(),
                reason: "target height exceeds PDFium's supported range".into(),
            })?;
        let pages = self.doc.pages();
        let page =
            pages
                .get(to_pdfium_index(page_index)?)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let pdfium_rotation = match rotation {
            Rotation::Degrees0 => PdfPageRenderRotation::None,
            Rotation::Degrees90 => PdfPageRenderRotation::Degrees90,
            Rotation::Degrees180 => PdfPageRenderRotation::Degrees180,
            Rotation::Degrees270 => PdfPageRenderRotation::Degrees270,
        };
        let render_config = PdfRenderConfig::new()
            .set_target_width(target_width)
            .set_target_height(target_height)
            .rotate(pdfium_rotation, true)
            .limit_render_image_cache_size(true);

        let bitmap =
            page.render_with_config(&render_config)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let w = u32::try_from(bitmap.width()).map_err(|_| PdfError::RenderingFailed {
            page_index: page_index.get(),
            reason: "PDFium returned a negative bitmap width".into(),
        })?;
        let h = u32::try_from(bitmap.height()).map_err(|_| PdfError::RenderingFailed {
            page_index: page_index.get(),
            reason: "PDFium returned a negative bitmap height".into(),
        })?;
        let pixels = bitmap.as_rgba_bytes();

        RawBitmap::new(w, h, pixels).map_err(|error| PdfError::RenderingFailed {
            page_index: page_index.get(),
            reason: error.to_string(),
        })
    }

    fn extract_text(&self, page_index: PageIndex) -> Result<String, PdfError> {
        let pages = self.doc.pages();
        let page = pages.get(to_pdfium_index(page_index)?).map_err(|e| {
            PdfError::TextExtractionFailed {
                page_index: page_index.get(),
                reason: e.to_string(),
            }
        })?;

        let text_page = page.text().map_err(|e| PdfError::TextExtractionFailed {
            page_index: page_index.get(),
            reason: e.to_string(),
        })?;

        validate_glyph_count(page_index, text_page.chars().len())?;
        Ok(text_page.all())
    }

    fn extract_text_spans(&self, page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
        let pages = self.doc.pages();
        let page = pages.get(to_pdfium_index(page_index)?).map_err(|e| {
            PdfError::TextExtractionFailed {
                page_index: page_index.get(),
                reason: e.to_string(),
            }
        })?;

        let text_page = page.text().map_err(|e| PdfError::TextExtractionFailed {
            page_index: page_index.get(),
            reason: e.to_string(),
        })?;

        let chars = text_page.chars();
        validate_glyph_count(page_index, chars.len())?;

        let glyphs = chars.iter().filter_map(|char_info| {
            let rect = char_info.loose_bounds().ok()?;
            Some(RawGlyph {
                ch: char_info.unicode_char().unwrap_or(' '),
                x: rect.left().value,
                y: rect.bottom().value,
                width: rect.width().value,
                height: rect.height().value,
            })
        });

        Ok(coalesce_glyphs_into_spans(glyphs, chars.len()))
    }

    fn get_page_text_geometry(
        &self,
        page_index: PageIndex,
    ) -> Result<barepdf_core::PageTextGeometry, PdfError> {
        let pages = self.doc.pages();
        let page = pages.get(to_pdfium_index(page_index)?).map_err(|e| {
            PdfError::TextExtractionFailed {
                page_index: page_index.get(),
                reason: e.to_string(),
            }
        })?;

        let text_page = page.text().map_err(|e| PdfError::TextExtractionFailed {
            page_index: page_index.get(),
            reason: e.to_string(),
        })?;

        let chars = text_page.chars();
        validate_glyph_count(page_index, chars.len())?;
        let mut glyphs = Vec::new();
        glyphs
            .try_reserve_exact(chars.len())
            .map_err(|_| PdfError::TextExtractionFailed {
                page_index: page_index.get(),
                reason: "could not allocate page text geometry".into(),
            })?;
        for char_info in chars.iter() {
            let ch = char_info.unicode_char().unwrap_or(' ');
            if let Ok(rect) = char_info.loose_bounds() {
                let x1 = rect.left().value.min(rect.right().value);
                let x2 = rect.left().value.max(rect.right().value);
                let y1 = rect.bottom().value.min(rect.top().value);
                let y2 = rect.bottom().value.max(rect.top().value);
                glyphs.push(barepdf_core::GlyphRect {
                    x: x1,
                    y: y1,
                    width: (x2 - x1).max(0.0),
                    height: (y2 - y1).max(0.0),
                    ch,
                });
            } else {
                glyphs.push(barepdf_core::GlyphRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    ch,
                });
            }
        }

        let mut links = Vec::new();
        for link in page.links().iter().take(MAX_LINKS_PER_PAGE) {
            let Ok(rect) = link.rect() else {
                continue;
            };
            let x1 = rect.left().value.min(rect.right().value);
            let x2 = rect.left().value.max(rect.right().value);
            let y1 = rect.bottom().value.min(rect.top().value);
            let y2 = rect.bottom().value.max(rect.top().value);

            let mut target = None;
            if let Some(action) = link.action() {
                if let Some(uri_action) = action.as_uri_action() {
                    if let Ok(uri) = uri_action.uri() {
                        target = Some(barepdf_core::LinkTarget::Url(uri));
                    }
                } else if let Some(local) = action.as_local_destination_action() {
                    if let Ok(dest) = local.destination() {
                        if let Ok(idx) = dest.page_index() {
                            if let Ok(idx_u32) = u32::try_from(idx) {
                                target = Some(barepdf_core::LinkTarget::Page(PageIndex::from_raw(
                                    idx_u32,
                                )));
                            }
                        }
                    }
                }
            }
            if target.is_none() {
                if let Some(dest) = link.destination() {
                    if let Ok(idx) = dest.page_index() {
                        if let Ok(idx_u32) = u32::try_from(idx) {
                            target =
                                Some(barepdf_core::LinkTarget::Page(PageIndex::from_raw(idx_u32)));
                        }
                    }
                }
            }
            if let Some(target) = target {
                links.push(barepdf_core::PageLink {
                    x: x1,
                    y: y1,
                    width: (x2 - x1).max(0.0),
                    height: (y2 - y1).max(0.0),
                    target,
                });
            }
        }

        Ok(barepdf_core::PageTextGeometry {
            page_index,
            glyphs,
            links,
        })
    }

    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
        let mut nodes = Vec::new();
        let mut current = self.doc.bookmarks().root();
        let mut visited = 0;
        while let Some(bookmark) = current {
            current = bookmark.next_sibling();
            nodes.push(bounded_outline(&bookmark, &mut visited)?);
        }
        Ok(nodes)
    }
}

fn validate_loaded_page_count(raw_len: PdfPageIndex) -> Result<PageCount, PdfError> {
    let count = u32::try_from(raw_len)
        .map_err(|_| PdfError::InvalidPdfReason("PDF page count exceeds supported range".into()))?;
    let page_count = PageCount::new(count)
        .ok_or_else(|| PdfError::InvalidPdfReason("PDF contains no pages".into()))?;
    barepdf_core::validate_document_page_count(page_count)
        .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))?;
    Ok(page_count)
}

#[derive(Debug, Clone, Copy)]
struct RawGlyph {
    ch: char,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn coalesce_glyphs_into_spans(
    glyphs: impl IntoIterator<Item = RawGlyph>,
    char_count_hint: usize,
) -> Vec<TextSpan> {
    let mut spans: Vec<TextSpan> = Vec::with_capacity(char_count_hint.min(1024));
    for glyph in glyphs {
        let can_merge = spans.last().is_some_and(|prev| {
            let line_height = prev.height.max(glyph.height).max(1.0);
            let same_baseline = (glyph.y - prev.y).abs() <= line_height * 0.5;
            let prev_right = prev.x + prev.width;
            let horizontal_gap = glyph.x - prev_right;
            same_baseline && (-line_height * 0.25..=line_height * 1.5).contains(&horizontal_gap)
        });
        if can_merge {
            if let Some(prev) = spans.last_mut() {
                prev.text.push(glyph.ch);
                let left = prev.x.min(glyph.x);
                let right = (prev.x + prev.width).max(glyph.x + glyph.width);
                let bottom = prev.y.min(glyph.y);
                let top = (prev.y + prev.height).max(glyph.y + glyph.height);
                prev.x = left;
                prev.y = bottom;
                prev.width = (right - left).max(0.0);
                prev.height = (top - bottom).max(0.0);
            }
        } else {
            let mut text = String::with_capacity(16);
            text.push(glyph.ch);
            spans.push(TextSpan {
                text,
                x: glyph.x,
                y: glyph.y,
                width: glyph.width,
                height: glyph.height,
            });
        }
    }
    spans
}

fn to_pdfium_index(index: PageIndex) -> Result<i32, PdfError> {
    if !index.is_within_limit() {
        return Err(PdfError::InvalidPdfReason(
            "Page index exceeds supported document page limit".into(),
        ));
    }
    i32::try_from(index.get()).map_err(|_| {
        PdfError::InvalidPdfReason("Page index exceeds PDFium's supported range".into())
    })
}

fn map_load_error(error: PdfiumError, password_supplied: bool) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if password_supplied {
                PdfError::IncorrectPassword
            } else {
                PdfError::PasswordRequired
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            PdfError::FileAccess {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            PdfError::InvalidPdf {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            PdfError::UnsupportedEncryption {
                source: Arc::new(error),
            }
        }
        error => PdfError::Backend {
            source: Arc::new(error),
        },
    }
}

fn validate_glyph_count(page_index: PageIndex, count: usize) -> Result<(), PdfError> {
    if count > MAX_TEXT_GLYPHS_PER_PAGE {
        return Err(PdfError::TextExtractionFailed {
            page_index: page_index.get(),
            reason: "page text geometry exceeds limit".into(),
        });
    }

    Ok(())
}

fn bounded_outline(root: &PdfBookmark<'_>, visited: &mut usize) -> Result<OutlineNode, PdfError> {
    let mut stack = vec![OutlineFrame::new(root, 0, visited)?];

    loop {
        let Some(frame) = stack.last_mut() else {
            return Err(PdfError::InvalidPdfReason(
                "PDF outline traversal ended unexpectedly".into(),
            ));
        };

        if let Some(bookmark) = frame.next_child.take() {
            frame.next_child = bookmark.next_sibling();
            let depth = frame.depth.saturating_add(1);
            stack.push(OutlineFrame::new(&bookmark, depth, visited)?);
            continue;
        }

        let node = stack.pop().map(|frame| frame.node).ok_or_else(|| {
            PdfError::InvalidPdfReason("PDF outline traversal ended unexpectedly".into())
        })?;
        if let Some(parent) = stack.last_mut() {
            parent.node.children.push(node);
        } else {
            return Ok(node);
        }
    }
}

struct OutlineFrame<'a> {
    node: OutlineNode,
    next_child: Option<PdfBookmark<'a>>,
    depth: usize,
}

impl<'a> OutlineFrame<'a> {
    fn new(
        bookmark: &PdfBookmark<'a>,
        depth: usize,
        visited: &mut usize,
    ) -> Result<Self, PdfError> {
        validate_outline_limits(depth, *visited)?;
        *visited = visited.saturating_add(1);
        let next_child = bookmark.iter_direct_children().next();
        let node = OutlineNode {
            title: bookmark.title().unwrap_or_default(),
            page_index: bookmark_page_index(bookmark),
            children: Vec::new(),
        };

        Ok(Self {
            node,
            next_child,
            depth,
        })
    }
}

fn validate_outline_limits(depth: usize, visited: usize) -> Result<(), PdfError> {
    if depth > MAX_OUTLINE_DEPTH || visited >= MAX_OUTLINE_ITEMS {
        return Err(PdfError::InvalidPdfReason(
            "PDF outline exceeds limits".into(),
        ));
    }

    Ok(())
}

fn bookmark_page_index(bookmark: &PdfBookmark<'_>) -> Option<u32> {
    if let Some(index) = bookmark
        .destination()
        .and_then(|destination| destination.page_index().ok())
        .and_then(|index| u32::try_from(index).ok())
    {
        return Some(index);
    }
    bookmark
        .action()?
        .as_local_destination_action()?
        .destination()
        .ok()?
        .page_index()
        .ok()
        .and_then(|index| u32::try_from(index).ok())
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_glyphs_into_spans, validate_glyph_count, validate_loaded_page_count,
        validate_outline_limits, RawGlyph, ZeroizingFfiPassword, MAX_TEXT_GLYPHS_PER_PAGE,
    };
    use barepdf_core::limits::MAX_DOCUMENT_PAGES;
    use barepdf_core::{PageIndex, PdfError, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS};
    use pdfium_render::prelude::PdfPageIndex;

    #[test]
    fn outline_limits_accept_boundary_and_reject_excess() {
        assert!(validate_outline_limits(MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS - 1).is_ok());
        assert!(validate_outline_limits(MAX_OUTLINE_DEPTH + 1, 0).is_err());
        assert!(validate_outline_limits(0, MAX_OUTLINE_ITEMS).is_err());
    }

    #[test]
    fn glyph_limit_rejects_oversized_page_geometry() {
        assert!(validate_glyph_count(PageIndex::zero(), MAX_TEXT_GLYPHS_PER_PAGE).is_ok());
        assert!(validate_glyph_count(PageIndex::zero(), MAX_TEXT_GLYPHS_PER_PAGE + 1).is_err());
    }

    #[test]
    fn loaded_page_count_enforces_max_document_pages() {
        assert!(validate_loaded_page_count(0).is_err());
        let max_valid = PdfPageIndex::try_from(MAX_DOCUMENT_PAGES).expect("fits in PdfPageIndex");
        assert_eq!(
            validate_loaded_page_count(max_valid)
                .expect("at limit")
                .get(),
            MAX_DOCUMENT_PAGES
        );
        let over_limit =
            PdfPageIndex::try_from(MAX_DOCUMENT_PAGES + 1).expect("fits in PdfPageIndex");
        assert!(matches!(
            validate_loaded_page_count(over_limit),
            Err(PdfError::InvalidPdfReason(_))
        ));
    }

    #[test]
    fn coalesce_glyphs_into_spans_buffers_adjacent_characters_per_line() {
        let glyphs = [
            RawGlyph {
                ch: 'H',
                x: 10.0,
                y: 100.0,
                width: 6.0,
                height: 12.0,
            },
            RawGlyph {
                ch: 'i',
                x: 16.5,
                y: 100.0,
                width: 3.0,
                height: 12.0,
            },
            RawGlyph {
                ch: '!',
                x: 20.0,
                y: 100.0,
                width: 3.0,
                height: 12.0,
            },
            RawGlyph {
                ch: 'N',
                x: 10.0,
                y: 80.0,
                width: 7.0,
                height: 12.0,
            },
            RawGlyph {
                ch: 'e',
                x: 17.0,
                y: 80.0,
                width: 5.0,
                height: 12.0,
            },
            RawGlyph {
                ch: 'w',
                x: 22.0,
                y: 80.0,
                width: 7.0,
                height: 12.0,
            },
        ];

        let spans = coalesce_glyphs_into_spans(glyphs, glyphs.len());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "Hi!");
        assert!((spans[0].x - 10.0).abs() < f32::EPSILON);
        assert!((spans[0].width - 13.0).abs() < f32::EPSILON);
        assert_eq!(spans[1].text, "New");
        assert!((spans[1].y - 80.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ffi_password_rejects_interior_nul_and_zeroizes_on_clear() {
        assert!(ZeroizingFfiPassword::new(None)
            .expect("none password")
            .is_none());
        assert!(matches!(
            ZeroizingFfiPassword::new(Some("bad\0password")),
            Err(PdfError::IncorrectPassword)
        ));

        let mut ffi_password = ZeroizingFfiPassword::new(Some("top-secret-pass"))
            .expect("valid password")
            .expect("some password");
        assert_eq!(ffi_password.expose(), "top-secret-pass");
        assert_eq!(ffi_password.bytes_for_test(), b"top-secret-pass");
        ffi_password.clear();
        assert!(ffi_password.bytes_for_test().is_empty());
        assert_eq!(ffi_password.expose(), "");
    }
}
