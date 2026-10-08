pub use barepdf_core::{InvalidBitmap, RawBitmap};
use barepdf_core::{PageCount, PageIndex, PageTextGeometry, PdfError, Rotation};
use std::path::Path;

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

#[allow(clippy::missing_errors_doc)] // Each implementation maps backend-specific failures to PdfError.
pub trait PdfDocument: Send {
    fn page_count(&self) -> Result<PageCount, PdfError>;
    fn page_dimensions(&self, page_index: PageIndex) -> Result<(f32, f32), PdfError>;
    fn all_page_dimensions(&self) -> Result<Vec<(f32, f32)>, PdfError> {
        let count = self.page_count()?.get();
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
    fn get_page_text_geometry(&self, page_index: PageIndex) -> Result<PageTextGeometry, PdfError>;
    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError>;
}

#[allow(clippy::missing_errors_doc)] // Each implementation maps backend-specific failures to PdfError.
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

#[cfg(test)]
mod tests {
    use super::RawBitmap;

    #[test]
    fn raw_bitmap_constructor_rejects_invalid_rgba_layouts() {
        assert!(RawBitmap::new(0, 1, Vec::new()).is_err());
        assert!(RawBitmap::new(1, 0, Vec::new()).is_err());
        assert!(RawBitmap::new(2, 2, vec![0; 15]).is_err());
        assert!(RawBitmap::new(u32::MAX, u32::MAX, Vec::new()).is_err());
    }

    #[test]
    fn raw_bitmap_constructor_exposes_valid_rgba_layout() {
        let bitmap = RawBitmap::new(2, 1, vec![0; 8]).expect("valid RGBA bitmap");

        assert_eq!(bitmap.width(), 2);
        assert_eq!(bitmap.height(), 1);
        assert_eq!(bitmap.pixels(), &[0; 8]);
    }

    #[test]
    fn raw_bitmap_parts_reuse_the_original_pixel_allocation() {
        let bitmap = RawBitmap::new(2, 1, vec![0; 8]).expect("valid RGBA bitmap");
        let original_pixels = bitmap.pixels().as_ptr();
        let (width, height, pixels) = bitmap.into_parts();

        assert_eq!((width, height), (2, 1));
        assert_eq!(pixels.as_ptr(), original_pixels);
    }
}
