use barepdf_core::{PageCount, PageIndex, PageTextGeometry, PdfError, Rotation};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct RawBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>, // RGBA 8-bit per channel
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidBitmap;

impl fmt::Display for InvalidBitmap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid RGBA bitmap dimensions or pixel length")
    }
}

impl std::error::Error for InvalidBitmap {}

impl RawBitmap {
    /// Creates an RGBA bitmap whose byte length matches its dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidBitmap`] when either dimension is zero, the dimensions overflow, or the
    /// pixel buffer is not exactly four bytes per pixel.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, InvalidBitmap> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| usize::try_from(height).ok()?.checked_mul(width))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(InvalidBitmap)?;

        (width != 0 && height != 0 && pixels.len() == expected)
            .then_some(Self {
                width,
                height,
                pixels,
            })
            .ok_or(InvalidBitmap)
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn into_parts(self) -> (u32, u32, Vec<u8>) {
        (self.width, self.height, self.pixels)
    }
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
