use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use barepdf_core::{
    pages_to_remove_to_retained_pages, validate_page_selection, PageCount, PageIndex, PdfError,
    Rotation,
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

        let pdfium = process_pdfium()?;
        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;

        for input in inputs {
            if !input.path.is_file() {
                return Err(PdfError::FileNotFound(input.path.display().to_string()));
            }
            let src_doc = pdfium
                .load_pdf_from_file(input.path, input.password)
                .map_err(|error| map_pdfium_load_error(error, input.password.is_some()))?;
            let page_count = src_doc.pages().len();
            if page_count == 0 {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Input file '{}' contains no pages",
                    input.path.display()
                )));
            }
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

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages_raw = u32::try_from(src_doc.pages().len()).map_err(|_| {
            PdfError::InvalidPdfReason("PDF page count exceeds supported range".into())
        })?;
        let total_pages = PageCount::new(total_pages_raw)
            .ok_or_else(|| PdfError::InvalidPdfReason("Source PDF contains no pages".into()))?;

        validate_page_selection(pages, total_pages)
            .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))?;

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

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        if total_pages == 0 {
            return Err(PdfError::InvalidPdfReason(
                "Source PDF contains no pages".into(),
            ));
        }

        let mut output_paths = Vec::with_capacity(total_pages as usize);
        for i in 0..total_pages {
            let file_name = format!("{trimmed_base}_page_{}.pdf", i + 1);
            let out_path = output_dir.join(file_name);
            let mut page_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;
            page_doc
                .pages_mut()
                .copy_page_from_document(&src_doc, i, 0)
                .map_err(map_pdfium_error)?;
            page_doc.save_to_file(&out_path).map_err(map_pdfium_error)?;
            output_paths.push(out_path);
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

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages_raw = u32::try_from(src_doc.pages().len()).map_err(|_| {
            PdfError::InvalidPdfReason("PDF page count exceeds supported range".into())
        })?;
        let total_pages = PageCount::new(total_pages_raw)
            .ok_or_else(|| PdfError::InvalidPdfReason("Source PDF contains no pages".into()))?;

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

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        if total_pages <= 0 {
            return Err(PdfError::InvalidPdfReason(
                "Source PDF contains no pages".into(),
            ));
        }
        let total_pages_u32 = u32::try_from(total_pages).map_err(|_| {
            PdfError::InvalidPdfReason("PDF page count exceeds supported range".into())
        })?;

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

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = src_doc.pages().len();
        if total_pages <= 0 {
            return Err(PdfError::InvalidPdfReason(
                "Source PDF contains no pages".into(),
            ));
        }
        let total_pages_u32 = u32::try_from(total_pages).map_err(|_| {
            PdfError::InvalidPdfReason("PDF page count exceeds supported range".into())
        })?;

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

    /// Flattens highlights, ink strokes, and signature stamps into the PDF and writes to output.
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

        let pdfium = process_pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(source, password)
            .map_err(|error| map_pdfium_load_error(error, password.is_some()))?;

        let total_pages = doc.pages().len();
        if total_pages <= 0 {
            return Err(PdfError::InvalidPdfReason(
                "Source PDF contains no pages".into(),
            ));
        }

        for p_idx in 0..total_pages {
            let Ok(p_u32) = u32::try_from(p_idx) else {
                continue;
            };
            let page_idx = PageIndex::from_raw(p_u32);
            let has_highlights = annotations.highlights.iter().any(|h| h.page == page_idx);
            let has_strokes = annotations.strokes.iter().any(|s| s.page == page_idx);
            let has_signatures = annotations.signatures.iter().any(|s| s.page == page_idx);
            if !has_highlights && !has_strokes && !has_signatures {
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
                    for &(nx, ny) in &stroke.points[1..] {
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
                                Some(PdfPoints::new(2.0)),
                                None,
                            )
                            .map_err(map_pdfium_error)?;
                            if polyline.len() == 1 {
                                path.line_to(PdfPoints::new(x0 + 0.5), PdfPoints::new(y0 + 0.5))
                                    .map_err(map_pdfium_error)?;
                            } else {
                                for &(px, py) in &polyline[1..] {
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
                        let w_i32 = i32::try_from(*width).map_err(|_| {
                            PdfError::InvalidPdfReason("signature image width out of bounds".into())
                        })?;
                        let h_i32 = i32::try_from(*height).map_err(|_| {
                            PdfError::InvalidPdfReason(
                                "signature image height out of bounds".into(),
                            )
                        })?;
                        let mut bgra = Vec::with_capacity(rgba.len());
                        for px in rgba.chunks_exact(4) {
                            bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
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

            page.regenerate_content().map_err(map_pdfium_error)?;
        }

        let bytes = doc.save_to_bytes().map_err(map_pdfium_error)?;
        std::fs::write(output, bytes).map_err(|error| PdfError::FileAccess {
            source: Arc::new(error),
        })?;
        Ok(())
    }
}

fn to_pdfium_page_index(index: PageIndex) -> Result<PdfPageIndex, PdfError> {
    PdfPageIndex::try_from(index.get()).map_err(|_| {
        PdfError::InvalidPdfReason("Page index exceeds PDFium's supported range".into())
    })
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
        // If index exceeds i32::MAX (or PdfPageIndex range)
        let huge_index = PageIndex::from_raw(u32::MAX);
        assert!(to_pdfium_page_index(huge_index).is_err());
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
