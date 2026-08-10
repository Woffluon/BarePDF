use pdfium_render::prelude::*;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

const INITIAL_MODULE_PATH_CAPACITY: usize = 260;
const MAX_MODULE_PATH_CAPACITY: usize = 32 * 1024;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static PDFIUM_INIT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub enum PdfiumLoadError {
    ModulePathUnavailable,
    ModulePathTooLong,
    InvalidModulePath,
    SiblingLibraryMissing,
    Bind(PdfiumError),
    InitializationLockPoisoned,
    InitializationRace,
}

impl fmt::Display for PdfiumLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModulePathUnavailable => {
                formatter.write_str("thumbnail module path is unavailable")
            }
            Self::ModulePathTooLong => formatter.write_str("thumbnail module path is too long"),
            Self::InvalidModulePath => {
                formatter.write_str("thumbnail module path is invalid UTF-16")
            }
            Self::SiblingLibraryMissing => formatter.write_str("sibling pdfium.dll is missing"),
            Self::Bind(_) => formatter.write_str("failed to bind sibling pdfium.dll"),
            Self::InitializationLockPoisoned => {
                formatter.write_str("PDFium initialization lock is poisoned")
            }
            Self::InitializationRace => {
                formatter.write_str("PDFium initialization state changed unexpectedly")
            }
        }
    }
}

impl Error for PdfiumLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bind(source) => Some(source),
            _ => None,
        }
    }
}

/// Resolves `pdfium.dll` beside this COM server, without DLL search-path fallback.
///
/// # Errors
///
/// Returns an error when module path resolution fails, the path is malformed or too long, or the
/// sibling `pdfium.dll` file is absent.
pub fn get_installed_pdfium_path(hinstance: HMODULE) -> Result<PathBuf, PdfiumLoadError> {
    let mut capacity = INITIAL_MODULE_PATH_CAPACITY;

    loop {
        let mut buffer = vec![0_u16; capacity];
        // SAFETY: `buffer` is writable UTF-16 storage supplied with its exact element count.
        let length = unsafe { GetModuleFileNameW(hinstance, &mut buffer) };
        let length = usize::try_from(length).map_err(|_| PdfiumLoadError::ModulePathTooLong)?;

        if length == 0 {
            return Err(PdfiumLoadError::ModulePathUnavailable);
        }

        if length < buffer.len() {
            let module_path = String::from_utf16(&buffer[..length])
                .map_err(|_| PdfiumLoadError::InvalidModulePath)?;
            let parent = Path::new(&module_path)
                .parent()
                .ok_or(PdfiumLoadError::ModulePathUnavailable)?;
            let pdfium_path = parent.join("pdfium.dll");

            return pdfium_path
                .is_file()
                .then_some(pdfium_path)
                .ok_or(PdfiumLoadError::SiblingLibraryMissing);
        }

        capacity = capacity
            .checked_mul(2)
            .filter(|next| *next <= MAX_MODULE_PATH_CAPACITY)
            .ok_or(PdfiumLoadError::ModulePathTooLong)?;
    }
}

/// Initializes the process-wide `PDFium` binding exactly once from the sibling DLL.
///
/// # Errors
///
/// Returns an error when initialization cannot resolve or bind the sibling `pdfium.dll` file.
pub fn init_pdfium(hinstance: HMODULE) -> Result<&'static Pdfium, PdfiumLoadError> {
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let _guard = PDFIUM_INIT_LOCK
        .lock()
        .map_err(|_| PdfiumLoadError::InitializationLockPoisoned)?;

    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let pdfium_path = get_installed_pdfium_path(hinstance)?;
    let bindings = Pdfium::bind_to_library(pdfium_path).map_err(PdfiumLoadError::Bind)?;
    let pdfium = Pdfium::new(bindings);

    PDFIUM
        .set(pdfium)
        .map_err(|_| PdfiumLoadError::InitializationRace)?;
    PDFIUM.get().ok_or(PdfiumLoadError::InitializationRace)
}
