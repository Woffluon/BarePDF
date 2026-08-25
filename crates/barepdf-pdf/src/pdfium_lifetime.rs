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

    let exe = std::env::current_exe().map_err(|error| {
        PdfError::PlatformError(format!("Cannot locate application executable: {error}"))
    })?;
    let dll_name = Pdfium::pdfium_platform_library_name();
    let library_path = exe
        .parent()
        .map(|directory| directory.join(&dll_name))
        .filter(|path| path.exists())
        .or_else(|| {
            exe.parent()
                .and_then(|d| d.parent())
                .map(|directory| directory.join(&dll_name))
                .filter(|path| path.exists())
        })
        .or_else(|| {
            let target_release = std::path::PathBuf::from("target/release").join(&dll_name);
            target_release.exists().then_some(target_release)
        })
        .or_else(|| {
            let target_debug = std::path::PathBuf::from("target/debug").join(&dll_name);
            target_debug.exists().then_some(target_debug)
        })
        .ok_or_else(|| {
            PdfError::PlatformError("Cannot locate sibling PDFium library: file not found".into())
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
