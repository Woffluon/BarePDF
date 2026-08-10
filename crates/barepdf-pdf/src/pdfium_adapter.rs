use crate::backend::{
    OutlineNode, PdfBackend, PdfDocument as CorePdfDocument, RawBitmap, TextSpan,
};
use barepdf_core::{PageCount, PageIndex, PdfError, Rotation};
use pdfium_render::prelude::*;
use std::path::Path;
use std::sync::Arc;

pub struct PdfiumEngine {
    pdfium: Arc<Pdfium>,
}

impl PdfiumEngine {
    /// # Errors
    ///
    /// Returns a platform error when the sibling `PDFium` library cannot be located or bound.
    pub fn new() -> Result<Self, PdfError> {
        let library_path = std::env::current_exe()
            .map_err(|error| PdfError::PlatformError(format!("Cannot locate application executable: {error}")))?
            .parent()
            .map(|directory| directory.join(Pdfium::pdfium_platform_library_name()))
            .ok_or_else(|| PdfError::PlatformError("Application executable has no parent directory".into()))?
            .canonicalize()
            .map_err(|error| PdfError::PlatformError(format!("Cannot locate sibling PDFium library: {error}")))?;
        let bindings = Pdfium::bind_to_library(library_path)
            .map_err(|e| PdfError::PlatformError(format!("Failed to bind PDFium library: {e}")))?;

        let pdfium = Pdfium::new(bindings);
        Ok(Self {
            pdfium: Arc::new(pdfium),
        })
    }
}

pub struct PdfiumDocumentOwned {
    doc: PdfDocument<'static>,
    _pdfium: Arc<Pdfium>,
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
            .map_err(|error| map_load_error(&error, password.is_some()))?;

        // SAFETY: doc borrows Pdfium. This owner keeps the Arc<Pdfium> alive and declares doc
        // first, so Rust drops doc before the binding owner.
        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };

        Ok(Box::new(PdfiumDocumentOwned {
            doc: doc_static,
            _pdfium: self.pdfium.clone(),
        }))
    }

    fn open_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<Box<dyn CorePdfDocument>, PdfError> {
        let doc = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, password)
            .map_err(|error| map_load_error(&error, password.is_some()))?;

        // SAFETY: doc borrows Pdfium. This owner keeps the Arc<Pdfium> alive and declares doc
        // first, so Rust drops doc before the binding owner.
        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };

        Ok(Box::new(PdfiumDocumentOwned {
            doc: doc_static,
            _pdfium: self.pdfium.clone(),
        }))
    }
}

impl CorePdfDocument for PdfiumDocumentOwned {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        let count = u32::try_from(self.doc.pages().len())
            .map_err(|_| PdfError::InvalidPdf("PDF page count exceeds supported range".into()))?;
        PageCount::new(count).ok_or_else(|| PdfError::InvalidPdf("PDF contains no pages".into()))
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
        let target_height = i32::try_from(target_height).map_err(|_| PdfError::RenderingFailed {
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

        Ok(RawBitmap {
            width: w,
            height: h,
            pixels,
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

        let mut glyphs = Vec::new();
        for char_info in text_page.chars().iter() {
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
        while let Some(bookmark) = current {
            nodes.push(outline_node(&bookmark));
            current = bookmark.next_sibling();
        }
        Ok(nodes)
    }
}

fn to_pdfium_index(index: PageIndex) -> Result<i32, PdfError> {
    i32::try_from(index.get())
        .map_err(|_| PdfError::InvalidPdf("Page index exceeds PDFium's supported range".into()))
}

fn map_load_error(error: &PdfiumError, password_supplied: bool) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if password_supplied {
                PdfError::IncorrectPassword
            } else {
                PdfError::PasswordRequired
            }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            PdfError::FileNotFound(error.to_string())
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            PdfError::InvalidPdf(error.to_string())
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            PdfError::UnsupportedEncryption(error.to_string())
        }
        _ => PdfError::InvalidPdf(error.to_string()),
    }
}

fn outline_node(bookmark: &PdfBookmark<'_>) -> OutlineNode {
    OutlineNode {
        title: bookmark.title().unwrap_or_default(),
        page_index: bookmark_page_index(bookmark),
        children: bookmark
            .iter_direct_children()
            .map(|child| outline_node(&child))
            .collect(),
    }
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
