use barepdf_core::PdfError;
use pdfium_render::prelude::Pdfium;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static PDFIUM_INIT: Mutex<()> = Mutex::new(());
static PDFIUM_FFI_LOCK: Mutex<()> = Mutex::new(());

/// Acquires the global process-wide lock for thread-unsafe PDFium FFI interactions.
///
/// Automatically recovers from mutex poisoning if a previous thread panicked while
/// holding the lock.
pub fn pdfium_ffi_lock() -> MutexGuard<'static, ()> {
    PDFIUM_FFI_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[doc(hidden)]
pub fn resolve_pdfium_library_path_with_policy(
    exe: &Path,
    dll_name: impl AsRef<Path>,
    allow_fallbacks: bool,
) -> Option<PathBuf> {
    let dll_name = dll_name.as_ref();
    let sibling = exe
        .parent()
        .map(|directory| directory.join(dll_name))
        .filter(|path| path.exists());

    if sibling.is_some() || !allow_fallbacks {
        return sibling;
    }

    #[cfg(any(test, debug_assertions))]
    {
        exe.parent()
            .and_then(|d| d.parent())
            .map(|directory| directory.join(dll_name))
            .filter(|path| path.exists())
            .or_else(|| {
                let target_release = PathBuf::from("target/release").join(dll_name);
                target_release.exists().then_some(target_release)
            })
            .or_else(|| {
                let target_debug = PathBuf::from("target/debug").join(dll_name);
                target_debug.exists().then_some(target_debug)
            })
    }

    #[cfg(not(any(test, debug_assertions)))]
    {
        None
    }
}

fn resolve_pdfium_library_path(exe: &Path, dll_name: impl AsRef<Path>) -> Option<PathBuf> {
    #[cfg(any(test, debug_assertions))]
    let allow_fallbacks = true;

    #[cfg(not(any(test, debug_assertions)))]
    let allow_fallbacks = false;

    resolve_pdfium_library_path_with_policy(exe, dll_name, allow_fallbacks)
}

pub(crate) fn process_pdfium() -> Result<&'static Pdfium, PdfError> {
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let _guard = PDFIUM_INIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let exe = std::env::current_exe().map_err(|error| {
        PdfError::PlatformError(format!("Cannot locate application executable: {error}"))
    })?;
    let dll_name = Pdfium::pdfium_platform_library_name();
    let library_path = resolve_pdfium_library_path(&exe, &dll_name)
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
