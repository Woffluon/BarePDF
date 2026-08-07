use barepdf_core::{PageCount, PageIndex, PdfError, Rotation};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct RawBitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA 8-bit per channel
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineNode {
    pub title: String,
    pub page_index: Option<u32>,
    pub children: Vec<OutlineNode>,
}

pub trait PdfDocument: Send + Sync {
    fn page_count(&self) -> PageCount;
    fn page_dimensions(&self, page_index: PageIndex) -> Result<(f32, f32), PdfError>;
    fn all_page_dimensions(&self) -> Result<Vec<(f32, f32)>, PdfError> {
        let count = self.page_count().get();
        let mut dims = Vec::with_capacity(count as usize);
        for i in 0..count {
            let dim = self.page_dimensions(PageIndex::from_raw(i))?;
            dims.push(dim);
        }
        Ok(dims)
    }
    fn render_page(
        &self,
        page_index: PageIndex,
        target_width: u32,
        target_height: u32,
        rotation: Rotation,
    ) -> Result<RawBitmap, PdfError>;
    fn extract_text(&self, page_index: PageIndex) -> Result<String, PdfError>;
    fn extract_text_spans(&self, page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError>;
    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError>;
}

pub trait PdfBackend: Send + Sync {
    fn open_path(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError>;
    fn open_bytes(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<Box<dyn PdfDocument>, PdfError>;
}
