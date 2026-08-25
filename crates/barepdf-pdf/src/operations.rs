use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use barepdf_core::{
    pages_to_remove_to_retained_pages, validate_page_selection, PageCount, PageIndex, PdfError,
    Rotation,
};
use pdfium_render::prelude::*;

use crate::pdfium_lifetime::process_pdfium;

pub struct PdfOperations;

impl PdfOperations {
    /// Merges multiple PDF files in given order into a new PDF file.
    ///
    /// # Errors
    ///
    /// Returns `PdfError` if inputs are empty, any input file cannot be found or loaded,
    /// or the output document cannot be created or saved.
    pub fn merge_files(inputs: &[PathBuf], output: &Path) -> Result<(), PdfError> {
        if inputs.is_empty() {
            return Err(PdfError::InvalidPdfReason(
                "No input files provided for merge".into(),
            ));
        }

        let pdfium = process_pdfium()?;
        let mut new_doc = pdfium.create_new_pdf().map_err(map_pdfium_error)?;

        for input in inputs {
            if !input.is_file() {
                return Err(PdfError::FileNotFound(input.display().to_string()));
            }
            let src_doc = pdfium
                .load_pdf_from_file(input, None)
                .map_err(map_pdfium_error)?;
            let page_count = src_doc.pages().len();
            if page_count == 0 {
                return Err(PdfError::InvalidPdfReason(format!(
                    "Input file '{}' contains no pages",
                    input.display()
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
            .load_pdf_from_file(source, None)
            .map_err(map_pdfium_error)?;

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
            .load_pdf_from_file(source, None)
            .map_err(map_pdfium_error)?;

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
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, None)
            .map_err(map_pdfium_error)?;

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
        if !source.is_file() {
            return Err(PdfError::FileNotFound(source.display().to_string()));
        }

        let pdfium = process_pdfium()?;
        let src_doc = pdfium
            .load_pdf_from_file(source, None)
            .map_err(map_pdfium_error)?;

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
            .load_pdf_from_file(source, None)
            .map_err(map_pdfium_error)?;

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
}
