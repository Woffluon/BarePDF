use std::path::{Path, PathBuf};
use tempfile::tempdir;

use barepdf_core::{PageIndex, PdfError, Rotation};
use barepdf_pdf::{PdfOperations, PdfiumEngine};
use pdfium_render::prelude::*;

/// Helper to create a test PDF with `count` pages, where each page `i` (0-indexed)
/// has width `(i + 1) * 100` and height `(i + 1) * 200` points.
fn create_test_pdf(path: &Path, count: usize) {
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

/// Helper to inspect page count, dimensions, and rotations of a saved PDF.
fn inspect_pdf(path: &Path) -> (usize, Vec<(f32, f32)>, Vec<PdfPageRenderRotation>) {
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
