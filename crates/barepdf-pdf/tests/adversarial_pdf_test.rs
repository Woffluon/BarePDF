#![forbid(unsafe_code)]

use barepdf_core::{
    PageIndex, PdfError, Rotation, MAX_OUTLINE_DEPTH, MAX_PASSWORD_BYTES, MAX_SAFE_RENDER_DIMENSION,
};
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
