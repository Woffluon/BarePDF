use crate::backend::{
    OutlineNode, PdfBackend, PdfDocument as CorePdfDocument, RawBitmap, TextSpan,
};
use crate::pdfium_lifetime::process_pdfium;
use barepdf_core::{
    AnnotationRecord, FormFieldRecord, FormFieldType, HighlightQuad, PageCount, PageCropRect,
    PageIndex, PdfError, Rotation, SecretPassword, MAX_OUTLINE_DEPTH, MAX_OUTLINE_ITEMS,
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
        process_pdfium()
            .map(|pdfium| Self { pdfium })
            .inspect_err(|error| {
                tracing::warn!(%error, "Failed to initialize PDFium engine");
            })
    }
}

pub struct PdfiumDocumentOwned {
    doc: PdfDocument<'static>,
}

impl PdfiumDocumentOwned {
    /// Serializes and saves the loaded document into a byte vector.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if serialization fails within PDFium.
    pub fn save_to_bytes(&self) -> Result<Vec<u8>, PdfError> {
        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        self.doc.save_to_bytes().map_err(|error| PdfError::Backend {
            source: Arc::new(error),
        })
    }
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
        let (target_width, target_height) =
            barepdf_core::limits::sanitize_render_dimensions(target_width, target_height).map_err(
                |error| {
                    tracing::warn!(
                        page_index = page_index.as_u32(),
                        target_width,
                        target_height,
                        "Rejected out-of-bounds render dimensions"
                    );
                    match error {
                        PdfError::RenderingFailed { reason, .. } => PdfError::RenderingFailed {
                            page_index: page_index.as_u32(),
                            reason,
                        },
                        other => PdfError::RenderingFailed {
                            page_index: page_index.as_u32(),
                            reason: other.to_string(),
                        },
                    }
                },
            )?;
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

        let bitmap = page.render_with_config(&render_config).map_err(|e| {
            tracing::warn!(
                page_index = page_index.as_u32(),
                target_width,
                target_height,
                "PDFium page render failed"
            );
            PdfError::RenderingFailed {
                page_index: page_index.get(),
                reason: e.to_string(),
            }
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

    fn annotations(&self, page_index: PageIndex) -> Result<Vec<AnnotationRecord>, PdfError> {
        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pages = self.doc.pages();
        let page =
            pages
                .get(to_pdfium_index(page_index)?)
                .map_err(|e| PdfError::RenderingFailed {
                    page_index: page_index.get(),
                    reason: e.to_string(),
                })?;

        let page_w = page.width().value;
        let page_h = page.height().value;
        let annots = page.annotations();
        let mut records = Vec::with_capacity(annots.len());

        for annot in annots.iter() {
            match annot.annotation_type() {
                PdfPageAnnotationType::Highlight => {
                    let quads = extract_quads_from_annotation(&annot, page_index, page_w, page_h);
                    let contents = annot.contents();
                    records.push(AnnotationRecord::Highlight {
                        page: page_index,
                        quads,
                        contents,
                    });
                }
                PdfPageAnnotationType::Underline => {
                    let quads = extract_quads_from_annotation(&annot, page_index, page_w, page_h);
                    let contents = annot.contents();
                    records.push(AnnotationRecord::Underline {
                        page: page_index,
                        quads,
                        contents,
                    });
                }
                PdfPageAnnotationType::FreeText => {
                    let rect = extract_crop_rect(&annot, page_index);
                    let text = annot.contents().unwrap_or_default();
                    records.push(AnnotationRecord::FreeText {
                        page: page_index,
                        rect,
                        text,
                    });
                }
                PdfPageAnnotationType::Stamp => {
                    let rect = extract_crop_rect(&annot, page_index);
                    records.push(AnnotationRecord::Stamp {
                        page: page_index,
                        rect,
                    });
                }
                other => {
                    records.push(AnnotationRecord::Unknown {
                        page: page_index,
                        raw_type: format!("{other:?}"),
                    });
                }
            }
        }

        Ok(records)
    }

    fn form_fields(&self) -> Result<Vec<FormFieldRecord>, PdfError> {
        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pages = self.doc.pages();
        let page_count = pages.len();
        let mut records = Vec::new();

        for page_idx in 0..page_count {
            let page = match pages.get(page_idx) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(page_idx, error = %e, "Failed to load page while scanning form fields");
                    continue;
                }
            };
            let core_page_index = PageIndex::from_raw(page_idx as u32);
            let annotations = page.annotations();

            for annot in annotations.iter() {
                let is_widget = annot.annotation_type() == PdfPageAnnotationType::Widget
                    || annot.annotation_type() == PdfPageAnnotationType::XfaWidget;
                let form_field_opt = annot.as_form_field();

                if !is_widget && form_field_opt.is_none() {
                    continue;
                }

                let bounds = extract_crop_rect(&annot, core_page_index);

                if let Some(field) = form_field_opt {
                    let name = field.name().unwrap_or_default();
                    let is_read_only = field.is_read_only();
                    let (field_type, value) = match field.field_type() {
                        PdfFormFieldType::Text => {
                            let val = field.as_text_field().and_then(|t| t.value());
                            (FormFieldType::Text, val)
                        }
                        PdfFormFieldType::Checkbox => {
                            let val = field.as_checkbox_field().map(|cb| {
                                if cb.is_checked().unwrap_or(false) {
                                    "true".to_string()
                                } else {
                                    "false".to_string()
                                }
                            });
                            (FormFieldType::Checkbox, val)
                        }
                        PdfFormFieldType::RadioButton => {
                            let val = field.as_radio_button_field().map(|rb| {
                                if rb.is_checked().unwrap_or(false) {
                                    rb.group_value().unwrap_or_else(|| "true".to_string())
                                } else {
                                    "false".to_string()
                                }
                            });
                            (FormFieldType::RadioButton, val)
                        }
                        PdfFormFieldType::ComboBox => {
                            let val = field.as_combo_box_field().and_then(|cb| cb.value());
                            (FormFieldType::ComboBox, val)
                        }
                        PdfFormFieldType::ListBox => {
                            let val = field.as_list_box_field().and_then(|lb| lb.value());
                            (FormFieldType::ListBox, val)
                        }
                        PdfFormFieldType::Signature => (FormFieldType::Signature, None),
                        PdfFormFieldType::PushButton => {
                            (FormFieldType::Unknown("PushButton".to_string()), None)
                        }
                        PdfFormFieldType::Unknown => {
                            (FormFieldType::Unknown("Unknown".to_string()), None)
                        }
                    };

                    records.push(FormFieldRecord {
                        name,
                        field_type,
                        page_index: core_page_index,
                        bounds,
                        value,
                        is_read_only,
                    });
                } else {
                    let name = annot.name().unwrap_or_default();
                    let raw_type = format!("{:?}", annot.annotation_type());
                    records.push(FormFieldRecord {
                        name,
                        field_type: FormFieldType::Unknown(raw_type),
                        page_index: core_page_index,
                        bounds,
                        value: None,
                        is_read_only: false,
                    });
                }
            }
        }

        Ok(records)
    }

    fn set_form_field_value(&mut self, name: &str, value: &str) -> Result<(), PdfError> {
        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pages = self.doc.pages();
        let page_count = pages.len();
        let mut matched = false;

        for page_idx in 0..page_count {
            let page = match pages.get(page_idx) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(page_idx, error = %e, "Failed to load page while updating form field");
                    continue;
                }
            };

            let annotations = page.annotations();
            let annot_count = annotations.len();

            for annot_idx in 0..annot_count {
                let mut annot = match annotations.get(annot_idx) {
                    Ok(a) => a,
                    Err(_) => continue,
                };

                let Some(mut field) = annot.as_form_field_mut() else {
                    continue;
                };

                if field.name().as_deref() == Some(name) {
                    matched = true;
                    if field.is_read_only() {
                        return Err(PdfError::InvalidPdfReason(format!(
                            "Form field '{name}' is read-only"
                        )));
                    }

                    match &mut field {
                        PdfFormField::Text(text_field) => {
                            text_field
                                .set_value(value)
                                .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;
                        }
                        PdfFormField::Checkbox(checkbox_field) => {
                            let is_checked = value.eq_ignore_ascii_case("true")
                                || value == "1"
                                || value.eq_ignore_ascii_case("yes")
                                || value.eq_ignore_ascii_case("checked");
                            checkbox_field
                                .set_checked(is_checked)
                                .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;
                        }
                        PdfFormField::RadioButton(radio_field) => {
                            let is_checked = value.eq_ignore_ascii_case("true")
                                || value == "1"
                                || value.eq_ignore_ascii_case("yes")
                                || radio_field.group_value().as_deref() == Some(value)
                                || radio_field.appearance_stream().as_deref() == Some(value);
                            if is_checked {
                                radio_field
                                    .set_checked()
                                    .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;
                            }
                        }
                        other => {
                            tracing::warn!(
                                name,
                                field_type = ?other.field_type(),
                                "Attempted to set value on unsupported form field type"
                            );
                            return Err(PdfError::InvalidPdfReason(format!(
                                "Setting value for form field '{name}' of type {:?} is not supported",
                                other.field_type()
                            )));
                        }
                    }
                }
            }
        }

        if !matched {
            return Err(PdfError::InvalidPdfReason(format!(
                "Form field '{name}' not found"
            )));
        }

        Ok(())
    }

    fn save_to_bytes(&self) -> Result<Vec<u8>, PdfError> {
        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        self.doc.save_to_bytes().map_err(|error| PdfError::Backend {
            source: Arc::new(error),
        })
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
            tracing::debug!(
                password_supplied,
                "PDFium document open requires or rejected password"
            );
            if password_supplied {
                PdfError::IncorrectPassword
            } else {
                PdfError::PasswordRequired
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            tracing::warn!("PDFium reported file access error while opening document");
            PdfError::FileAccess {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            tracing::warn!("PDFium reported invalid PDF format error while opening document");
            PdfError::InvalidPdf {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            tracing::warn!("PDFium reported unsupported encryption while opening document");
            PdfError::UnsupportedEncryption {
                source: Arc::new(error),
            }
        }
        error => {
            tracing::warn!("PDFium reported backend error while opening document");
            PdfError::Backend {
                source: Arc::new(error),
            }
        }
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

fn extract_quads_from_annotation(
    annot: &PdfPageAnnotation<'_>,
    page: PageIndex,
    page_w: f32,
    page_h: f32,
) -> Vec<HighlightQuad> {
    let pw = page_w.max(1.0);
    let ph = page_h.max(1.0);
    let mut quads = Vec::new();

    let points = annot.attachment_points();
    for i in 0..points.len() {
        if let Ok(qp) = points.get(i) {
            let left = qp.left().value.min(qp.right().value);
            let right = qp.left().value.max(qp.right().value);
            let bottom = qp.bottom().value.min(qp.top().value);
            let top = qp.bottom().value.max(qp.top().value);

            let x_norm = (left / pw).clamp(0.0, 1.0);
            let y_norm = ((ph - top) / ph).clamp(0.0, 1.0);
            let w_norm = ((right - left) / pw).clamp(0.0001, 1.0 - x_norm);
            let h_norm = ((top - bottom) / ph).clamp(0.0001, 1.0 - y_norm);

            quads.push(HighlightQuad {
                page,
                x_norm,
                y_norm,
                w_norm,
                h_norm,
            });
        }
    }

    if quads.is_empty() {
        if let Ok(bounds) = annot.bounds() {
            let left = bounds.left().value.min(bounds.right().value);
            let right = bounds.left().value.max(bounds.right().value);
            let bottom = bounds.bottom().value.min(bounds.top().value);
            let top = bounds.bottom().value.max(bounds.top().value);

            let x_norm = (left / pw).clamp(0.0, 1.0);
            let y_norm = ((ph - top) / ph).clamp(0.0, 1.0);
            let w_norm = ((right - left) / pw).clamp(0.0001, 1.0 - x_norm);
            let h_norm = ((top - bottom) / ph).clamp(0.0001, 1.0 - y_norm);

            quads.push(HighlightQuad {
                page,
                x_norm,
                y_norm,
                w_norm,
                h_norm,
            });
        }
    }

    quads
}

fn extract_crop_rect(annot: &PdfPageAnnotation<'_>, page: PageIndex) -> PageCropRect {
    let (left, bottom, right, top) = if let Ok(bounds) = annot.bounds() {
        (
            bounds.left().value.min(bounds.right().value),
            bounds.bottom().value.min(bounds.top().value),
            bounds.left().value.max(bounds.right().value),
            bounds.bottom().value.max(bounds.top().value),
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    PageCropRect {
        page_index: page.get() as usize,
        left,
        bottom,
        right,
        top,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        coalesce_glyphs_into_spans, validate_glyph_count, validate_loaded_page_count,
        validate_outline_limits, RawGlyph, ZeroizingFfiPassword, MAX_TEXT_GLYPHS_PER_PAGE,
    };
    use barepdf_core::limits::{sanitize_render_dimensions, MAX_DOCUMENT_PAGES};
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
    fn render_dimensions_reject_zero_and_oversized_bounds() {
        assert!(matches!(
            sanitize_render_dimensions(8193, 100),
            Err(PdfError::RenderingFailed { .. })
        ));
        assert!(matches!(
            sanitize_render_dimensions(0, 100),
            Err(PdfError::RenderingFailed { .. })
        ));
        assert_eq!(sanitize_render_dimensions(8192, 100).unwrap(), (8192, 100));
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
