use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use barepdf_core::{
    pages_to_remove_to_retained_pages, validate_page_selection, PageCount, PageCropRect, PageIndex,
    PdfError, Rotation,
};
use pdfium_render::prelude::*;

use crate::pdfium_lifetime::process_pdfium;

pub struct PdfOperationInput<'a> {
    path: &'a Path,
    password: Option<&'a str>,
}

impl<'a> PdfOperationInput<'a> {
    #[must_use]
    pub const fn new(path: &'a Path, password: Option<&'a str>) -> Self {
        Self { path, password }
    }
}

pub struct PdfOperations;

impl PdfOperations {
    /// Merges multiple PDF files in given order into a new PDF file.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if inputs are empty, any input file cannot be found or loaded,
    /// or the output document cannot be created or saved.
    pub fn merge_files(inputs: &[PathBuf], output: &Path) -> Result<(), PdfError> {
        let inputs = inputs
            .iter()
            .map(|path| PdfOperationInput::new(path, None))
            .collect::<Vec<_>>();
        Self::merge_files_with_passwords(&inputs, output)
    }

    /// Merges PDF inputs using a separate optional borrowed password for each file.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if inputs are empty, any input file cannot be found or loaded with its
    /// password, or the output document cannot be created or saved.
    pub fn merge_files_with_passwords(
        inputs: &[PdfOperationInput<'_>],
        output: &Path,
    ) -> Result<(), PdfError> {
        if inputs.is_empty() {
            return Err(PdfError::InvalidPdfReason(
                "No input files provided for merge".into(),
            ));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
        let mut total_merged_pages: u32 = 0;

        for input in inputs {
            if !input.path.is_file() {
                return Err(PdfError::FileNotFound(input.path.display().to_string()));
            }
            let src_doc = pdfium
                .load_pdf_from_file(input.path, input.password)
                .map_err(|error| map_pdfium_load_error(error, input.password.is_some()))?;
            let page_count = src_doc.pages().len();
            let src_pages = validate_operation_page_count(
                page_count,
                &format!("Input file '{}' contains no pages", input.path.display()),
            )?;
            total_merged_pages =
                total_merged_pages
                    .checked_add(src_pages.get())
                    .ok_or_else(|| {
                        PdfError::InvalidPdfReason(
                            "Merged PDF page count exceeds supported range".into(),
                        )
                    })?;
            let merged_count = PageCount::new(total_merged_pages)
                .ok_or_else(|| PdfError::InvalidPdfReason("Merged PDF contains no pages".into()))?;
            barepdf_core::validate_document_page_count(merged_count)
                .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))?;

            for src_idx in 0..page_count {
                let dest_idx = new_doc.pages().len();
                new_doc
                    .pages_mut()
                    .copy_page_from_document(&src_doc, src_idx, dest_idx)
                    .map_err(map_pdfium_error)?;
            }
        }

        new_doc.save_to_file(output).map_err(map_pdfium_error)?;
        Ok(())
    }

    /// Extracts specific pages from source PDF into a new PDF file.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if pages list is empty, source file is missing or invalid,
    /// any page index is out of range, or the output file cannot be saved.
    pub fn extract_pages(
        source: &Path,
        pages: &[PageIndex],
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::extract_pages_with_password(source, pages, output, None)
    }

    /// Extracts specific pages using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if pages are empty, the source cannot be loaded with the password,
    /// a page index is out of range, or the output cannot be saved.
    pub fn extract_pages_with_password(
        source: &Path,
        pages: &[PageIndex],
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if pages.is_empty() {
            return Err(PdfError::InvalidPdfReason(
                "No pages specified for extraction".into(),
            ));
        }
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages =
            validate_operation_page_count(src_doc.pages().len(), "Source PDF contains no pages")?;

        validate_page_selection(pages, total_pages)
            .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;

        let extracted_raw = u32::try_from(pages.len()).map_err(|_| {
            PdfError::InvalidPdfReason("Extracted page count exceeds supported range".into())
        })?;
        let extracted_count = PageCount::new(extracted_raw).ok_or_else(|| {
            PdfError::InvalidPdfReason("No pages specified for extraction".into())
        })?;
        barepdf_core::validate_document_page_count(extracted_count)
            .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))?;

        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
        for page_idx in pages {
            let p_idx = to_pdfium_page_index(*page_idx)?;
            let dest_idx = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, p_idx, dest_idx)
                .map_err(map_pdfium_error)?;
        }

        new_doc.save_to_file(output).map_err(map_pdfium_error)?;
        Ok(())
    }

    /// Splits a PDF into single-page PDF files saved in output_dir.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file or output directory is missing,
    /// base name is empty, source contains no pages, or any single-page file cannot be saved.
    pub fn split_into_single_pages(
        source: &Path,
        output_dir: &Path,
        base_name: &str,
    ) -> Result<Vec<PathBuf>, PdfError> {
        Self::split_into_single_pages_with_password(source, output_dir, base_name, None)
    }

    /// Splits a PDF using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if the source cannot be loaded with the password, the output directory
    /// is missing, the base name is empty, or a page output cannot be saved.
    pub fn split_into_single_pages_with_password(
        source: &Path,
        output_dir: &Path,
        base_name: &str,
        password: Option<&str>,
    ) -> Result<Vec<PathBuf>, PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }
        if !output_dir.is_dir() {
            return Err(PdfError::FileNotFound(format!(
                "Output directory '{}' not found or is not a directory",
                output_dir.display()
            )));
        }
        let trimmed_base = base_name.trim();
        if trimmed_base.is_empty() {
            return Err(PdfError::InvalidPdfReason(
                "Base name cannot be empty".into(),
            ));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        let validated_total =
            validate_operation_page_count(total_pages, "Source PDF contains no pages")?;

        let mut output_paths = Vec::with_capacity(validated_total.get() as usize);
        let split_result = (|| -> Result<(), PdfError> {
            for i in 0..total_pages {
                let file_name = format!("{trimmed_base}_page_{}.pdf", i + 1);
                let out_path = output_dir.join(file_name);
                let mut page_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
                page_doc
                    .pages_mut()
                    .copy_page_from_document(&src_doc, i, 0)
                    .map_err(map_pdfium_error)?;
                let bytes = page_doc.save_to_bytes().map_err(map_pdfium_error)?;
                atomic_write_file(&out_path, &bytes)?;
                output_paths.push(out_path);
            }
            Ok(())
        })();

        if let Err(error) = split_result {
            for created_path in &output_paths {
                let _ = std::fs::remove_file(created_path);
            }
            return Err(error);
        }

        Ok(output_paths)
    }

    /// Deletes specified pages from source PDF and writes the result to output.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, source contains no pages,
    /// any removal page index is out of bounds, all pages would be deleted,
    /// or output file cannot be saved.
    pub fn delete_pages(
        source: &Path,
        pages_to_remove: &[PageIndex],
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::delete_pages_with_password(source, pages_to_remove, output, None)
    }

    /// Deletes specified pages using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if the source cannot be loaded with the password, a removal index is
    /// invalid, all pages would be removed, or the output cannot be saved.
    pub fn delete_pages_with_password(
        source: &Path,
        pages_to_remove: &[PageIndex],
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages =
            validate_operation_page_count(src_doc.pages().len(), "Source PDF contains no pages")?;

        let retained_pages = pages_to_remove_to_retained_pages(total_pages, pages_to_remove)
            .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;

        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
        for page_idx in &retained_pages {
            let p_idx = to_pdfium_page_index(*page_idx)?;
            let dest_idx = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, p_idx, dest_idx)
                .map_err(map_pdfium_error)?;
        }

        new_doc.save_to_file(output).map_err(map_pdfium_error)?;
        Ok(())
    }

    /// Rotates specified pages in source PDF by given rotation and writes to output.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, source contains no pages,
    /// any rotation page index is out of bounds, or output file cannot be saved.
    pub fn rotate_pages(
        source: &Path,
        rotations: &[(PageIndex, Rotation)],
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::rotate_pages_with_password(source, rotations, output, None)
    }

    /// Rotates specified pages using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if the source cannot be loaded with the password, a rotation page index
    /// is invalid, or the output cannot be saved.
    pub fn rotate_pages_with_password(
        source: &Path,
        rotations: &[(PageIndex, Rotation)],
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        let total_pages_u32 =
            validate_operation_page_count(total_pages, "Source PDF contains no pages")?.get();

        for (page_idx, _) in rotations {
            if page_idx.get() >= total_pages_u32 {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Page index {} is out of bounds (document has {} pages)",
                    page_idx.get() + 1,
                    total_pages
                )));
            }
        }

        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
        for src_idx in 0..total_pages {
            new_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, src_idx, src_idx)
                .map_err(map_pdfium_error)?;
        }

        for (page_idx, rotation) in rotations {
            let p_idx = to_pdfium_page_index(*page_idx)?;
            let mut page = new_doc.pages_mut().get(p_idx).map_err(map_pdfium_error)?;
            page.set_rotation(to_pdfium_rotation(*rotation));
        }

        new_doc.save_to_file(output).map_err(map_pdfium_error)?;
        Ok(())
    }

    /// Reorders pages in source PDF according to new_order and writes to output.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, new_order is empty,
    /// new_order length does not match page count, duplicate or out-of-bounds indices are present,
    /// or output file cannot be saved.
    pub fn reorder_pages(
        source: &Path,
        new_order: &[PageIndex],
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::reorder_pages_with_password(source, new_order, output, None)
    }

    /// Reorders pages using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if the source cannot be loaded with the password, the order is invalid,
    /// or the output cannot be saved.
    pub fn reorder_pages_with_password(
        source: &Path,
        new_order: &[PageIndex],
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }
        if new_order.is_empty() {
            return Err(PdfError::InvalidPdfReason(
                "New page order cannot be empty".into(),
            ));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        let total_pages_u32 =
            validate_operation_page_count(total_pages, "Source PDF contains no pages")?.get();

        if new_order.len() != total_pages as usize {
            return Err(PdfError::InvalidPdfReason(format!(
                "Reorder list length ({}) does not match document page count ({})",
                new_order.len(),
                total_pages
            )));
        }

        let mut seen = HashSet::with_capacity(new_order.len());
        for page_idx in new_order {
            if page_idx.get() >= total_pages_u32 {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Page index {} is out of bounds (document has {} pages)",
                    page_idx.get() + 1,
                    total_pages
                )));
            }
            if !seen.insert(page_idx.get()) {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Duplicate page index {} in reorder list",
                    page_idx.get() + 1
                )));
            }
        }

        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
        for page_idx in new_order {
            let p_idx = to_pdfium_page_index(*page_idx)?;
            let dest_idx = new_doc.pages().len();
            new_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, p_idx, dest_idx)
                .map_err(map_pdfium_error)?;
        }

        new_doc.save_to_file(output).map_err(map_pdfium_error)?;
        Ok(())
    }

    /// Crops specified pages in source PDF according to crops and writes to output.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded,
    /// any crop page index is out of bounds, crop dimensions are invalid, or output cannot be saved.
    pub fn crop_pages(
        source: &Path,
        crops: &[PageCropRect],
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::crop_pages_with_password(source, crops, output, None)
    }

    /// Crops specified pages in source PDF using an optional borrowed source password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded with the password,
    /// any crop page index is out of bounds, crop dimensions are invalid, or output cannot be saved.
    pub fn crop_pages_with_password(
        source: &Path,
        crops: &[PageCropRect],
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = doc.pages().len();
        let _ = validate_operation_page_count(total_pages, "Source PDF contains no pages")?;

        for crop in crops {
            if crop.page_index >= total_pages as usize {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Page index {} is out of bounds (document has {} pages)",
                    crop.page_index + 1,
                    total_pages
                )));
            }
            if crop.left >= crop.right || crop.bottom >= crop.top {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Invalid crop box dimensions for page {}: left={}, bottom={}, right={}, top={}",
                    crop.page_index + 1,
                    crop.left,
                    crop.bottom,
                    crop.right,
                    crop.top
                )));
            }
        }

        for crop in crops {
            let p_u32 = u32::try_from(crop.page_index).map_err(|_| {
                PdfError::InvalidPdfReason("Crop page index exceeds supported range".into())
            })?;
            let p_idx = to_pdfium_raw_page_index(p_u32)?;
            let mut page = doc.pages_mut().get(p_idx).map_err(map_pdfium_error)?;
            let rect = PdfRect::new_from_values(crop.bottom, crop.left, crop.top, crop.right);
            page.boundaries_mut()
                .set_crop(rect)
                .map_err(map_pdfium_error)?;
            page.boundaries_mut()
                .set_media(rect)
                .map_err(map_pdfium_error)?;
        }

        let bytes = doc.save_to_bytes().map_err(map_pdfium_error)?;
        drop(doc);
        drop(_ffi_guard);

        atomic_write_file(output, &bytes)?;
        Ok(())
    }

    /// Flattens annotations into the PDF and writes to output. Alias for `save_with_annotations`.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded, or output file cannot be saved.
    pub fn save_annotations(
        source: &Path,
        annotations: &barepdf_core::DocumentAnnotations,
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::save_with_annotations(source, annotations, output)
    }

    /// Flattens annotations into the PDF using an optional password. Alias for `save_with_annotations_with_password`.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded with the password, or output file cannot be saved.
    pub fn save_annotations_with_password(
        source: &Path,
        annotations: &barepdf_core::DocumentAnnotations,
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        Self::save_with_annotations_with_password(source, annotations, output, password)
    }

    /// Flattens highlights, ink strokes, signature stamps, and free text annotations into the PDF and writes to output.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded, or output file cannot be saved.
    pub fn save_with_annotations(
        source: &Path,
        annotations: &barepdf_core::DocumentAnnotations,
        output: &Path,
    ) -> Result<(), PdfError> {
        Self::save_with_annotations_with_password(source, annotations, output, None)
    }

    /// Flattens highlights, ink strokes, and signature stamps into the PDF using an optional borrowed password.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if source file is missing, cannot be loaded with the password, or output file cannot be saved.
    #[allow(clippy::too_many_lines)]
    pub fn save_with_annotations_with_password(
        source: &Path,
        annotations: &barepdf_core::DocumentAnnotations,
        output: &Path,
        password: Option<&str>,
    ) -> Result<(), PdfError> {
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let _ffi_guard = crate::pdfium_lifetime::pdfium_ffi_lock();
        let pdfium = process_pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = doc.pages().len();
        let _ = validate_operation_page_count(total_pages, "Source PDF contains no pages")?;

        let font_token = if annotations.free_texts.is_empty() {
            None
        } else {
            Some(doc.fonts_mut().helvetica())
        };

        for p_idx in 0..total_pages {
            let Ok(p_u32) = u32::try_from(p_idx) else {
                continue;
            };
            let page_idx = PageIndex::from_raw(p_u32);
            let has_highlights = annotations.highlights.iter().any(|h| h.page == page_idx);
            let has_strokes = annotations.strokes.iter().any(|s| s.page == page_idx);
            let has_signatures = annotations.signatures.iter().any(|s| s.page == page_idx);
            let has_free_texts = annotations
                .free_texts
                .iter()
                .any(|ft| ft.page_index == p_idx as usize);
            if !has_highlights && !has_strokes && !has_signatures && !has_free_texts {
                continue;
            }

            let mut page = doc.pages_mut().get(p_idx).map_err(map_pdfium_error)?;
            let page_w = page.width().value;
            let page_h = page.height().value;

            for quad in annotations.highlights.iter().filter(|h| h.page == page_idx) {
                let left = quad.x_norm * page_w;
                let right = (quad.x_norm + quad.w_norm) * page_w;
                let top = (1.0 - quad.y_norm) * page_h;
                let bottom = (1.0 - (quad.y_norm + quad.h_norm)) * page_h;
                let rect_obj = PdfPagePathObject::new_rect(
                    &doc,
                    PdfRect::new_from_values(bottom, left, top, right),
                    None,
                    None,
                    Some(PdfColor::new(250, 204, 21, 95)),
                )
                .map_err(map_pdfium_error)?;
                page.objects_mut()
                    .add_path_object(rect_obj)
                    .map_err(map_pdfium_error)?;
            }

            for stroke in annotations.strokes.iter().filter(|s| s.page == page_idx) {
                let Some(&(nx0, ny0)) = stroke.points.first() else {
                    continue;
                };
                let x0 = nx0 * page_w;
                let y0 = (1.0 - ny0) * page_h;
                let (r, g, b, a) = stroke.color.rgba();
                let mut path = PdfPagePathObject::new(
                    &doc,
                    PdfPoints::new(x0),
                    PdfPoints::new(y0),
                    Some(PdfColor::new(r, g, b, a)),
                    Some(PdfPoints::new(stroke.width_pts.max(0.5))),
                    None,
                )
                .map_err(map_pdfium_error)?;
                if stroke.points.len() == 1 {
                    path.line_to(PdfPoints::new(x0 + 0.5), PdfPoints::new(y0 + 0.5))
                        .map_err(map_pdfium_error)?;
                } else {
                    for &(nx, ny) in stroke.points.iter().skip(1) {
                        path.line_to(
                            PdfPoints::new(nx * page_w),
                            PdfPoints::new((1.0 - ny) * page_h),
                        )
                        .map_err(map_pdfium_error)?;
                    }
                }
                page.objects_mut()
                    .add_path_object(path)
                    .map_err(map_pdfium_error)?;
            }

            for sig in annotations.signatures.iter().filter(|s| s.page == page_idx) {
                let box_x = sig.x_norm * page_w;
                let box_y_bottom = (1.0 - (sig.y_norm + sig.h_norm)) * page_h;
                let box_w = (sig.w_norm * page_w).max(1.0);
                let box_h = (sig.h_norm * page_h).max(1.0);

                match &sig.payload {
                    barepdf_core::SignaturePayload::Drawn(polylines) => {
                        for polyline in polylines {
                            let Some(&(px0, py0)) = polyline.first() else {
                                continue;
                            };
                            let x0 = box_x + px0 * box_w;
                            let y0 = box_y_bottom + (1.0 - py0) * box_h;
                            let mut path = PdfPagePathObject::new(
                                &doc,
                                PdfPoints::new(x0),
                                PdfPoints::new(y0),
                                Some(PdfColor::new(20, 20, 40, 255)),
                                Some(PdfPoints::new(sig.stroke_width.max(0.5))),
                                None,
                            )
                            .map_err(map_pdfium_error)?;
                            if polyline.len() == 1 {
                                path.line_to(PdfPoints::new(x0 + 0.5), PdfPoints::new(y0 + 0.5))
                                    .map_err(map_pdfium_error)?;
                            } else {
                                for &(px, py) in polyline.iter().skip(1) {
                                    path.line_to(
                                        PdfPoints::new(box_x + px * box_w),
                                        PdfPoints::new(box_y_bottom + (1.0 - py) * box_h),
                                    )
                                    .map_err(map_pdfium_error)?;
                                }
                            }
                            page.objects_mut()
                                .add_path_object(path)
                                .map_err(map_pdfium_error)?;
                        }
                    }
                    barepdf_core::SignaturePayload::Image {
                        width,
                        height,
                        rgba,
                    } => {
                        let (w_i32, h_i32) = validate_signature_image(*width, *height, rgba)?;
                        let mut bgra = Vec::with_capacity(rgba.len());
                        for px in rgba.chunks_exact(4) {
                            if let &[r, g, b, a] = px {
                                bgra.extend_from_slice(&[b, g, r, a]);
                            }
                        }
                        let bitmap =
                            PdfBitmap::from_bytes(w_i32, h_i32, PdfBitmapFormat::BGRA, &mut bgra)
                                .map_err(map_pdfium_error)?;
                        let mut img_obj =
                            PdfPageImageObject::new(&doc).map_err(map_pdfium_error)?;
                        img_obj.set_bitmap(&bitmap).map_err(map_pdfium_error)?;
                        img_obj.scale(box_w, box_h).map_err(map_pdfium_error)?;
                        img_obj
                            .translate(PdfPoints::new(box_x), PdfPoints::new(box_y_bottom))
                            .map_err(map_pdfium_error)?;
                        page.objects_mut()
                            .add_image_object(img_obj)
                            .map_err(map_pdfium_error)?;
                    }
                }
            }

            if let Some(font) = font_token {
                for ft in annotations
                    .free_texts
                    .iter()
                    .filter(|ft| ft.page_index == p_idx as usize)
                {
                    let lines: Vec<&str> = if ft.text.is_empty() {
                        vec![""]
                    } else {
                        ft.text.lines().collect()
                    };
                    let [r, g, b, a] = ft.color_rgba;
                    let color = PdfColor::new(r, g, b, a);
                    let font_size_pts = PdfPoints::new(ft.font_size.max(1.0));
                    let line_height = ft.font_size.max(1.0) * 1.2;

                    for (line_idx, line) in lines.iter().enumerate() {
                        let line_y = ft.y - (line_idx as f32 * line_height);
                        let mut text_obj = PdfPageTextObject::new(&doc, *line, font, font_size_pts)
                            .map_err(map_pdfium_error)?;
                        text_obj.set_fill_color(color).map_err(map_pdfium_error)?;
                        text_obj
                            .translate(PdfPoints::new(ft.x), PdfPoints::new(line_y))
                            .map_err(map_pdfium_error)?;
                        page.objects_mut()
                            .add_text_object(text_obj)
                            .map_err(map_pdfium_error)?;
                    }
                }
            }

            page.regenerate_content().map_err(map_pdfium_error)?;
        }

        let bytes = doc.save_to_bytes().map_err(map_pdfium_error)?;
        drop(doc);
        drop(_ffi_guard);

        atomic_write_file(output, &bytes)?;
        Ok(())
    }
}

fn validate_operation_page_count(
    raw_len: PdfPageIndex,
    empty_reason: &str,
) -> Result<PageCount, PdfError> {
    let count = u32::try_from(raw_len)
        .map_err(|_| PdfError::InvalidPdfReason("PDF page count exceeds supported range".into()))?;
    let page_count =
        PageCount::new(count).ok_or_else(|| PdfError::InvalidPdfReason(empty_reason.into()))?;
    barepdf_core::validate_document_page_count(page_count)
        .map_err(|error| PdfError::InvalidPdfReason(error.to_string()))?;
    Ok(page_count)
}

fn validate_signature_image(width: u32, height: u32, rgba: &[u8]) -> Result<(i32, i32), PdfError> {
    let max_dim = barepdf_core::limits::MAX_SAFE_RENDER_DIMENSION;
    if width == 0 || height == 0 || width > max_dim || height > max_dim {
        return Err(PdfError::InvalidPdfReason(format!(
            "signature image dimensions {width}x{height} are outside allowed bounds 1..={max_dim}"
        )));
    }
    let expected_bytes = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| PdfError::InvalidPdfReason("signature image buffer size overflow".into()))?;
    if rgba.len() != expected_bytes {
        return Err(PdfError::InvalidPdfReason(format!(
            "signature image RGBA buffer length ({}) does not match {width}x{height}x4 ({expected_bytes})",
            rgba.len()
        )));
    }
    let w_i32 = i32::try_from(width)
        .map_err(|_| PdfError::InvalidPdfReason("signature image width out of bounds".into()))?;
    let h_i32 = i32::try_from(height)
        .map_err(|_| PdfError::InvalidPdfReason("signature image height out of bounds".into()))?;
    Ok((w_i32, h_i32))
}

static ATOMIC_WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

fn atomic_write_file(output: &Path, bytes: &[u8]) -> Result<(), PdfError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let file_name = output
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("barepdf_save.pdf");

    let mut last_error = None;
    for _ in 0..100 {
        let seq = ATOMIC_WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let temp_name = format!(".{file_name}.tmp-{pid}-{seq}");
        let temp_path = parent.join(temp_name);

        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(err);
                continue;
            }
            Err(err) => {
                return Err(PdfError::FileAccess {
                    source: Arc::new(err),
                })
            }
        };

        let result = (|| -> Result<(), std::io::Error> {
            use std::io::Write;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp_path, output)?;
            Ok(())
        })();

        if let Err(err) = result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(PdfError::FileAccess {
                source: Arc::new(err),
            });
        }

        return Ok(());
    }

    Err(PdfError::FileAccess {
        source: Arc::new(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "unable to allocate unique temporary file for atomic write",
            )
        })),
    })
}

fn to_pdfium_raw_page_index(raw: u32) -> Result<PdfPageIndex, PdfError> {
    if raw >= barepdf_core::limits::MAX_DOCUMENT_PAGES {
        return Err(PdfError::InvalidPdfReason(
            "Page index exceeds supported document page limit".into(),
        ));
    }
    PdfPageIndex::try_from(raw).map_err(|_| {
        PdfError::InvalidPdfReason("Page index exceeds PDFium's supported range".into())
    })
}

fn to_pdfium_page_index(index: PageIndex) -> Result<PdfPageIndex, PdfError> {
    if !index.is_within_limit() {
        return Err(PdfError::InvalidPdfReason(
            "Page index exceeds supported document page limit".into(),
        ));
    }
    to_pdfium_raw_page_index(index.get())
}

fn to_pdfium_rotation(rotation: Rotation) -> PdfPageRenderRotation {
    match rotation {
        Rotation::Degrees0 => PdfPageRenderRotation::None,
        Rotation::Degrees90 => PdfPageRenderRotation::Degrees90,
        Rotation::Degrees180 => PdfPageRenderRotation::Degrees180,
        Rotation::Degrees270 => PdfPageRenderRotation::Degrees270,
    }
}

fn map_pdfium_error(error: PdfiumError) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            PdfError::PasswordRequired
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            PdfError::FileAccess {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            PdfError::InvalidPdf {
                source: Arc::new(error),
            }
        }
        error @ PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            PdfError::UnsupportedEncryption {
                source: Arc::new(error),
            }
        }
        error => PdfError::Backend {
            source: Arc::new(error),
        },
    }
}

fn map_pdfium_load_error(error: PdfiumError, password_supplied: bool) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if password_supplied {
                PdfError::IncorrectPassword
            } else {
                PdfError::PasswordRequired
            }
        }
        error => map_pdfium_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::limits::{MAX_DOCUMENT_PAGES, MAX_SAFE_RENDER_DIMENSION};
    use barepdf_core::{PageIndex, Rotation};

    #[test]
    fn test_to_pdfium_rotation_mapping() {
        assert_eq!(
            to_pdfium_rotation(Rotation::Degrees0),
            PdfPageRenderRotation::None
        );
        assert_eq!(
            to_pdfium_rotation(Rotation::Degrees90),
            PdfPageRenderRotation::Degrees90
        );
        assert_eq!(
            to_pdfium_rotation(Rotation::Degrees180),
            PdfPageRenderRotation::Degrees180
        );
        assert_eq!(
            to_pdfium_rotation(Rotation::Degrees270),
            PdfPageRenderRotation::Degrees270
        );
    }

    #[test]
    fn test_to_pdfium_page_index_bounds() {
        assert_eq!(to_pdfium_page_index(PageIndex::zero()).unwrap(), 0);
        assert_eq!(to_pdfium_page_index(PageIndex::from_raw(100)).unwrap(), 100);
        // PageIndex::from_raw clamps to MAX_DOCUMENT_PAGES - 1, while raw conversion rejects >= MAX_DOCUMENT_PAGES
        let clamped_index = PageIndex::from_raw(u32::MAX);
        assert_eq!(clamped_index.get(), MAX_DOCUMENT_PAGES - 1);
        assert!(to_pdfium_page_index(clamped_index).is_ok());
        assert!(to_pdfium_raw_page_index(MAX_DOCUMENT_PAGES).is_err());
        assert!(to_pdfium_raw_page_index(u32::MAX).is_err());
    }

    #[test]
    fn validate_operation_page_count_enforces_max_document_pages() {
        assert!(validate_operation_page_count(0, "empty").is_err());
        let max_valid = PdfPageIndex::try_from(MAX_DOCUMENT_PAGES).expect("fits in PdfPageIndex");
        assert_eq!(
            validate_operation_page_count(max_valid, "empty")
                .expect("at limit")
                .get(),
            MAX_DOCUMENT_PAGES
        );
        let over_limit =
            PdfPageIndex::try_from(MAX_DOCUMENT_PAGES + 1).expect("fits in PdfPageIndex");
        assert!(validate_operation_page_count(over_limit, "empty").is_err());
    }

    #[test]
    fn validate_signature_image_rejects_zero_oversized_or_mismatched_buffers() {
        assert!(validate_signature_image(0, 2, &[0; 8]).is_err());
        assert!(validate_signature_image(2, 0, &[0; 8]).is_err());
        assert!(validate_signature_image(MAX_SAFE_RENDER_DIMENSION + 1, 1, &[0; 4]).is_err());
        assert!(validate_signature_image(1, MAX_SAFE_RENDER_DIMENSION + 1, &[0; 4]).is_err());
        assert!(validate_signature_image(2, 2, &[0; 12]).is_err());
        assert!(validate_signature_image(2, 2, &[0; 20]).is_err());
        assert_eq!(
            validate_signature_image(2, 3, &[0; 24]).expect("valid 2x3 RGBA"),
            (2, 3)
        );
    }

    #[test]
    fn test_map_pdfium_error_password() {
        let err = PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError);
        assert!(matches!(map_pdfium_error(err), PdfError::PasswordRequired));
    }

    #[test]
    fn supplied_password_maps_pdfium_password_error_to_incorrect_password() {
        let err = PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError);
        assert!(matches!(
            map_pdfium_load_error(err, true),
            PdfError::IncorrectPassword
        ));
    }
}
