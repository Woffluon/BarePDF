use barepdf_core::PdfError;
use pdfium_render::prelude::Pdfium;
use std::sync::{Mutex, OnceLock};

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static PDFIUM_INIT: Mutex<()> = Mutex::new(());

pub(crate) fn process_pdfium() -> Result<&'static Pdfium, PdfError> {
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let _guard = PDFIUM_INIT
        .lock()
        .map_err(|_| PdfError::PlatformError("PDFium initialization lock was poisoned".into()))?;
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let library_path = std::env::current_exe()
        .map_err(|error| {
            PdfError::PlatformError(format!("Cannot locate application executable: {error}"))
        })?
        .parent()
        .map(|directory| directory.join(Pdfium::pdfium_platform_library_name()))
        .ok_or_else(|| {
            PdfError::PlatformError("Application executable has no parent directory".into())
        })?
        .canonicalize()
        .map_err(|error| {
            PdfError::PlatformError(format!("Cannot locate sibling PDFium library: {error}"))
        })?;
    let bindings = Pdfium::bind_to_library(library_path).map_err(|error| {
        PdfError::PlatformError(format!("Failed to bind PDFium library: {error}"))
    })?;
    PDFIUM
        .set(Pdfium::new(bindings))
        .map_err(|_| PdfError::PlatformError("PDFium was initialized concurrently".into()))?;
    PDFIUM
        .get()
        .ok_or_else(|| PdfError::PlatformError("PDFium initialization failed".into()))
}
