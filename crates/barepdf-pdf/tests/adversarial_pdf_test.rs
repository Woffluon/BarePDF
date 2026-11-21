#![forbid(unsafe_code)]

use barepdf_core::{
    AnnotationRecord, PageIndex, PdfError, Rotation, MAX_ANNOTATIONS_PER_PAGE,
    MAX_ANNOTATION_QUADS, MAX_FORM_FIELDS_PER_DOCUMENT, MAX_FORM_FIELD_VALUE_BYTES,
    MAX_OUTLINE_DEPTH, MAX_PASSWORD_BYTES, MAX_SAFE_RENDER_DIMENSION,
};
use barepdf_pdf::backend::DocumentCapabilities;
use barepdf_pdf::{PdfBackend, PdfDocument, PdfiumEngine};

fn build_pdf(objects: &[&str], root_id: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (idx, body) in objects.iter().enumerate() {
        let obj_id = idx + 1;
        offsets.push(out.len());
        out.extend_from_slice(format!("{obj_id} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    let xref_offset = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {size} /Root {root_id} 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
        )
        .as_bytes(),
    );
    out
}

fn minimal_single_page_pdf(media_box: &str) -> Vec<u8> {
    let page = format!("<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox {media_box} >>");
    build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            &page,
        ],
        1,
    )
}

fn decode_base64(value: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        let chunk = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(chunk);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits) - 1;
        }
    }
    output
}

fn encrypted_single_page_pdf_bytes() -> Vec<u8> {
    const PDF_B64: &str = "JVBERi0xLjMKJeLjz9MKMSAwIG9iago8PAovUHJvZHVjZXIgPDhjOGU0MjU5ZDc+Cj4+CmVuZG9iagoyIDAgb2JqCjw8Ci9UeXBlIC9QYWdlcwovQ291bnQgMQovS2lkcyBbIDQgMCBSIF0KPj4KZW5kb2JqCjMgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDIgMCBSCj4+CmVuZG9iago0IDAgb2JqCjw8Ci9UeXBlIC9QYWdlCi9SZXNvdXJjZXMgPDwKPj4KL01lZGlhQm94IFsgMC4wIDAuMCA3MiA3MiBdCi9QYXJlbnQgMiAwIFIKPj4KZW5kb2JqCjUgMCBvYmoKPDwKL1YgMgovUiAzCi9MZW5ndGggMTI4Ci9QIDQyOTQ5NjcyOTIKL0ZpbHRlciAvU3RhbmRhcmQKL08gPDBjYzhjMzkyODQ4YzY0NTA5YTc1Zjk2ZjkwOTQ0MDk4NTZiNWJmYTRlMjA2ZDM5ZjNkYTQ3NjZkMzVhNDQzZTA+Ci9VIDwwMWQ5M2FhOGRhODk2ZDdmNmFkYjJlNmVhYTNlZjlmMDI4YmY0ZTVlNGU3NThhNDE2NDAwNGU1NmZmZmEwMTA4Pgo+PgplbmRvYmoKeHJlZgowIDYKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDE1IDAwMDAwIG4gCjAwMDAwMDAwNTkgMDAwMDAgbiAKMDAwMDAwMDExOCAwMDAwMCBuIAowMDAwMDAwMTY3IDAwMDAwIG4gCjAwMDAwMDAyNTkgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA2Ci9Sb290IDMgMCBSCi9JbmZvIDEgMCBSCi9JRCBbIDw2NDY2NjM2MTY2MzUzNDMyMzczOTMwMzMzMjMwMzY2NDY0MzEzMTM0MzQzMTY0MzA2MjM1NjI2MTM5MzYzMTYyPiA8NjQ2NjM2MTY2MzUzNDMyMzczOTMwMzMzMjMwMzY2NDY0MzEzMTM0MzQzMTY0MzA2MjM1NjI2MTM5MzYzMTYyPiBdCi9FbmNyeXB0IDUgMCBSCj4+CnN0YXJ0eHJlZgo0NzQKJSVFT0YK";
    decode_base64(PDF_B64)
}

fn exercise_loaded_document(doc: &dyn PdfDocument) {
    let _ = doc.page_count();
    let _ = doc.page_dimensions(PageIndex::zero());
    let _ = doc.render_page(PageIndex::zero(), 64, 64, Rotation::Degrees0);
    let _ = doc.extract_text(PageIndex::zero());
    let _ = doc.get_outline();
}

#[test]
fn empty_and_whitespace_bytes_return_err_without_panicking() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    for payload in [Vec::new(), b"   \r\n\t  ".to_vec()] {
        let result = engine.open_bytes(payload, None);
        assert!(result.is_err(), "empty or whitespace payload must fail");
        assert!(
            !matches!(result, Err(PdfError::PlatformError(_))),
            "empty payload must fail in PDF parsing, not library binding"
        );
    }
}

#[test]
fn corrupted_header_and_random_bytes_return_err() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let all_byte_values: Vec<u8> = (0..=255_u8).collect();
    let inputs = [
        b"NOT_A_PDF_HEADER_0123456789".to_vec(),
        b"%PDF-1.7\n\x00\xff\xfe\xfdcorrupted_binary_payload".to_vec(),
        all_byte_values,
    ];

    for payload in inputs {
        let result = engine.open_bytes(payload, None);
        assert!(
            result.is_err(),
            "corrupted header or binary garbage must return Err"
        );
        assert!(!matches!(result, Err(PdfError::PlatformError(_))));
    }
}

#[test]
fn corrupted_xref_table_does_not_panic() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let malformed_xref_only = b"%PDF-1.4\n\
        1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
        xref\n0 99\n9999999999 00000 n \nnot_an_xref_entry\n\
        trailer\n<< /Size 99 /Root 1 0 R >>\nstartxref\n99999999\n%%EOF\n"
        .to_vec();
    let res = engine.open_bytes(malformed_xref_only, None);
    assert!(
        res.is_err(),
        "PDF with bogus xref and missing page tree must fail"
    );

    let valid = minimal_single_page_pdf("[0 0 200 200]");
    let valid_text = String::from_utf8_lossy(&valid);
    if let Some(xref_idx) = valid_text.find("xref\n") {
        let mut damaged = valid[..xref_idx].to_vec();
        damaged.extend_from_slice(
            b"xref\n0 4\n0000000000 65535 f \n9999999999 00000 n \n8888888888 00000 n \n7777777777 00000 n \n\
              trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n999999999\n%%EOF\n",
        );
        if let Ok(doc) = engine.open_bytes(damaged, None) {
            exercise_loaded_document(doc.as_ref());
        }
    }
}

#[test]
fn truncated_pdf_sequences_do_not_panic() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let valid = minimal_single_page_pdf("[0 0 200 200]");
    let strict_short_cuts = [1, 4, 8, 16];
    for cut in strict_short_cuts {
        let prefix = valid[..cut.min(valid.len())].to_vec();
        let res = engine.open_bytes(prefix, None);
        assert!(res.is_err(), "truncated header ({cut} bytes) must fail");
    }

    let mid_cuts = [
        valid.len() / 4,
        valid.len() / 2,
        valid.len().saturating_sub(32),
        valid.len().saturating_sub(10),
        valid.len().saturating_sub(1),
    ];
    for cut in mid_cuts {
        let prefix = valid[..cut].to_vec();
        if let Ok(doc) = engine.open_bytes(prefix, None) {
            exercise_loaded_document(doc.as_ref());
        }
    }
}

#[test]
fn zero_page_and_corrupted_page_tree_return_err() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let zero_pages = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [] /Count 0 >>",
        ],
        1,
    );
    let missing_pages = build_pdf(&["<< /Type /Catalog >>"], 1);
    let lying_count = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [] /Count 5 >>",
        ],
        1,
    );
    let cyclic_pages = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [2 0 R] /Count 1 >>",
        ],
        1,
    );
    let dangling_kid = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [999 0 R] /Count 1 >>",
        ],
        1,
    );

    for (label, bytes) in [("zero_pages", zero_pages), ("missing_pages", missing_pages)] {
        let res = engine.open_bytes(bytes, None);
        assert!(res.is_err(), "{label} must return Err");
    }

    for bytes in [lying_count, cyclic_pages, dangling_kid] {
        if let Ok(doc) = engine.open_bytes(bytes, None) {
            exercise_loaded_document(doc.as_ref());
        }
    }
}

#[test]
fn render_page_dimension_bounds_and_invalid_page_index() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let pdf = minimal_single_page_pdf("[0 0 200 200]");
    let doc = engine
        .open_bytes(pdf, None)
        .expect("opens valid 1-page PDF");

    for (w, h) in [
        (0, 64),
        (64, 0),
        (0, 0),
        (MAX_SAFE_RENDER_DIMENSION + 1, 64),
        (64, MAX_SAFE_RENDER_DIMENSION + 1),
        (u32::MAX, u32::MAX),
    ] {
        let res = doc.render_page(PageIndex::zero(), w, h, Rotation::Degrees0);
        assert!(
            matches!(res, Err(PdfError::RenderingFailed { page_index: 0, .. })),
            "render_page({w}, {h}) must fail with RenderingFailed"
        );
    }

    let tiny = doc
        .render_page(PageIndex::zero(), 1, 1, Rotation::Degrees0)
        .expect("1x1 minimum safe render succeeds");
    assert_eq!(tiny.width(), 1);
    assert_eq!(tiny.height(), 1);
    assert_eq!(tiny.pixels().len(), 4);

    assert!(
        PageIndex::try_from_raw(u32::MAX).is_err(),
        "raw u32::MAX must be rejected by PageIndex::try_from_raw"
    );

    for invalid_idx in [
        PageIndex::from_raw(1),
        PageIndex::from_raw(999),
        PageIndex::clamped_from_raw(u32::MAX),
    ] {
        assert!(doc
            .render_page(invalid_idx, 64, 64, Rotation::Degrees0)
            .is_err());
        assert!(doc.page_dimensions(invalid_idx).is_err());
        assert!(doc.extract_text(invalid_idx).is_err());
        assert!(doc.extract_text_spans(invalid_idx).is_err());
        assert!(doc.get_page_text_geometry(invalid_idx).is_err());
        assert!(doc.annotations(invalid_idx).is_err());
    }
}

#[test]
fn abnormal_mediabox_pages_render_safely_without_panic() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    for media_box in [
        "[0 0 0 0]",
        "[0 0 -100 -200]",
        "[0 0 10000000 10000000]",
        "[0 0 0.01 0.01]",
    ] {
        let pdf = minimal_single_page_pdf(media_box);
        if let Ok(doc) = engine.open_bytes(pdf, None) {
            let _ = doc.page_dimensions(PageIndex::zero());
            if let Ok(bitmap) = doc.render_page(PageIndex::zero(), 64, 64, Rotation::Degrees0) {
                assert!(bitmap.width() <= 64);
                assert!(bitmap.height() <= 64);
                assert_eq!(
                    bitmap.pixels().len(),
                    (bitmap.width() as usize) * (bitmap.height() as usize) * 4
                );
            }
            if let Ok(tiny) = doc.render_page(PageIndex::zero(), 1, 1, Rotation::Degrees0) {
                assert_eq!(tiny.width(), 1);
                assert_eq!(tiny.height(), 1);
                assert_eq!(tiny.pixels().len(), 4);
            }
        }
    }
}

#[test]
fn nul_byte_and_unicode_passwords_do_not_panic() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let unencrypted = minimal_single_page_pdf("[0 0 200 200]");
    let encrypted = encrypted_single_page_pdf_bytes();

    for nul_password in ["\0", "pass\0word", "secret\0"] {
        assert!(
            matches!(
                engine.open_bytes(unencrypted.clone(), Some(nul_password)),
                Err(PdfError::IncorrectPassword)
            ),
            "NUL byte password on unencrypted PDF must return IncorrectPassword"
        );
        assert!(
            matches!(
                engine.open_bytes(encrypted.clone(), Some(nul_password)),
                Err(PdfError::IncorrectPassword)
            ),
            "NUL byte password on encrypted PDF must return IncorrectPassword"
        );
    }

    let oversized = "x".repeat(MAX_PASSWORD_BYTES * 4);
    for password in [
        "",
        "sifre-parola-\u{015f}\u{0131}-\u{65e5}\u{672c}\u{8a9e}-\u{1f511}",
        &oversized,
    ] {
        let enc_res = engine.open_bytes(encrypted.clone(), Some(password));
        assert!(
            matches!(
                enc_res,
                Err(PdfError::IncorrectPassword | PdfError::UnsupportedEncryption { .. })
            ),
            "wrong empty/unicode/oversized password on encrypted PDF must fail safely"
        );

        let _ = engine.open_bytes(unencrypted.clone(), Some(password));
    }
}

#[test]
fn deep_and_cyclic_outlines_respect_bounds_without_panic() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");

    let chain_len = MAX_OUTLINE_DEPTH + 5;
    let mut deep_objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>".to_string(),
    ];
    for i in 0..chain_len {
        let parent_id = if i == 0 { 4 } else { 5 + i - 1 };
        if i + 1 < chain_len {
            let child_id = 5 + i + 1;
            deep_objects.push(format!(
                "<< /Title (Level {i}) /Parent {parent_id} 0 R /First {child_id} 0 R /Last {child_id} 0 R /Count 1 /Dest [3 0 R /Fit] >>"
            ));
        } else {
            deep_objects.push(format!(
                "<< /Title (Level {i}) /Parent {parent_id} 0 R /Dest [3 0 R /Fit] >>"
            ));
        }
    }
    let deep_refs: Vec<&str> = deep_objects.iter().map(String::as_str).collect();
    let deep_pdf = build_pdf(&deep_refs, 1);

    let deep_doc = engine
        .open_bytes(deep_pdf, None)
        .expect("deep outline PDF opens");
    let deep_outline = deep_doc.get_outline();
    assert!(
        matches!(deep_outline, Err(PdfError::InvalidPdfReason(ref msg)) if msg.contains("outline")),
        "outline deeper than MAX_OUTLINE_DEPTH must be rejected safely: {deep_outline:?}"
    );

    let parent_child_cycle_pdf = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] >>",
            "<< /Type /Outlines /First 5 0 R /Last 5 0 R /Count 1 >>",
            "<< /Title (Node A) /Parent 4 0 R /First 6 0 R /Last 6 0 R /Count 1 /Dest [3 0 R /Fit] >>",
            "<< /Title (Node B) /Parent 5 0 R /First 5 0 R /Last 5 0 R /Count 1 /Dest [3 0 R /Fit] >>",
        ],
        1,
    );
    if let Ok(doc) = engine.open_bytes(parent_child_cycle_pdf, None) {
        let res = doc.get_outline();
        assert!(
            matches!(res, Ok(_) | Err(PdfError::InvalidPdfReason(_))),
            "parent-child cyclic outline must terminate safely: {res:?}"
        );
    }

    let sibling_cycle_pdf = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] >>",
            "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
            "<< /Title (Sib A) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>",
            "<< /Title (Sib B) /Parent 4 0 R /Prev 5 0 R /Next 5 0 R /Dest [3 0 R /Fit] >>",
        ],
        1,
    );
    if let Ok(doc) = engine.open_bytes(sibling_cycle_pdf, None) {
        let res = doc.get_outline();
        assert!(
            matches!(res, Ok(_) | Err(PdfError::InvalidPdfReason(_))),
            "sibling cyclic outline must terminate safely: {res:?}"
        );
    }
}

#[test]
fn pdfium_document_reports_expected_capabilities() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdf = minimal_single_page_pdf("[0 0 200 200]");
    let doc = engine.open_bytes(pdf, None).expect("opens minimal PDF");
    let caps = doc.capabilities();

    assert!(caps.supports(DocumentCapabilities::READ_ANNOTATIONS));
    assert!(caps.supports(DocumentCapabilities::READ_FORM_FIELDS));
    assert!(caps.supports(DocumentCapabilities::WRITE_FORM_FIELDS));
    assert!(caps.supports(DocumentCapabilities::SAVE_BYTES));
    assert!(!caps.supports(DocumentCapabilities::WRITE_ANNOTATIONS));
}

#[test]
fn annotations_exceeding_page_limit_are_safely_bounded() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let total = MAX_ANNOTATIONS_PER_PAGE + 50;

    let mut annot_refs = String::new();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        String::new(),
    ];

    for i in 0..total {
        let obj_id = 4 + i;
        if !annot_refs.is_empty() {
            annot_refs.push(' ');
        }
        annot_refs.push_str(&format!("{obj_id} 0 R"));
        objects.push("<< /Type /Annot /Subtype /Stamp /Rect [10 10 20 20] >>".to_string());
    }
    objects[2] = format!(
        "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [{annot_refs}] >>"
    );

    let obj_refs: Vec<&str> = objects.iter().map(String::as_str).collect();
    let pdf = build_pdf(&obj_refs, 1);
    let doc = engine.open_bytes(pdf, None).expect("opens annotated PDF");

    let annots = doc
        .annotations(PageIndex::zero())
        .expect("reads bounded page annotations");
    assert_eq!(annots.len(), MAX_ANNOTATIONS_PER_PAGE);
}

#[test]
fn annotation_with_excessive_quads_is_bounded_to_max_annotation_quads() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let total_quads = MAX_ANNOTATION_QUADS + 64;
    let mut quad_points = String::with_capacity(total_quads * 24);
    for i in 0..total_quads {
        if i > 0 {
            quad_points.push(' ');
        }
        quad_points.push_str("10 10 50 10 50 20 10 20");
    }

    let annot = format!(
        "<< /Type /Annot /Subtype /Highlight /Rect [10 10 50 20] /QuadPoints [{quad_points}] >>"
    );
    let pdf = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [4 0 R] >>",
            &annot,
        ],
        1,
    );

    let doc = engine.open_bytes(pdf, None).expect("opens highlight PDF");
    let annots = doc
        .annotations(PageIndex::zero())
        .expect("reads bounded highlight quads");
    assert_eq!(annots.len(), 1);
    match &annots[0] {
        AnnotationRecord::Highlight { quads, .. } => {
            assert_eq!(quads.len(), MAX_ANNOTATION_QUADS);
        }
        other => panic!("expected highlight annotation, got {other:?}"),
    }
}

#[test]
fn form_fields_exceeding_document_limit_are_safely_bounded() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let p1_count = 2_100;
    let p2_count = (MAX_FORM_FIELDS_PER_DOCUMENT + 50) - p1_count;

    let mut all_field_refs = String::new();
    let mut p1_annots = String::new();
    let mut p2_annots = String::new();
    let mut objects = vec![
        String::new(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        String::new(),
        String::new(),
    ];

    for i in 0..p1_count {
        let obj_id = 5 + i;
        if !all_field_refs.is_empty() {
            all_field_refs.push(' ');
        }
        all_field_refs.push_str(&format!("{obj_id} 0 R"));
        if !p1_annots.is_empty() {
            p1_annots.push(' ');
        }
        p1_annots.push_str(&format!("{obj_id} 0 R"));
        objects.push(format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (f_{i}) /V (v) /Rect [10 10 50 30] /P 3 0 R /F 4 >>"
        ));
    }

    for j in 0..p2_count {
        let idx = p1_count + j;
        let obj_id = 5 + idx;
        all_field_refs.push_str(&format!(" {obj_id} 0 R"));
        if !p2_annots.is_empty() {
            p2_annots.push(' ');
        }
        p2_annots.push_str(&format!("{obj_id} 0 R"));
        objects.push(format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (f_{idx}) /V (v) /Rect [10 10 50 30] /P 4 0 R /F 4 >>"
        ));
    }

    objects[0] =
        format!("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [{all_field_refs}] >> >>");
    objects[2] = format!(
        "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [{p1_annots}] >>"
    );
    objects[3] = format!(
        "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [{p2_annots}] >>"
    );

    let obj_refs: Vec<&str> = objects.iter().map(String::as_str).collect();
    let pdf = build_pdf(&obj_refs, 1);
    let doc = engine
        .open_bytes(pdf, None)
        .expect("opens multi-page form PDF");

    let fields = doc.form_fields().expect("reads bounded form fields");
    assert_eq!(fields.len(), MAX_FORM_FIELDS_PER_DOCUMENT);
}

#[test]
fn oversized_form_field_value_is_rejected_on_read_and_write() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let oversized = "a".repeat(MAX_FORM_FIELD_VALUE_BYTES + 1);
    let oversized_widget = format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (big) /V ({oversized}) /Rect [10 10 100 30] /P 3 0 R /F 4 >>"
    );
    let oversized_pdf = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [4 0 R] >>",
            &oversized_widget,
        ],
        1,
    );
    let oversized_doc = engine
        .open_bytes(oversized_pdf, None)
        .expect("opens oversized form value PDF");
    assert!(
        matches!(
            oversized_doc.form_fields(),
            Err(PdfError::InvalidPdfReason(_))
        ),
        "form_fields must reject field values exceeding MAX_FORM_FIELD_VALUE_BYTES"
    );

    let small_pdf = build_pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /Resources << >> /MediaBox [0 0 200 200] /Annots [4 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (field) /V (ok) /Rect [10 10 100 30] /P 3 0 R /F 4 >>",
        ],
        1,
    );
    let mut doc = engine
        .open_bytes(small_pdf, None)
        .expect("opens small form PDF");
    assert!(
        matches!(
            doc.set_form_field_value("field", &oversized),
            Err(PdfError::InvalidPdfReason(_))
        ),
        "set_form_field_value must reject values exceeding MAX_FORM_FIELD_VALUE_BYTES"
    );
}

#[test]
fn oversized_password_is_rejected_at_max_password_bytes_boundary() {
    let engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let unencrypted = minimal_single_page_pdf("[0 0 200 200]");
    let encrypted = encrypted_single_page_pdf_bytes();
    let over_limit = "x".repeat(MAX_PASSWORD_BYTES + 1);

    assert!(matches!(
        engine.open_bytes(unencrypted, Some(&over_limit)),
        Err(PdfError::IncorrectPassword)
    ));
    assert!(matches!(
        engine.open_bytes(encrypted, Some(&over_limit)),
        Err(PdfError::IncorrectPassword)
    ));
}
