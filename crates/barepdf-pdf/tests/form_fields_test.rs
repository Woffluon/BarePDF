use barepdf_core::{
    FormFieldRecord, FormFieldType, PageCount, PageCropRect, PageIndex, PageTextGeometry, PdfError,
    Rotation,
};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, PdfiumEngine, RawBitmap, TextSpan};

struct FakeFormDocument {
    page_count: u32,
    fields: Vec<FormFieldRecord>,
}

impl FakeFormDocument {
    fn new(page_count: u32, fields: Vec<FormFieldRecord>) -> Self {
        Self { page_count, fields }
    }
}

impl PdfDocument for FakeFormDocument {
    fn page_count(&self) -> Result<PageCount, PdfError> {
        PageCount::new(self.page_count)
            .ok_or_else(|| PdfError::InvalidPdfReason("empty fake document".into()))
    }

    fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
        Ok((612.0, 792.0))
    }

    fn render_page(
        &self,
        _page_index: PageIndex,
        target_width: u32,
        target_height: u32,
        _rotation: Rotation,
    ) -> Result<RawBitmap, PdfError> {
        let bytes = (target_width as usize) * (target_height as usize) * 4;
        RawBitmap::new(target_width, target_height, vec![0; bytes])
            .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))
    }

    fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
        Ok(String::new())
    }

    fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
        Ok(Vec::new())
    }

    fn get_page_text_geometry(&self, page_index: PageIndex) -> Result<PageTextGeometry, PdfError> {
        Ok(PageTextGeometry {
            page_index,
            glyphs: Vec::new(),
            links: Vec::new(),
        })
    }

    fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
        Ok(Vec::new())
    }

    fn form_fields(&self) -> Result<Vec<FormFieldRecord>, PdfError> {
        Ok(self.fields.clone())
    }

    fn set_form_field_value(&mut self, name: &str, value: &str) -> Result<(), PdfError> {
        let field = self
            .fields
            .iter_mut()
            .find(|f| f.name == name)
            .ok_or_else(|| PdfError::InvalidPdfReason(format!("Form field '{name}' not found")))?;

        if field.is_read_only {
            return Err(PdfError::InvalidPdfReason(format!(
                "Form field '{name}' is read-only"
            )));
        }

        field.value = Some(value.to_string());
        Ok(())
    }

    fn save_to_bytes(&self) -> Result<Vec<u8>, PdfError> {
        Ok(b"%PDF-FAKE-SAVED".to_vec())
    }
}

#[test]
fn test_fake_document_form_fields_reading_and_updating() {
    let p0 = PageIndex::from_raw(0);
    let p1 = PageIndex::from_raw(1);

    let bounds = PageCropRect {
        page_index: 0,
        left: 50.0,
        bottom: 700.0,
        right: 250.0,
        top: 730.0,
    };

    let fields = vec![
        FormFieldRecord::new(
            "first_name",
            FormFieldType::Text,
            p0,
            bounds,
            Some("Alice".into()),
            false,
        ),
        FormFieldRecord::new(
            "terms_agreed",
            FormFieldType::Checkbox,
            p0,
            bounds,
            Some("true".into()),
            false,
        ),
        FormFieldRecord::new(
            "system_id",
            FormFieldType::Text,
            p0,
            bounds,
            Some("SYS-123".into()),
            true,
        ),
        FormFieldRecord::new("feedback", FormFieldType::Text, p1, bounds, None, false),
    ];

    let mut doc = FakeFormDocument::new(2, fields);

    // Read form fields
    let initial_fields = doc.form_fields().expect("reads form fields");
    assert_eq!(initial_fields.len(), 4);
    assert_eq!(initial_fields[0].name, "first_name");
    assert_eq!(initial_fields[0].value.as_deref(), Some("Alice"));
    assert_eq!(initial_fields[1].field_type, FormFieldType::Checkbox);
    assert!(initial_fields[2].is_read_only);

    // Update mutable field
    doc.set_form_field_value("first_name", "Bob")
        .expect("updates field");
    let updated_fields = doc.form_fields().expect("reads updated fields");
    assert_eq!(updated_fields[0].value.as_deref(), Some("Bob"));

    // Updating read-only field fails
    let ro_err = doc.set_form_field_value("system_id", "HACKED");
    assert!(ro_err.is_err());
    let err_msg = ro_err.unwrap_err().to_string();
    assert!(err_msg.contains("read-only"));

    // Updating non-existent field fails
    let not_found_err = doc.set_form_field_value("non_existent", "val");
    assert!(not_found_err.is_err());

    // Save to bytes
    let saved = doc.save_to_bytes().expect("saves to bytes");
    assert!(!saved.is_empty());
}

fn create_acroform_pdf_bytes() -> Vec<u8> {
    let pdf = "%PDF-1.4\n\
1 0 obj\n\
<<\n\
  /Type /Catalog\n\
  /Pages 2 0 R\n\
  /AcroForm <<\n\
    /Fields [3 0 R 4 0 R]\n\
    /NeedAppearances true\n\
  >>\n\
>>\n\
endobj\n\
2 0 obj\n\
<<\n\
  /Type /Pages\n\
  /Kids [5 0 R]\n\
  /Count 1\n\
>>\n\
endobj\n\
3 0 obj\n\
<<\n\
  /Type /Annot\n\
  /Subtype /Widget\n\
  /FT /Tx\n\
  /T (username)\n\
  /V (initial_user)\n\
  /Rect [50 700 250 730]\n\
  /P 5 0 R\n\
  /F 4\n\
>>\n\
endobj\n\
4 0 obj\n\
<<\n\
  /Type /Annot\n\
  /Subtype /Widget\n\
  /FT /Btn\n\
  /T (newsletter)\n\
  /V /Yes\n\
  /AS /Yes\n\
  /Rect [50 650 70 670]\n\
  /P 5 0 R\n\
  /F 4\n\
>>\n\
endobj\n\
5 0 obj\n\
<<\n\
  /Type /Page\n\
  /Parent 2 0 R\n\
  /MediaBox [0 0 600 800]\n\
  /Annots [3 0 R 4 0 R]\n\
>>\n\
endobj\n\
xref\n\
0 6\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000109 00000 n \n\
0000000168 00000 n \n\
0000000282 00000 n \n\
0000000392 00000 n \n\
trailer\n\
<<\n\
  /Size 6\n\
  /Root 1 0 R\n\
>>\n\
startxref\n\
484\n\
%%EOF\n";
    pdf.as_bytes().to_vec()
}

#[test]
fn test_pdfium_engine_form_fields_read_write_roundtrip() {
    let pdf_bytes = create_acroform_pdf_bytes();

    let engine = PdfiumEngine::new().expect("PDFiumEngine initializes");
    let mut doc = engine
        .open_bytes(pdf_bytes, None)
        .expect("opens acroform pdf from bytes");

    assert_eq!(doc.page_count().expect("page count").get(), 1);

    // Read initial form fields
    let fields = doc.form_fields().expect("reads form fields");
    assert!(
        fields.len() >= 2,
        "Expected at least 2 form fields, found: {}",
        fields.len()
    );

    let user_field = fields
        .iter()
        .find(|f| f.name == "username")
        .expect("username field exists");
    assert_eq!(user_field.field_type, FormFieldType::Text);
    assert_eq!(user_field.value.as_deref(), Some("initial_user"));
    assert_eq!(user_field.page_index.get(), 0);

    let newsletter_field = fields
        .iter()
        .find(|f| f.name == "newsletter")
        .expect("newsletter field exists");
    assert_eq!(newsletter_field.field_type, FormFieldType::Checkbox);

    // Update field value
    doc.set_form_field_value("username", "updated_user_42")
        .expect("sets username value");

    // Toggle checkbox
    doc.set_form_field_value("newsletter", "false")
        .expect("sets newsletter value");

    // Save modified document to bytes (Roundtrip)
    let saved_bytes = doc.save_to_bytes().expect("saves modified document");
    assert!(!saved_bytes.is_empty(), "Saved bytes must not be empty");

    // Open saved document with engine to verify persistence
    let reloaded_doc = engine
        .open_bytes(saved_bytes, None)
        .expect("re-opens saved document from bytes");

    let reloaded_fields = reloaded_doc
        .form_fields()
        .expect("reads form fields on reloaded document");

    let reloaded_user = reloaded_fields
        .iter()
        .find(|f| f.name == "username")
        .expect("username field exists on reloaded doc");
    assert_eq!(
        reloaded_user.value.as_deref(),
        Some("updated_user_42"),
        "Updated form field value must persist across save-load roundtrip"
    );

    let reloaded_newsletter = reloaded_fields
        .iter()
        .find(|f| f.name == "newsletter")
        .expect("newsletter field exists on reloaded doc");
    assert_eq!(
        reloaded_newsletter.value.as_deref(),
        Some("false"),
        "Updated checkbox value must persist across roundtrip"
    );
}

#[test]
fn test_pdfium_engine_set_non_existent_field_returns_error() {
    let pdf_bytes = create_acroform_pdf_bytes();
    let engine = PdfiumEngine::new().expect("PDFiumEngine initializes");
    let mut doc = engine
        .open_bytes(pdf_bytes, None)
        .expect("opens acroform pdf");

    let res = doc.set_form_field_value("does_not_exist", "some_value");
    assert!(res.is_err(), "Setting non-existent field must return error");
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("not found"));
}
