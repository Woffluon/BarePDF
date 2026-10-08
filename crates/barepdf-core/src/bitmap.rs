use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;

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

    pub fn invert_rgb(&mut self) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel[0] = 255 - pixel[0];
            pixel[1] = 255 - pixel[1];
            pixel[2] = 255 - pixel[2];
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat {
    Png,
    Jpeg { quality: u8 },
}

#[derive(Debug, Clone)]
pub struct ImageEncodeError {
    reason: String,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}

impl ImageEncodeError {
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(
        reason: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            reason: reason.into(),
            source: Some(Arc::new(source)),
        }
    }

    #[must_use]
    pub fn from_io(source: io::Error) -> Self {
        let reason = source.to_string();
        Self::with_source(reason, source)
    }
}

impl From<io::Error> for ImageEncodeError {
    fn from(source: io::Error) -> Self {
        Self::from_io(source)
    }
}

impl fmt::Display for ImageEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for ImageEncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

pub trait ImageEncoder: Send + Sync {
    /// Encodes one tightly packed RGBA bitmap to a newly staged output file.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested format is unsupported or the staged file cannot be
    /// encoded completely.
    fn encode_rgba(
        &self,
        output: &Path,
        bitmap: &RawBitmap,
        format: EncodedImageFormat,
        dpi: u16,
    ) -> Result<(), ImageEncodeError>;
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

    #[test]
    fn raw_bitmap_invert_rgb_inverts_rgb_and_preserves_alpha() {
        let mut bitmap = RawBitmap::new(2, 1, vec![0, 100, 255, 128, 10, 20, 30, 255])
            .expect("valid RGBA bitmap");
        bitmap.invert_rgb();
        assert_eq!(bitmap.pixels(), &[255, 155, 0, 128, 245, 235, 225, 255]);
    }
}
