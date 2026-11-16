use std::path::{Path, PathBuf};
use tempfile::tempdir;

use barepdf_core::{PageIndex, PdfError, Rotation};
use barepdf_pdf::{PdfOperationInput, PdfOperations, PdfiumEngine};
use pdfium_render::prelude::*;

/// Helper to create a test PDF with `count` pages, where each page `i` (0-indexed)
/// has width `(i + 1) * 100` and height `(i + 1) * 200` points.
fn create_test_pdf(path: &Path, count: usize) {
    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let mut doc = pdfium.create_new_pdf().expect("create new pdf");
    for i in 0..count {
        let width = PdfPoints::new(100.0 * (i as f32 + 1.0));
        let height = PdfPoints::new(200.0 * (i as f32 + 1.0));
        doc.pages_mut()
            .create_page_at_end(PdfPagePaperSize::Custom(width, height))
            .expect("create page");
    }
    doc.save_to_file(path).expect("save pdf");
}

fn create_encrypted_test_pdf(path: &Path) {
    const PDF: &str = "JVBERi0xLjMKJeLjz9MKMSAwIG9iago8PAovUHJvZHVjZXIgPDhjOGU0MjU5ZDc+Cj4+CmVuZG9iagoyIDAgb2JqCjw8Ci9UeXBlIC9QYWdlcwovQ291bnQgMQovS2lkcyBbIDQgMCBSIF0KPj4KZW5kb2JqCjMgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDIgMCBSCj4+CmVuZG9iago0IDAgb2JqCjw8Ci9UeXBlIC9QYWdlCi9SZXNvdXJjZXMgPDwKPj4KL01lZGlhQm94IFsgMC4wIDAuMCA3MiA3MiBdCi9QYXJlbnQgMiAwIFIKPj4KZW5kb2JqCjUgMCBvYmoKPDwKL1YgMgovUiAzCi9MZW5ndGggMTI4Ci9QIDQyOTQ5NjcyOTIKL0ZpbHRlciAvU3RhbmRhcmQKL08gPDBjYzhjMzkyODQ4YzY0NTA5YTc1Zjk2ZjkwOTQ0MDk4NTZiNWJmYTRlMjA2ZDM5ZjNkYTQ3NjZkMzVhNDQzZTA+Ci9VIDwwMWQ5M2FhOGRhODk2ZDdmNmFkYjJlNmVhYTNlZjlmMDI4YmY0ZTVlNGU3NThhNDE2NDAwNGU1NmZmZmEwMTA4Pgo+PgplbmRvYmoKeHJlZgowIDYKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDE1IDAwMDAwIG4gCjAwMDAwMDAwNTkgMDAwMDAgbiAKMDAwMDAwMDExOCAwMDAwMCBuIAowMDAwMDAwMTY3IDAwMDAwIG4gCjAwMDAwMDAyNTkgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA2Ci9Sb290IDMgMCBSCi9JbmZvIDEgMCBSCi9JRCBbIDw2NDY2NjM2MTY2MzUzNDMyMzczOTMwMzMzMjMwMzY2NDY0MzEzMTM0MzQzMTY0MzA2MjM1NjI2MTM5MzYzMTYyPiA8NjQ2NjM2MTY2MzUzNDMyMzczOTMwMzMzMjMwMzY2NDY0MzEzMTM0MzQzMTY0MzA2MjM1NjI2MTM5MzYzMTYyPiBdCi9FbmNyeXB0IDUgMCBSCj4+CnN0YXJ0eHJlZgo0NzQKJSVFT0YK";
    std::fs::write(path, decode_base64(PDF)).expect("write encrypted PDF fixture");
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

/// Helper to inspect page count, dimensions, and rotations of a saved PDF.
fn inspect_pdf(path: &Path) -> (usize, Vec<(f32, f32)>, Vec<PdfPageRenderRotation>) {
    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let doc = pdfium.load_pdf_from_file(path, None).expect("load pdf");
    let count = doc.pages().len() as usize;
    let mut dimensions = Vec::with_capacity(count);
    let mut rotations = Vec::with_capacity(count);
    for i in 0..doc.pages().len() {
        let page = doc.pages().get(i).expect("get page");
        dimensions.push((page.width().value, page.height().value));
        rotations.push(page.rotation().expect("page rotation"));
    }
    (count, dimensions, rotations)
}

fn idx(i: u32) -> PageIndex {
    PageIndex::from_raw(i)
}

// ---------------------------------------------------------------------------
// merge_files tests
// ---------------------------------------------------------------------------

#[test]
fn test_merge_files_success() {
    let dir = tempdir().expect("tempdir");
    let pdf1 = dir.path().join("doc1.pdf");
    let pdf2 = dir.path().join("doc2.pdf");
    let output = dir.path().join("merged.pdf");

    create_test_pdf(&pdf1, 2); // pages with w=100, 200
    create_test_pdf(&pdf2, 3); // pages with w=100, 200, 300

    let inputs = vec![pdf1, pdf2];
    PdfOperations::merge_files(&inputs, &output).expect("merge succeeds");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 5);
    assert_eq!(dims[0].0, 100.0);
    assert_eq!(dims[1].0, 200.0);
    assert_eq!(dims[2].0, 100.0);
    assert_eq!(dims[3].0, 200.0);
    assert_eq!(dims[4].0, 300.0);
}

#[test]
fn test_merge_files_three_documents() {
    let dir = tempdir().expect("tempdir");
    let p1 = dir.path().join("a.pdf");
    let p2 = dir.path().join("b.pdf");
    let p3 = dir.path().join("c.pdf");
    let output = dir.path().join("merged_3.pdf");

    create_test_pdf(&p1, 1);
    create_test_pdf(&p2, 2);
    create_test_pdf(&p3, 1);

    let inputs = vec![p1, p2, p3];
    PdfOperations::merge_files(&inputs, &output).expect("merge succeeds");

    let (count, _, _) = inspect_pdf(&output);
    assert_eq!(count, 4);
}

#[test]
fn test_merge_files_single_input() {
    let dir = tempdir().expect("tempdir");
    let p1 = dir.path().join("single.pdf");
    let output = dir.path().join("merged_single.pdf");

    create_test_pdf(&p1, 3);
    PdfOperations::merge_files(&[p1], &output).expect("merge single file");

    let (count, _, _) = inspect_pdf(&output);
    assert_eq!(count, 3);
}

#[test]
fn test_merge_files_empty_inputs_fails() {
    let dir = tempdir().expect("tempdir");
    let output = dir.path().join("empty_merged.pdf");

    let inputs: Vec<PathBuf> = vec![];
    let result = PdfOperations::merge_files(&inputs, &output);
    assert!(result.is_err());
    match result {
        Err(PdfError::InvalidPdfReason(msg)) => {
            assert!(
                msg.to_lowercase().contains("no input") || msg.to_lowercase().contains("empty")
            );
        }
        other => panic!("expected InvalidPdfReason, got: {other:?}"),
    }
}

#[test]
fn test_merge_files_non_existent_input_fails() {
    let dir = tempdir().expect("tempdir");
    let p1 = dir.path().join("non_existent.pdf");
    let output = dir.path().join("out.pdf");

    let result = PdfOperations::merge_files(&[p1], &output);
    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(PdfError::FileNotFound(_)) | Err(PdfError::FileAccess { .. })
    ));
}

#[test]
fn password_aware_merge_accepts_borrowed_per_input_credentials() {
    let dir = tempdir().expect("tempdir");
    let encrypted = dir.path().join("encrypted.pdf");
    let plain = dir.path().join("plain.pdf");
    let output = dir.path().join("merged-password-aware.pdf");
    create_encrypted_test_pdf(&encrypted);
    create_test_pdf(&plain, 2);
    let inputs = [
        PdfOperationInput::new(&encrypted, Some("right-password")),
        PdfOperationInput::new(&plain, None),
    ];

    PdfOperations::merge_files_with_passwords(&inputs, &output)
        .expect("password-aware merge unlocks only the encrypted input");

    assert_eq!(inspect_pdf(&output).0, 3);
}

// ---------------------------------------------------------------------------
// extract_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_extract_pages_subset() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("extracted.pdf");

    create_test_pdf(&src, 4); // pages 0(100), 1(200), 2(300), 3(400)

    PdfOperations::extract_pages(&src, &[idx(0), idx(2)], &output).expect("extract pages");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0].0, 100.0);
    assert_eq!(dims[1].0, 300.0);
}

#[test]
fn test_extract_pages_single_page() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("extracted_one.pdf");

    create_test_pdf(&src, 4);

    PdfOperations::extract_pages(&src, &[idx(1)], &output).expect("extract page");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 1);
    assert_eq!(dims[0].0, 200.0);
}

#[test]
fn test_extract_pages_custom_order() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("extracted_order.pdf");

    create_test_pdf(&src, 4);

    PdfOperations::extract_pages(&src, &[idx(3), idx(1)], &output).expect("extract custom order");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0].0, 400.0);
    assert_eq!(dims[1].0, 200.0);
}

#[test]
fn test_extract_pages_empty_list_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("extracted.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::extract_pages(&src, &[], &output);
    assert!(result.is_err());
}

#[test]
fn test_extract_pages_out_of_bounds_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("extracted.pdf");

    create_test_pdf(&src, 3); // pages 0, 1, 2

    let result = PdfOperations::extract_pages(&src, &[idx(0), idx(5)], &output);
    assert!(result.is_err());
}

#[test]
fn test_extract_pages_non_existent_source_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("missing.pdf");
    let output = dir.path().join("extracted.pdf");

    let result = PdfOperations::extract_pages(&src, &[idx(0)], &output);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// split_into_single_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_split_into_single_pages_success() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let out_dir = dir.path().join("split_out");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    create_test_pdf(&src, 3); // 3 pages: 100, 200, 300

    let files = PdfOperations::split_into_single_pages(&src, &out_dir, "doc")
        .expect("split into single pages");

    assert_eq!(files.len(), 3);
    for (i, file_path) in files.iter().enumerate() {
        assert!(file_path.is_file());
        let (count, dims, _) = inspect_pdf(file_path);
        assert_eq!(count, 1);
        assert_eq!(dims[0].0, 100.0 * (i as f32 + 1.0));
    }
}

#[test]
fn test_split_single_page_doc() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("single.pdf");
    let out_dir = dir.path().join("split_single");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    create_test_pdf(&src, 1);

    let files =
        PdfOperations::split_into_single_pages(&src, &out_dir, "page").expect("split single page");

    assert_eq!(files.len(), 1);
    assert!(files[0].is_file());
    let (count, _, _) = inspect_pdf(&files[0]);
    assert_eq!(count, 1);
}

#[test]
fn test_split_empty_base_name_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    create_test_pdf(&src, 2);

    let result = PdfOperations::split_into_single_pages(&src, &out_dir, "   ");
    assert!(result.is_err());
}

#[test]
fn test_split_non_existent_output_dir_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let out_dir = dir.path().join("does_not_exist");

    create_test_pdf(&src, 2);

    let result = PdfOperations::split_into_single_pages(&src, &out_dir, "split");
    assert!(result.is_err());
}

#[test]
fn test_split_non_existent_source_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("does_not_exist.pdf");
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    let result = PdfOperations::split_into_single_pages(&src, &out_dir, "split");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// delete_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_delete_pages_middle() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("deleted.pdf");

    create_test_pdf(&src, 4); // pages 0(100), 1(200), 2(300), 3(400)

    PdfOperations::delete_pages(&src, &[idx(1), idx(2)], &output).expect("delete middle pages");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0].0, 100.0);
    assert_eq!(dims[1].0, 400.0);
}

#[test]
fn test_delete_pages_first_and_last() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("deleted.pdf");

    create_test_pdf(&src, 4);

    PdfOperations::delete_pages(&src, &[idx(0), idx(3)], &output).expect("delete ends");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0].0, 200.0);
    assert_eq!(dims[1].0, 300.0);
}

#[test]
fn test_delete_pages_empty_list_retains_all() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("deleted_none.pdf");

    create_test_pdf(&src, 3);

    PdfOperations::delete_pages(&src, &[], &output).expect("delete no pages");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 3);
    assert_eq!(dims[0].0, 100.0);
    assert_eq!(dims[1].0, 200.0);
    assert_eq!(dims[2].0, 300.0);
}

#[test]
fn test_delete_pages_all_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("deleted_all.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::delete_pages(&src, &[idx(0), idx(1), idx(2)], &output);
    assert!(result.is_err());
}

#[test]
fn test_delete_pages_out_of_bounds_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("deleted.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::delete_pages(&src, &[idx(5)], &output);
    assert!(result.is_err());
}

#[test]
fn test_delete_pages_non_existent_source_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("missing.pdf");
    let output = dir.path().join("deleted.pdf");

    let result = PdfOperations::delete_pages(&src, &[idx(0)], &output);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// rotate_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_rotate_pages_success() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("rotated.pdf");

    create_test_pdf(&src, 3);

    let rotations = vec![
        (idx(0), Rotation::Degrees90),
        (idx(2), Rotation::Degrees270),
    ];
    PdfOperations::rotate_pages(&src, &rotations, &output).expect("rotate pages");

    let (count, _, rots) = inspect_pdf(&output);
    assert_eq!(count, 3);
    assert_eq!(rots[0], PdfPageRenderRotation::Degrees90);
    assert_eq!(rots[1], PdfPageRenderRotation::None);
    assert_eq!(rots[2], PdfPageRenderRotation::Degrees270);
}

#[test]
fn test_rotate_pages_all_orientations() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("rotated_all.pdf");

    create_test_pdf(&src, 4);

    let rotations = vec![
        (idx(0), Rotation::Degrees0),
        (idx(1), Rotation::Degrees90),
        (idx(2), Rotation::Degrees180),
        (idx(3), Rotation::Degrees270),
    ];
    PdfOperations::rotate_pages(&src, &rotations, &output).expect("rotate all pages");

    let (count, _, rots) = inspect_pdf(&output);
    assert_eq!(count, 4);
    assert_eq!(rots[0], PdfPageRenderRotation::None);
    assert_eq!(rots[1], PdfPageRenderRotation::Degrees90);
    assert_eq!(rots[2], PdfPageRenderRotation::Degrees180);
    assert_eq!(rots[3], PdfPageRenderRotation::Degrees270);
}

#[test]
fn test_rotate_pages_empty_rotations() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("rotated_empty.pdf");

    create_test_pdf(&src, 2);

    PdfOperations::rotate_pages(&src, &[], &output).expect("rotate empty");

    let (count, _, rots) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(rots[0], PdfPageRenderRotation::None);
    assert_eq!(rots[1], PdfPageRenderRotation::None);
}

#[test]
fn test_rotate_pages_out_of_bounds_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("rotated.pdf");

    create_test_pdf(&src, 2);

    let result = PdfOperations::rotate_pages(&src, &[(idx(5), Rotation::Degrees90)], &output);
    assert!(result.is_err());
}

#[test]
fn test_rotate_pages_non_existent_source_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("missing.pdf");
    let output = dir.path().join("rotated.pdf");

    let result = PdfOperations::rotate_pages(&src, &[(idx(0), Rotation::Degrees90)], &output);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// reorder_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_reorder_pages_success() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered.pdf");

    create_test_pdf(&src, 3); // pages 0(100), 1(200), 2(300)

    let new_order = vec![idx(2), idx(0), idx(1)];
    PdfOperations::reorder_pages(&src, &new_order, &output).expect("reorder pages");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 3);
    assert_eq!(dims[0].0, 300.0);
    assert_eq!(dims[1].0, 100.0);
    assert_eq!(dims[2].0, 200.0);
}

#[test]
fn test_reorder_pages_identity() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered_id.pdf");

    create_test_pdf(&src, 3);

    let new_order = vec![idx(0), idx(1), idx(2)];
    PdfOperations::reorder_pages(&src, &new_order, &output).expect("reorder identity");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 3);
    assert_eq!(dims[0].0, 100.0);
    assert_eq!(dims[1].0, 200.0);
    assert_eq!(dims[2].0, 300.0);
}

#[test]
fn test_reorder_pages_empty_order_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::reorder_pages(&src, &[], &output);
    assert!(result.is_err());
}

#[test]
fn test_reorder_pages_mismatched_length_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::reorder_pages(&src, &[idx(0), idx(1)], &output);
    assert!(result.is_err());
}

#[test]
fn test_reorder_pages_duplicate_indices_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::reorder_pages(&src, &[idx(0), idx(0), idx(1)], &output);
    assert!(result.is_err());
}

#[test]
fn test_reorder_pages_out_of_bounds_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("reordered.pdf");

    create_test_pdf(&src, 3);

    let result = PdfOperations::reorder_pages(&src, &[idx(0), idx(1), idx(5)], &output);
    assert!(result.is_err());
}

#[test]
fn test_reorder_pages_non_existent_source_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("missing.pdf");
    let output = dir.path().join("reordered.pdf");

    let result = PdfOperations::reorder_pages(&src, &[idx(0)], &output);
    assert!(result.is_err());
}

#[test]
fn password_aware_single_input_operations_accept_borrowed_job_credentials() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("source.pdf");
    create_test_pdf(&source, 2);
    let password = Some("ephemeral job password");

    let extracted = dir.path().join("extracted-password-aware.pdf");
    PdfOperations::extract_pages_with_password(&source, &[idx(0)], &extracted, password)
        .expect("password-aware extraction succeeds");

    let split_dir = dir.path().join("split-password-aware");
    std::fs::create_dir(&split_dir).expect("create split directory");
    let split =
        PdfOperations::split_into_single_pages_with_password(&source, &split_dir, "page", password)
            .expect("password-aware split succeeds");

    let deleted = dir.path().join("deleted-password-aware.pdf");
    PdfOperations::delete_pages_with_password(&source, &[idx(1)], &deleted, password)
        .expect("password-aware deletion succeeds");

    let rotated = dir.path().join("rotated-password-aware.pdf");
    PdfOperations::rotate_pages_with_password(
        &source,
        &[(idx(0), Rotation::Degrees90)],
        &rotated,
        password,
    )
    .expect("password-aware rotation succeeds");

    let reordered = dir.path().join("reordered-password-aware.pdf");
    PdfOperations::reorder_pages_with_password(&source, &[idx(1), idx(0)], &reordered, password)
        .expect("password-aware reorder succeeds");

    assert_eq!(inspect_pdf(&extracted).0, 1);
    assert_eq!(split.len(), 2);
    assert_eq!(inspect_pdf(&deleted).0, 1);
    assert_eq!(inspect_pdf(&rotated).2[0], PdfPageRenderRotation::Degrees90);
    assert_eq!(inspect_pdf(&reordered).1[0].0, 200.0);
}

#[test]
fn wrong_job_password_is_distinct_and_leaves_no_partial_tool_outputs() {
    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("encrypted.pdf");
    create_encrypted_test_pdf(&source);
    let wrong_password = Some("wrong-password");

    let extracted = dir.path().join("wrong-extracted.pdf");
    let error =
        PdfOperations::extract_pages_with_password(&source, &[idx(0)], &extracted, wrong_password)
            .expect_err("wrong extraction password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert!(!extracted.exists());

    let split_dir = dir.path().join("wrong-split");
    std::fs::create_dir(&split_dir).expect("create split directory");
    let error = PdfOperations::split_into_single_pages_with_password(
        &source,
        &split_dir,
        "page",
        wrong_password,
    )
    .expect_err("wrong split password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert_eq!(
        std::fs::read_dir(&split_dir)
            .expect("read split dir")
            .count(),
        0
    );

    let deleted = dir.path().join("wrong-deleted.pdf");
    let error = PdfOperations::delete_pages_with_password(&source, &[], &deleted, wrong_password)
        .expect_err("wrong delete password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert!(!deleted.exists());

    let rotated = dir.path().join("wrong-rotated.pdf");
    let error = PdfOperations::rotate_pages_with_password(
        &source,
        &[(idx(0), Rotation::Degrees90)],
        &rotated,
        wrong_password,
    )
    .expect_err("wrong rotate password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert!(!rotated.exists());

    let reordered = dir.path().join("wrong-reordered.pdf");
    let error =
        PdfOperations::reorder_pages_with_password(&source, &[idx(0)], &reordered, wrong_password)
            .expect_err("wrong reorder password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert!(!reordered.exists());

    let merged = dir.path().join("wrong-merged.pdf");
    let inputs = [PdfOperationInput::new(&source, wrong_password)];
    let error = PdfOperations::merge_files_with_passwords(&inputs, &merged)
        .expect_err("wrong merge password fails");
    assert!(matches!(error, PdfError::IncorrectPassword));
    assert!(!merged.exists());

    let unlocked = dir.path().join("unlocked.pdf");
    PdfOperations::extract_pages_with_password(
        &source,
        &[idx(0)],
        &unlocked,
        Some("right-password"),
    )
    .expect("fixture unlocks with the correct password");
    assert_eq!(inspect_pdf(&unlocked).0, 1);
}

#[test]
fn test_save_with_annotations_embeds_highlights_strokes_and_signatures() {
    use barepdf_core::{
        DocumentAnnotations, HighlightQuad, InkColor, InkStroke, SignaturePayload, SignatureStamp,
    };

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("annotated_src.pdf");
    let output = dir.path().join("annotated_out.pdf");
    create_test_pdf(&source, 2);

    let annotations = DocumentAnnotations {
        highlights: vec![HighlightQuad {
            page: idx(0),
            x_norm: 0.1,
            y_norm: 0.2,
            w_norm: 0.5,
            h_norm: 0.05,
        }],
        strokes: vec![
            InkStroke {
                page: idx(0),
                points: vec![(0.1, 0.1), (0.4, 0.4), (0.6, 0.3)],
                color: InkColor::Red,
                width_pts: 4.0,
            },
            InkStroke {
                page: idx(1),
                points: vec![(0.5, 0.5)],
                color: InkColor::Blue,
                width_pts: 2.0,
            },
        ],
        signatures: vec![
            SignatureStamp {
                page: idx(0),
                x_norm: 0.5,
                y_norm: 0.7,
                w_norm: 0.3,
                h_norm: 0.12,
                payload: SignaturePayload::Drawn(vec![vec![(0.0, 0.5), (0.5, 0.2), (1.0, 0.8)]]),
                stroke_width: 2.0,
            },
            SignatureStamp {
                page: idx(1),
                x_norm: 0.2,
                y_norm: 0.6,
                w_norm: 0.25,
                h_norm: 0.1,
                payload: SignaturePayload::Image {
                    width: 2,
                    height: 2,
                    rgba: vec![
                        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                    ],
                },
                stroke_width: 2.0,
            },
        ],
        free_texts: Vec::new(),
    };

    PdfOperations::save_with_annotations(&source, &annotations, &output)
        .expect("save_with_annotations to new file succeeds");
    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0], (100.0, 200.0));

    // Also verify in-place saving (source == output)
    PdfOperations::save_with_annotations(&output, &annotations, &output)
        .expect("in-place save_with_annotations succeeds");
    assert_eq!(inspect_pdf(&output).0, 2);
}

#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn test_save_with_annotations_atomic_write_preserves_original_on_failure() {
    use barepdf_core::DocumentAnnotations;

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("source.pdf");
    create_test_pdf(&source, 1);

    let output_dir = if cfg!(windows) {
        dir.path().to_path_buf()
    } else {
        let protected = dir.path().join("protected");
        std::fs::create_dir_all(&protected).expect("create protected dir");
        protected
    };
    let output = output_dir.join("destination.pdf");
    let original_content = b"ORIGINAL_IMPORTANT_DOCUMENT_BYTES_NOT_ZERO";
    std::fs::write(&output, original_content).expect("write original destination");

    // Make destination read-only on Windows, or its parent directory read-only on POSIX,
    // so writing/replacing fails cleanly without touching the original file.
    let protected_path = if cfg!(windows) { &output } else { &output_dir };
    let mut perms = std::fs::metadata(protected_path)
        .expect("metadata")
        .permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(protected_path, perms).expect("set readonly");

    let annotations = DocumentAnnotations {
        highlights: Vec::new(),
        strokes: Vec::new(),
        signatures: Vec::new(),
        free_texts: Vec::new(),
    };

    let result = PdfOperations::save_with_annotations(&source, &annotations, &output);
    assert!(
        result.is_err(),
        "saving over read-only destination must fail"
    );

    // Restore permissions so file inspection and tempdir cleanup succeed
    let mut restore_perms = std::fs::metadata(protected_path)
        .expect("metadata")
        .permissions();
    restore_perms.set_readonly(false);
    let _ = std::fs::set_permissions(protected_path, restore_perms);

    // Verify original content is completely intact and never truncated
    let current_content = std::fs::read(&output).expect("read output after failed save");
    assert_eq!(
        current_content, original_content,
        "Original file must be preserved and not zero-byte corrupted"
    );

    // Verify no temporary files remain in output directory
    let temp_files: Vec<_> = std::fs::read_dir(&output_dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".destination.pdf.tmp-")
        })
        .collect();
    assert!(
        temp_files.is_empty(),
        "No temporary staging files should remain after failure"
    );
}

#[test]
fn test_split_rolls_back_created_files_when_later_page_fails() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("three_pages.pdf");
    let out_dir = dir.path().join("split_rollback");
    std::fs::create_dir_all(&out_dir).expect("create out_dir");

    create_test_pdf(&src, 3);

    // Pre-create a directory at the path where page 2's file would be renamed,
    // causing page 1 to succeed and page 2's atomic rename to fail.
    let blocking_dir = out_dir.join("doc_page_2.pdf");
    std::fs::create_dir_all(&blocking_dir).expect("create blocking directory for page 2");

    let result = PdfOperations::split_into_single_pages(&src, &out_dir, "doc");
    assert!(
        result.is_err(),
        "split must fail when page 2 cannot be written"
    );

    let page_1_path = out_dir.join("doc_page_1.pdf");
    let page_3_path = out_dir.join("doc_page_3.pdf");
    assert!(
        !page_1_path.exists(),
        "page 1 partial output must be rolled back on mid-split failure"
    );
    assert!(
        !page_3_path.exists(),
        "page 3 output must not be created after page 2 failure"
    );
}

#[test]
fn test_save_with_annotations_rejects_malformed_signature_images() {
    use barepdf_core::limits::MAX_SAFE_RENDER_DIMENSION;
    use barepdf_core::{DocumentAnnotations, SignaturePayload, SignatureStamp};

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("source.pdf");
    let output = dir.path().join("rejected_out.pdf");
    create_test_pdf(&source, 1);

    let invalid_payloads = [
        SignaturePayload::Image {
            width: 0,
            height: 2,
            rgba: Vec::new(),
        },
        SignaturePayload::Image {
            width: 2,
            height: 0,
            rgba: Vec::new(),
        },
        SignaturePayload::Image {
            width: MAX_SAFE_RENDER_DIMENSION + 1,
            height: 1,
            rgba: vec![0; 4],
        },
        SignaturePayload::Image {
            width: 2,
            height: 2,
            rgba: vec![255; 15], // expected 16 bytes
        },
    ];

    for payload in invalid_payloads {
        let annotations = DocumentAnnotations {
            highlights: Vec::new(),
            strokes: Vec::new(),
            signatures: vec![SignatureStamp {
                page: idx(0),
                x_norm: 0.1,
                y_norm: 0.1,
                w_norm: 0.2,
                h_norm: 0.1,
                payload,
                stroke_width: 2.0,
            }],
            free_texts: Vec::new(),
        };

        let result = PdfOperations::save_with_annotations(&source, &annotations, &output);
        assert!(
            matches!(result, Err(PdfError::InvalidPdfReason(_))),
            "expected InvalidPdfReason for malformed signature image, got: {result:?}"
        );
        assert!(!output.exists());
    }
}

// ---------------------------------------------------------------------------
// crop_pages tests
// ---------------------------------------------------------------------------

#[test]
fn test_crop_pages_modifies_page_dimensions() {
    use barepdf_core::PageCropRect;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("cropped.pdf");

    create_test_pdf(&src, 2); // page 0: 100x200, page 1: 200x400

    let crops = vec![
        PageCropRect {
            page_index: 0,
            left: 10.0,
            bottom: 20.0,
            right: 80.0,
            top: 120.0,
        },
        PageCropRect {
            page_index: 1,
            left: 20.0,
            bottom: 40.0,
            right: 150.0,
            top: 300.0,
        },
    ];

    PdfOperations::crop_pages(&src, &crops, &output).expect("crop succeeds");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0], (70.0, 100.0));
    assert_eq!(dims[1], (130.0, 260.0));
}

#[test]
fn test_crop_pages_empty_crops_preserves_document() {
    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("crop_empty.pdf");

    create_test_pdf(&src, 2);

    PdfOperations::crop_pages(&src, &[], &output).expect("empty crop succeeds");

    let (count, dims, _) = inspect_pdf(&output);
    assert_eq!(count, 2);
    assert_eq!(dims[0], (100.0, 200.0));
    assert_eq!(dims[1], (200.0, 400.0));
}

#[test]
fn test_crop_pages_out_of_bounds_fails() {
    use barepdf_core::PageCropRect;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("crop_oob.pdf");

    create_test_pdf(&src, 2);

    let crops = vec![PageCropRect {
        page_index: 5,
        left: 10.0,
        bottom: 20.0,
        right: 80.0,
        top: 120.0,
    }];

    let result = PdfOperations::crop_pages(&src, &crops, &output);
    assert!(result.is_err());
    assert!(!output.exists());
}

#[test]
fn test_crop_pages_invalid_box_fails() {
    use barepdf_core::PageCropRect;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.pdf");
    let output = dir.path().join("crop_inv.pdf");

    create_test_pdf(&src, 2);

    let invalid_horizontal = vec![PageCropRect {
        page_index: 0,
        left: 80.0,
        bottom: 20.0,
        right: 10.0,
        top: 120.0,
    }];
    assert!(PdfOperations::crop_pages(&src, &invalid_horizontal, &output).is_err());
    assert!(!output.exists());

    let invalid_vertical = vec![PageCropRect {
        page_index: 0,
        left: 10.0,
        bottom: 120.0,
        right: 80.0,
        top: 20.0,
    }];
    assert!(PdfOperations::crop_pages(&src, &invalid_vertical, &output).is_err());
    assert!(!output.exists());
}

#[test]
fn test_crop_pages_non_existent_source_fails() {
    use barepdf_core::PageCropRect;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("missing.pdf");
    let output = dir.path().join("cropped.pdf");

    let crops = vec![PageCropRect {
        page_index: 0,
        left: 10.0,
        bottom: 20.0,
        right: 80.0,
        top: 120.0,
    }];

    let result = PdfOperations::crop_pages(&src, &crops, &output);
    assert!(matches!(result, Err(PdfError::FileNotFound(_))));
}

#[test]
fn test_crop_pages_in_place_succeeds() {
    use barepdf_core::PageCropRect;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("in_place.pdf");
    create_test_pdf(&src, 1);

    let crops = vec![PageCropRect {
        page_index: 0,
        left: 10.0,
        bottom: 10.0,
        right: 60.0,
        top: 80.0,
    }];

    PdfOperations::crop_pages(&src, &crops, &src).expect("in-place crop succeeds");
    let (count, dims, _) = inspect_pdf(&src);
    assert_eq!(count, 1);
    assert_eq!(dims[0], (50.0, 70.0));
}

// ---------------------------------------------------------------------------
// free_text annotation tests
// ---------------------------------------------------------------------------

#[test]
fn test_save_with_annotations_flattens_free_text() {
    use barepdf_core::{DocumentAnnotations, FreeTextAnnotation};

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("free_text_src.pdf");
    let output = dir.path().join("free_text_out.pdf");
    create_test_pdf(&source, 1);

    let mut annotations = DocumentAnnotations::default();
    annotations.free_texts.push(FreeTextAnnotation {
        id: uuid::Uuid::new_v4(),
        page_index: 0,
        x: 5.0,
        y: 50.0,
        text: "Hello BarePDF FreeText".to_string(),
        font_size: 6.0,
        color_rgba: [0, 0, 0, 255],
    });

    PdfOperations::save_with_annotations(&source, &annotations, &output)
        .expect("save free text annotation succeeds");

    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let doc = pdfium
        .load_pdf_from_file(&output, None)
        .expect("load output");
    let page = doc.pages().get(0).expect("get page 0");
    let text = page.text().expect("page text").all();
    assert!(
        text.contains("Hello BarePDF FreeText"),
        "expected text on page to contain 'Hello BarePDF FreeText', got: {text}"
    );
}

#[test]
fn test_save_with_annotations_flattens_multiline_free_text() {
    use barepdf_core::{DocumentAnnotations, FreeTextAnnotation};

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("multiline_src.pdf");
    let output = dir.path().join("multiline_out.pdf");
    create_test_pdf(&source, 1);

    let mut annotations = DocumentAnnotations::default();
    annotations.free_texts.push(FreeTextAnnotation {
        id: uuid::Uuid::new_v4(),
        page_index: 0,
        x: 20.0,
        y: 100.0,
        text: "Line One\nLine Two".to_string(),
        font_size: 12.0,
        color_rgba: [255, 0, 0, 255],
    });

    PdfOperations::save_with_annotations(&source, &annotations, &output)
        .expect("save multiline free text annotation succeeds");

    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let doc = pdfium
        .load_pdf_from_file(&output, None)
        .expect("load output");
    let page = doc.pages().get(0).expect("get page 0");
    let text = page.text().expect("page text").all();
    assert!(
        text.contains("Line One") && text.contains("Line Two"),
        "expected text on page to contain 'Line One' and 'Line Two', got: {text}"
    );
}

#[test]
fn signature_custom_stroke_width_is_preserved() {
    use barepdf_core::{DocumentAnnotations, SignaturePayload, SignatureStamp};

    let dir = tempdir().expect("tempdir");
    let source = dir.path().join("sig_stroke.pdf");
    let output = dir.path().join("sig_stroke_out.pdf");
    create_test_pdf(&source, 1);

    let custom_stroke = 4.5;
    let annotations = DocumentAnnotations {
        highlights: Vec::new(),
        strokes: Vec::new(),
        signatures: vec![SignatureStamp {
            page: idx(0),
            x_norm: 0.2,
            y_norm: 0.3,
            w_norm: 0.4,
            h_norm: 0.15,
            payload: SignaturePayload::Drawn(vec![vec![(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)]]),
            stroke_width: custom_stroke,
        }],
        free_texts: Vec::new(),
    };

    assert_eq!(annotations.signatures[0].stroke_width, 4.5);

    // Saving document with custom signature stroke succeeds
    PdfOperations::save_with_annotations(&source, &annotations, &output)
        .expect("save with custom signature stroke succeeds");
    assert!(output.exists());
}

// ---------------------------------------------------------------------------
// compress_pdf tests
// ---------------------------------------------------------------------------

fn make_gradient_photo_bgra(width: i32, height: i32, alpha: u8) -> Vec<u8> {
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            // Smooth multi-frequency photographic gradient (realistic natural tones)
            let fx = x as f32 / width as f32;
            let fy = y as f32 / height as f32;
            let r = ((128.0 + 90.0 * (fx * 6.0).sin() + 30.0 * (fy * 11.0).cos()).clamp(0.0, 255.0))
                as u8;
            let g = ((120.0 + 70.0 * (fy * 5.0).sin() + 40.0 * ((fx + fy) * 8.0).cos())
                .clamp(0.0, 255.0)) as u8;
            let b = ((140.0 + 80.0 * ((fx - fy) * 7.0).cos()).clamp(0.0, 255.0)) as u8;
            bgra.extend_from_slice(&[b, g, r, alpha]);
        }
    }
    bgra
}

fn create_hybrid_vector_and_image_pdf(path: &Path, img_w: i32, img_h: i32) {
    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let mut doc = pdfium.create_new_pdf().expect("create new pdf");
    let font = doc.fonts_mut().helvetica();
    let mut page = doc
        .pages_mut()
        .create_page_at_end(PdfPagePaperSize::a4())
        .expect("create A4 page");

    // Z=0: background vector rectangle
    let bg_rect = PdfPagePathObject::new_rect(
        &doc,
        PdfRect::new_from_values(100.0, 50.0, 500.0, 540.0),
        Some(PdfColor::BLUE),
        Some(PdfPoints::new(2.0)),
        Some(PdfColor::new(240, 244, 248, 255)),
    )
    .expect("create background rect");
    page.objects_mut()
        .add_path_object(bg_rect)
        .expect("add bg rect");

    // Z=1: high-DPI opaque raster image (placed in 240x180 pt box => ~360 DPI at 1200x900)
    let mut bgr = Vec::with_capacity((img_w * img_h * 3) as usize);
    for px in make_gradient_photo_bgra(img_w, img_h, 255).chunks_exact(4) {
        bgr.extend_from_slice(&px[..3]);
    }
    let bitmap =
        PdfBitmap::from_bytes(img_w, img_h, PdfBitmapFormat::BGR, &mut bgr).expect("create bitmap");
    let mut img = PdfPageImageObject::new(&doc).expect("create image obj");
    img.set_bitmap(&bitmap).expect("set bitmap");
    img.scale(240.0, 180.0).expect("scale image");
    img.translate(PdfPoints::new(80.0), PdfPoints::new(200.0))
        .expect("translate image");
    page.objects_mut().add_image_object(img).expect("add image");

    // Z=2: foreground vector stroke over the image
    let fg_line = PdfPagePathObject::new_line(
        &doc,
        PdfPoints::new(80.0),
        PdfPoints::new(200.0),
        PdfPoints::new(320.0),
        PdfPoints::new(380.0),
        PdfColor::RED,
        PdfPoints::new(1.5),
    )
    .expect("create foreground line");
    page.objects_mut()
        .add_path_object(fg_line)
        .expect("add fg line");

    // Z=3: foreground lossless vector text
    let mut text_obj = PdfPageTextObject::new(
        &doc,
        "BarePDF Lossless Vector Text",
        font,
        PdfPoints::new(16.0),
    )
    .expect("create text obj");
    text_obj
        .translate(PdfPoints::new(80.0), PdfPoints::new(560.0))
        .expect("translate text");
    page.objects_mut()
        .add_text_object(text_obj)
        .expect("add text");

    page.regenerate_content().expect("regenerate content");
    drop(page);
    doc.save_to_file(path).expect("save hybrid pdf");
}

#[test]
fn test_compress_pdf_preserves_text_vectors_zorder_and_shrinks_all_modes() {
    use barepdf_core::CompressionMode;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("hybrid_src.pdf");
    create_hybrid_vector_and_image_pdf(&src, 1200, 900);

    let out_light = dir.path().join("compressed_light.pdf");
    let out_balanced = dir.path().join("compressed_balanced.pdf");
    let out_max = dir.path().join("compressed_max.pdf");

    let rep_light = PdfOperations::compress_pdf(&src, CompressionMode::LosslessLight, &out_light)
        .expect("LosslessLight compress succeeds");
    let rep_balanced = PdfOperations::compress_pdf(&src, CompressionMode::Balanced, &out_balanced)
        .expect("Balanced compress succeeds");
    let rep_max = PdfOperations::compress_pdf(&src, CompressionMode::Maximum, &out_max)
        .expect("Maximum compress succeeds");

    println!(
        "COMPRESS_METRICS 1200x900: orig={}B | Light={}B ({:.1}%) | Balanced={}B ({:.1}%) | Max={}B ({:.1}%)",
        rep_light.original_bytes,
        rep_light.compressed_bytes,
        rep_light.saved_percent,
        rep_balanced.compressed_bytes,
        rep_balanced.saved_percent,
        rep_max.compressed_bytes,
        rep_max.saved_percent,
    );

    assert!(rep_light.compressed_bytes < rep_light.original_bytes);
    assert!(rep_balanced.compressed_bytes < rep_balanced.original_bytes);
    assert!(rep_max.compressed_bytes < rep_max.original_bytes);
    assert!(rep_light.saved_percent > 0.0);
    assert!(rep_max.compressed_bytes <= rep_balanced.compressed_bytes);
    assert!(rep_balanced.compressed_bytes <= rep_light.compressed_bytes);

    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    for out_path in [&out_light, &out_balanced, &out_max] {
        let doc = pdfium
            .load_pdf_from_file(out_path, None)
            .expect("compressed PDF loads");
        let page = doc.pages().get(0).expect("page 0");

        let extracted_text = page.text().expect("page text").all();
        assert!(
            extracted_text.contains("BarePDF Lossless Vector Text"),
            "Text must be 100% preserved in {out_path:?}, got: {extracted_text}"
        );

        let obj_types: Vec<PdfPageObjectType> =
            page.objects().iter().map(|obj| obj.object_type()).collect();
        assert_eq!(
            obj_types,
            vec![
                PdfPageObjectType::Path,
                PdfPageObjectType::Image,
                PdfPageObjectType::Path,
                PdfPageObjectType::Text,
            ],
            "Z-order and vector/text objects must remain intact"
        );

        let img_bounds = page
            .objects()
            .get(1)
            .expect("image object")
            .bounds()
            .expect("image bounds")
            .to_rect();
        assert!((img_bounds.left().value - 80.0).abs() < 1.0);
        assert!((img_bounds.bottom().value - 200.0).abs() < 1.0);
        assert!((img_bounds.width().value - 240.0).abs() < 1.0);
        assert!((img_bounds.height().value - 180.0).abs() < 1.0);

        let rendered = page
            .render_with_config(&PdfRenderConfig::new().set_target_width(400))
            .expect("compressed page renders cleanly");
        assert_eq!(rendered.width(), 400);
    }
}

#[test]
fn test_compress_pdf_skips_transparent_alpha_images() {
    use barepdf_core::CompressionMode;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("alpha_src.pdf");
    let out = dir.path().join("alpha_out.pdf");

    {
        let _lock = barepdf_pdf::pdfium_ffi_lock();
        let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
        let pdfium = Pdfium::default();
        let mut doc = pdfium.create_new_pdf().expect("create new pdf");
        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .expect("create page");

        // Transparent image (alpha = 120) at high DPI
        let mut alpha_bgra = make_gradient_photo_bgra(600, 600, 120);
        let alpha_bmp = PdfBitmap::from_bytes(600, 600, PdfBitmapFormat::BGRA, &mut alpha_bgra)
            .expect("alpha bitmap");
        let mut alpha_img = PdfPageImageObject::new(&doc).expect("alpha img obj");
        alpha_img.set_bitmap(&alpha_bmp).expect("set alpha bitmap");
        alpha_img.scale(100.0, 100.0).expect("scale alpha img");
        page.objects_mut()
            .add_image_object(alpha_img)
            .expect("add alpha img");

        page.regenerate_content().expect("regen");
        drop(page);
        doc.save_to_file(&src).expect("save alpha pdf");
    }

    let before_raw_len = {
        let _lock = barepdf_pdf::pdfium_ffi_lock();
        let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
        let pdfium = Pdfium::default();
        let doc = pdfium.load_pdf_from_file(&src, None).expect("load src");
        let page = doc.pages().get(0).expect("page 0");
        let obj = page.objects().get(0).expect("obj 0");
        let img = obj.as_image_object().expect("image obj");
        img.get_raw_image_data().expect("raw data").len()
    };

    PdfOperations::compress_pdf(&src, CompressionMode::Maximum, &out)
        .expect("compress alpha pdf succeeds");

    let _lock = barepdf_pdf::pdfium_ffi_lock();
    let _engine = PdfiumEngine::new().expect("PDFium engine initializes");
    let pdfium = Pdfium::default();
    let doc = pdfium.load_pdf_from_file(&out, None).expect("load out");
    let page = doc.pages().get(0).expect("page 0");
    let obj = page.objects().get(0).expect("obj 0");
    let img = obj.as_image_object().expect("image obj");
    let after_raw_len = img.get_raw_image_data().expect("raw data").len();
    let filters: Vec<String> = img.filters().iter().map(|f| f.name().to_owned()).collect();
    assert!(!filters.iter().any(|f| f == "DCTDecode"));
    assert_eq!(after_raw_len, before_raw_len);
    let processed = img.get_processed_bitmap(&doc).expect("processed alpha bmp");
    assert_eq!(processed.format().ok(), Some(PdfBitmapFormat::BGRA));
    assert_eq!(processed.as_raw_bytes().get(3).copied(), Some(120));
}

#[test]
fn test_compress_pdf_preserves_original_bytes_when_no_reduction() {
    use barepdf_core::CompressionMode;

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("minimal.pdf");
    let out = dir.path().join("minimal_out.pdf");
    create_test_pdf(&src, 1);
    let original_bytes = std::fs::read(&src).expect("read original");

    let report = PdfOperations::compress_pdf(&src, CompressionMode::LosslessLight, &out)
        .expect("compress minimal succeeds");
    let output_bytes = std::fs::read(&out).expect("read output");

    if report.compressed_bytes == report.original_bytes {
        assert_eq!(report.saved_percent, 0.0);
        assert_eq!(output_bytes, original_bytes);
    } else {
        assert!(output_bytes.len() < original_bytes.len());
    }
}

#[test]
fn test_compress_pdf_handles_missing_and_encrypted_sources() {
    use barepdf_core::CompressionMode;

    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("missing.pdf");
    let out = dir.path().join("out.pdf");
    assert!(matches!(
        PdfOperations::compress_pdf(&missing, CompressionMode::Balanced, &out),
        Err(PdfError::FileNotFound(_))
    ));

    let encrypted = dir.path().join("encrypted.pdf");
    create_encrypted_test_pdf(&encrypted);
    assert!(matches!(
        PdfOperations::compress_pdf_with_password(
            &encrypted,
            CompressionMode::Balanced,
            &out,
            Some("wrong-password")
        ),
        Err(PdfError::IncorrectPassword)
    ));
    assert!(!out.exists());

    let report = PdfOperations::compress_pdf_with_password(
        &encrypted,
        CompressionMode::Balanced,
        &out,
        Some("right-password"),
    )
    .expect("compress with right password succeeds");
    assert!(out.is_file());
    assert!(report.compressed_bytes <= report.original_bytes);
}
