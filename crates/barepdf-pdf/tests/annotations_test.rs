use std::path::Path;
use tempfile::tempdir;

use barepdf_core::{
    AnnotationRecord, HighlightQuad, PageCount, PageCropRect, PageIndex, PageTextGeometry,
    PdfError, Rotation,
};
use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, PdfiumEngine, RawBitmap, TextSpan};
use pdfium_render::prelude::*;

struct FakeAnnotationDocument {
    page_count: u32,
    annotations: Vec<AnnotationRecord>,
}

impl FakeAnnotationDocument {
    fn new(page_count: u32, annotations: Vec<AnnotationRecord>) -> Self {
        Self {
            page_count,
            annotations,
        }
    }
}

impl PdfDocument for FakeAnnotationDocument {
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

    fn annotations(&self, page: PageIndex) -> Result<Vec<AnnotationRecord>, PdfError> {
        let page_count = self.page_count()?.get();
        if page.get() >= page_count {
            return Err(PdfError::RenderingFailed {
                page_index: page.get(),
                reason: "Page index out of bounds".into(),
            });
        }
        Ok(self
            .annotations
            .iter()
            .filter(|record| record.page() == page)
            .cloned()
            .collect())
    }
}

#[test]
fn test_fake_document_annotations_reading() {
    let p0 = PageIndex::from_raw(0);
    let p1 = PageIndex::from_raw(1);

    let records = vec![
        AnnotationRecord::Highlight {
            page: p0,
            quads: vec![HighlightQuad {
                page: p0,
                x_norm: 0.1,
                y_norm: 0.2,
                w_norm: 0.3,
                h_norm: 0.05,
            }],
            contents: Some("Highlight contents".into()),
        },
        AnnotationRecord::Underline {
            page: p0,
            quads: vec![HighlightQuad {
                page: p0,
                x_norm: 0.15,
                y_norm: 0.25,
                w_norm: 0.2,
                h_norm: 0.04,
            }],
            contents: None,
        },
        AnnotationRecord::FreeText {
            page: p0,
            rect: PageCropRect {
                page_index: 0,
                left: 72.0,
                bottom: 100.0,
                right: 200.0,
                top: 150.0,
            },
            text: "Free text comment".into(),
        },
        AnnotationRecord::Stamp {
            page: p0,
            rect: PageCropRect {
                page_index: 0,
                left: 300.0,
                bottom: 400.0,
                right: 450.0,
                top: 500.0,
            },
        },
        AnnotationRecord::Unknown {
            page: p0,
            raw_type: "Ink".into(),
        },
        AnnotationRecord::Highlight {
            page: p1,
            quads: vec![HighlightQuad {
                page: p1,
                x_norm: 0.5,
                y_norm: 0.6,
                w_norm: 0.1,
                h_norm: 0.02,
            }],
            contents: Some("Page 2 note".into()),
        },
    ];

    let doc = FakeAnnotationDocument::new(2, records);

    // Page 0 has 5 annotations
    let p0_annots = doc.annotations(p0).expect("reads page 0 annotations");
    assert_eq!(p0_annots.len(), 5);
    assert_eq!(p0_annots[0].page(), p0);
    assert_eq!(p0_annots[1].page(), p0);
    assert_eq!(p0_annots[2].page(), p0);
    assert_eq!(p0_annots[3].page(), p0);
    assert_eq!(p0_annots[4].page(), p0);

    // Page 1 has 1 annotation
    let p1_annots = doc.annotations(p1).expect("reads page 1 annotations");
    assert_eq!(p1_annots.len(), 1);
    assert_eq!(p1_annots[0].page(), p1);

    // Out-of-bounds page fails gracefully
    let p2 = PageIndex::from_raw(2);
    assert!(doc.annotations(p2).is_err());
}

fn create_annotated_pdf_fixture(path: &Path) {
    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let mut doc = pdfium.create_new_pdf().expect("create new pdf");

    // Page 0: 600 x 800 pt
    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(
            PdfPoints::new(600.0),
            PdfPoints::new(800.0),
        ))
        .expect("create page");

    let annots = page.annotations_mut();

    // 1. Highlight annotation with attachment points
    let mut highlight = annots
        .create_highlight_annotation()
        .expect("create highlight");
    highlight
        .set_contents("Important finding")
        .expect("set highlight contents");
    highlight
        .set_bounds(PdfRect::new_from_values(680.0, 50.0, 720.0, 350.0))
        .expect("set bounds");
    highlight
        .attachment_points_mut()
        .create_attachment_point_at_end(PdfQuadPoints::new_from_values(
            50.0, 680.0, 350.0, 680.0, 350.0, 720.0, 50.0, 720.0,
        ))
        .expect("add quad");

    // 2. Underline annotation with attachment points
    let mut underline = annots
        .create_underline_annotation()
        .expect("create underline");
    underline
        .set_contents("Check reference")
        .expect("set underline contents");
    underline
        .set_bounds(PdfRect::new_from_values(580.0, 60.0, 610.0, 250.0))
        .expect("set bounds");
    underline
        .attachment_points_mut()
        .create_attachment_point_at_end(PdfQuadPoints::new_from_values(
            60.0, 580.0, 250.0, 580.0, 250.0, 610.0, 60.0, 610.0,
        ))
        .expect("add quad");

    // 3. FreeText annotation with contents and bounds
    let mut free_text = annots
        .create_free_text_annotation("Author comment")
        .expect("create free text");
    free_text
        .set_bounds(PdfRect::new_from_values(400.0, 100.0, 480.0, 300.0))
        .expect("set bounds");

    // 4. Stamp annotation with bounds
    let mut stamp = annots.create_stamp_annotation().expect("create stamp");
    stamp
        .set_bounds(PdfRect::new_from_values(200.0, 350.0, 280.0, 500.0))
        .expect("set bounds");

    // 5. Ink annotation (unknown variant for roundtrip preservation)
    let mut ink = annots.create_ink_annotation().expect("create ink");
    ink.set_bounds(PdfRect::new_from_values(50.0, 50.0, 150.0, 150.0))
        .expect("set bounds");

    // Page 1: Empty page with no annotations
    doc.pages_mut()
        .create_page_at_end(PdfPagePaperSize::Custom(
            PdfPoints::new(600.0),
            PdfPoints::new(800.0),
        ))
        .expect("create page 2");

    doc.save_to_file(path).expect("save pdf");
}

#[test]
fn test_pdfium_engine_reads_annotations_with_zero_loss_guarantee() {
    let dir = tempdir().expect("tempdir");
    let pdf_path = dir.path().join("annotated_test.pdf");
    create_annotated_pdf_fixture(&pdf_path);

    let engine = PdfiumEngine::new().expect("PDFiumEngine initializes");
    let doc = engine
        .open_path(&pdf_path, None)
        .expect("opens annotated pdf");

    // Verify page count
    assert_eq!(doc.page_count().expect("page count").get(), 2);

    // Read page 0 annotations
    let p0 = PageIndex::from_raw(0);
    let annotations = doc.annotations(p0).expect("reads annotations on page 0");

    // Exactly 5 annotations were added and must be read back (zero loss)
    assert_eq!(
        annotations.len(),
        5,
        "All 5 created annotations must be returned"
    );

    // 1. Verify Highlight
    let highlight = annotations
        .iter()
        .find(|a| matches!(a, AnnotationRecord::Highlight { .. }))
        .expect("highlight annotation found");
    match highlight {
        AnnotationRecord::Highlight {
            page,
            quads,
            contents,
        } => {
            assert_eq!(*page, p0);
            assert_eq!(contents.as_deref(), Some("Important finding"));
            assert!(!quads.is_empty(), "Highlight must have at least one quad");
            let q = &quads[0];
            assert_eq!(q.page, p0);
            assert!(q.x_norm >= 0.0 && q.x_norm <= 1.0);
            assert!(q.y_norm >= 0.0 && q.y_norm <= 1.0);
            assert!(q.w_norm > 0.0 && q.w_norm <= 1.0);
            assert!(q.h_norm > 0.0 && q.h_norm <= 1.0);
        }
        _ => unreachable!(),
    }

    // 2. Verify Underline
    let underline = annotations
        .iter()
        .find(|a| matches!(a, AnnotationRecord::Underline { .. }))
        .expect("underline annotation found");
    match underline {
        AnnotationRecord::Underline {
            page,
            quads,
            contents,
        } => {
            assert_eq!(*page, p0);
            assert_eq!(contents.as_deref(), Some("Check reference"));
            assert!(!quads.is_empty(), "Underline must have at least one quad");
            let q = &quads[0];
            assert_eq!(q.page, p0);
            assert!(q.x_norm >= 0.0 && q.x_norm <= 1.0);
            assert!(q.y_norm >= 0.0 && q.y_norm <= 1.0);
        }
        _ => unreachable!(),
    }

    // 3. Verify FreeText
    let free_text = annotations
        .iter()
        .find(|a| matches!(a, AnnotationRecord::FreeText { .. }))
        .expect("free text annotation found");
    match free_text {
        AnnotationRecord::FreeText { page, rect, text } => {
            assert_eq!(*page, p0);
            assert_eq!(text, "Author comment");
            assert_eq!(rect.page_index, 0);
            assert!((rect.left - 100.0).abs() < 1.0);
            assert!((rect.bottom - 400.0).abs() < 1.0);
            assert!((rect.right - 300.0).abs() < 1.0);
            assert!((rect.top - 480.0).abs() < 1.0);
        }
        _ => unreachable!(),
    }

    // 4. Verify Stamp
    let stamp = annotations
        .iter()
        .find(|a| matches!(a, AnnotationRecord::Stamp { .. }))
        .expect("stamp annotation found");
    match stamp {
        AnnotationRecord::Stamp { page, rect } => {
            assert_eq!(*page, p0);
            assert_eq!(rect.page_index, 0);
            assert!((rect.left - 350.0).abs() < 1.0);
            assert!((rect.bottom - 200.0).abs() < 1.0);
            assert!((rect.right - 500.0).abs() < 1.0);
            assert!((rect.top - 280.0).abs() < 1.0);
        }
        _ => unreachable!(),
    }

    // 5. Verify Ink is captured as Unknown (no loss)
    let ink = annotations
        .iter()
        .find(|a| matches!(a, AnnotationRecord::Unknown { .. }))
        .expect("ink captured as unknown");
    match ink {
        AnnotationRecord::Unknown { page, raw_type } => {
            assert_eq!(*page, p0);
            assert!(
                raw_type.contains("Ink"),
                "Raw type should identify Ink, got: {raw_type}"
            );
        }
        _ => unreachable!(),
    }

    // Page 1 has 0 annotations
    let p1 = PageIndex::from_raw(1);
    let p1_annots = doc.annotations(p1).expect("reads page 1 annotations");
    assert!(p1_annots.is_empty(), "Page 1 must have 0 annotations");
}
