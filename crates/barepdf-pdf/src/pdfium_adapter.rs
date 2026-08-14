use crate::backend::{
    OutlineNode, PdfBackend, PdfDocument as CorePdfDocument, RawBitmap, TextSpan,
};
use crate::pdfium_lifetime::process_pdfium;
use barepdf_core::{
    PageCount, PageIndex, PdfError, Rotation, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS,
};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::Arc;

const MAX_TEXT_GLYPHS_PER_PAGE: usize = 250_000;

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
        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|error| map_load_error(error, password.is_some()))?;
        Ok(Box::new(PdfiumDocumentOwned { doc }))
    }

    fn open_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<Box<dyn CorePdfDocument>, PdfError> {
        let doc = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, password)
            .map_err(|error| map_load_error(error, password.is_some()))?;
        Ok(Box::new(PdfiumDocumentOwned { doc }))
    }
}

impl CorePdfDocument for PdfiumDocumentOwned {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        let count = u32::try_from(self.doc.pages().len()).map_err(|_| {
            PdfError::InvalidPdfReason("PDF page count exceeds supported range".into())
        })?;
        PageCount::new(count)
            .ok_or_else(|| PdfError::InvalidPdfReason("PDF contains no pages".into()))
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

        let mut spans = Vec::new();
        for char_info in text_page.chars().iter() {
            if let Ok(rect) = char_info.loose_bounds() {
                let char_text = char_info
                    .unicode_char()
                    .map_or_else(|| " ".to_string(), |character| character.to_string());
                spans.push(TextSpan {
                    text: char_text,
                    x: rect.left().value,
                    y: rect.bottom().value,
                    width: rect.width().value,
                    height: rect.height().value,
                });
            }
        }

        Ok(spans)
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

        Ok(barepdf_core::PageTextGeometry { page_index, glyphs })
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

fn to_pdfium_index(index: PageIndex) -> Result<i32, PdfError> {
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
    use super::{validate_glyph_count, validate_outline_limits};
    use barepdf_core::{PageIndex, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS};

    #[test]
    fn outline_limits_accept_boundary_and_reject_excess() {
        assert!(validate_outline_limits(MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS - 1).is_ok());
        assert!(validate_outline_limits(MAX_OUTLINE_DEPTH + 1, 0).is_err());
        assert!(validate_outline_limits(0, MAX_OUTLINE_ITEMS).is_err());
    }

    #[test]
    fn glyph_limit_rejects_oversized_page_geometry() {
        assert!(validate_glyph_count(PageIndex::zero(), 250_000).is_ok());
        assert!(validate_glyph_count(PageIndex::zero(), 250_001).is_err());
    }
}
