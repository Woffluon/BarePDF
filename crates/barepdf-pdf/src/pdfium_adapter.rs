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
    pub fn new() -> Result<Self, PdfError> {
        let bindings = Pdfium::bind_to_system_library()
            .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(".")))
            .map_err(|e| PdfError::PlatformError(format!("Failed to bind PDFium library: {e}")))?;

        let pdfium = Pdfium::new(bindings);
        Ok(Self {
            pdfium: Arc::new(pdfium),
        })
    }
}

pub struct PdfiumDocumentOwned {
    _pdfium: Arc<Pdfium>,
    doc: PdfDocument<'static>,
}

// SAFETY: Document handle usage is thread-isolated within the background actor thread.
unsafe impl Send for PdfiumDocumentOwned {}
unsafe impl Sync for PdfiumDocumentOwned {}

impl PdfBackend for PdfiumEngine {
    fn open_path(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Box<dyn CorePdfDocument>, PdfError> {
        let path_str = path
            .to_str()
            .ok_or_else(|| PdfError::FileNotFound(path.display().to_string()))?;

        let doc = self
            .pdfium
            .load_pdf_from_file(path_str, password)
            .map_err(|error| map_load_error(error, password.is_some()))?;

        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };

        Ok(Box::new(PdfiumDocumentOwned {
            _pdfium: self.pdfium.clone(),
            doc: doc_static,
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
            .map_err(|error| map_load_error(error, password.is_some()))?;

        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };

        Ok(Box::new(PdfiumDocumentOwned {
            _pdfium: self.pdfium.clone(),
            doc: doc_static,
        }))
    }
}

impl CorePdfDocument for PdfiumDocumentOwned {
    fn page_count(&self) -> PageCount {
        let count = self.doc.pages().len() as u32;
        PageCount::new(count).unwrap_or_else(|| PageCount::new(1).expect("non-zero"))
    }

    fn page_dimensions(&self, page_index: PageIndex) -> Result<(f32, f32), PdfError> {
        let pages = self.doc.pages();
        let page =
            pages
                .get(page_index.get() as u16 as i32)
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
        _rotation: Rotation,
    ) -> Result<RawBitmap, PdfError> {
        let pages = self.doc.pages();
        let page =
            pages
                .get(page_index.get() as u16 as i32)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let render_config = PdfRenderConfig::new()
            .set_target_width(target_width as i32)
            .set_target_height(target_height as i32)
            .limit_render_image_cache_size(true);

        let bitmap =
            page.render_with_config(&render_config)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let w = bitmap.width() as u32;
        let h = bitmap.height() as u32;
        let pixels = bitmap.as_rgba_bytes();

        Ok(RawBitmap {
            width: w,
            height: h,
            pixels,
        })
    }

    fn extract_text(&self, page_index: PageIndex) -> Result<String, PdfError> {
        let pages = self.doc.pages();
        let page = pages.get(page_index.get() as u16 as i32).map_err(|e| {
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
        let page = pages.get(page_index.get() as u16 as i32).map_err(|e| {
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
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| " ".to_string());
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
        let page = pages.get(page_index.get() as u16 as i32).map_err(|e| {
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

fn map_load_error(error: PdfiumError, password_supplied: bool) -> PdfError {
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
