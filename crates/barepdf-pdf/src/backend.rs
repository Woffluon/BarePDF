use barepdf_core::{AnnotationRecord, PageCount, PageIndex, PageTextGeometry, PdfError, Rotation};
pub use barepdf_core::{FormFieldRecord, FormFieldType, InvalidBitmap, RawBitmap};
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
    fn annotations(&self, _page: PageIndex) -> Result<Vec<AnnotationRecord>, PdfError> {
        Ok(Vec::new())
    }
    fn form_fields(&self) -> Result<Vec<FormFieldRecord>, PdfError> {
        Ok(Vec::new())
    }
    fn set_form_field_value(&mut self, _name: &str, _value: &str) -> Result<(), PdfError> {
        Ok(())
    }
    fn save_to_bytes(&self) -> Result<Vec<u8>, PdfError> {
        Ok(Vec::new())
    }
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
    use super::{PdfDocument, RawBitmap};
    use barepdf_core::{PageCount, PageIndex, PdfError, Rotation};

    struct MinimalStubDoc;
    impl PdfDocument for MinimalStubDoc {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            Ok(PageCount::new(1).unwrap())
        }
        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((100.0, 100.0))
        }
        fn render_page(
            &self,
            _page_index: PageIndex,
            _target_width: u32,
            _target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            RawBitmap::new(1, 1, vec![0; 4]).map_err(|e| PdfError::InvalidPdfReason(e.to_string()))
        }
        fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
            Ok(String::new())
        }
        fn extract_text_spans(
            &self,
            _page_index: PageIndex,
        ) -> Result<Vec<super::TextSpan>, PdfError> {
            Ok(Vec::new())
        }
        fn get_page_text_geometry(
            &self,
            page_index: PageIndex,
        ) -> Result<barepdf_core::PageTextGeometry, PdfError> {
            Ok(barepdf_core::PageTextGeometry {
                page_index,
                glyphs: Vec::new(),
                links: Vec::new(),
            })
        }
        fn get_outline(&self) -> Result<Vec<super::OutlineNode>, PdfError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn pdf_document_default_form_methods() {
        let mut doc = MinimalStubDoc;
        let fields = doc.form_fields().expect("default form_fields");
        assert!(fields.is_empty());
        assert!(doc.set_form_field_value("any", "val").is_ok());
        let bytes = doc.save_to_bytes().expect("default save_to_bytes");
        assert!(bytes.is_empty());
    }

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
