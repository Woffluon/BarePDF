use pdfium_render::prelude::*;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

const INITIAL_MODULE_PATH_CAPACITY: usize = 260;
const MAX_MODULE_PATH_CAPACITY: usize = 32 * 1024;

/// Process-wide `PDFium` instance bound on first thumbnail request.
///
/// Rust `static` items in a `cdylib` are not dropped automatically when `DLL_PROCESS_DETACH` fires,
/// which is required here for safety: `Pdfium`'s `Drop` implementation calls `FPDF_DestroyLibrary`
/// and `FreeLibrary` (via `libloading`), both of which are forbidden inside `DllMain` while the
/// Windows loader lock is held. Keeping `OnceLock<Pdfium>` alive until the host process (`prevhost`
/// / `dllhost` / `explorer`) exits or unloads the module avoids loader-lock deadlocks.
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
    resolve_pdfium_path_with(|buffer| {
        // SAFETY: `buffer` is writable UTF-16 storage supplied with its exact element count.
        unsafe { GetModuleFileNameW(hinstance, buffer) }
    })
}

fn resolve_pdfium_path_with<F>(mut read_module_path: F) -> Result<PathBuf, PdfiumLoadError>
where
    F: FnMut(&mut [u16]) -> u32,
{
    let mut capacity = INITIAL_MODULE_PATH_CAPACITY;

    loop {
        let mut buffer = vec![0_u16; capacity];
        let length = read_module_path(&mut buffer);
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
    init_pdfium_with(|| get_installed_pdfium_path(hinstance))
}

fn init_pdfium_with<F>(resolve_path: F) -> Result<&'static Pdfium, PdfiumLoadError>
where
    F: FnOnce() -> Result<PathBuf, PdfiumLoadError>,
{
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let _guard = PDFIUM_INIT_LOCK
        .lock()
        .map_err(|_| PdfiumLoadError::InitializationLockPoisoned)?;

    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }

    let pdfium_path = resolve_path()?;
    let bindings = Pdfium::bind_to_library(pdfium_path).map_err(PdfiumLoadError::Bind)?;
    let pdfium = Pdfium::new(bindings);

    PDFIUM
        .set(pdfium)
        .map_err(|_| PdfiumLoadError::InitializationRace)?;
    PDFIUM.get().ok_or(PdfiumLoadError::InitializationRace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use tempfile::tempdir;

    fn copy_utf16(units: &[u16], buffer: &mut [u16]) -> u32 {
        let count = units.len().min(buffer.len());
        buffer[..count].copy_from_slice(&units[..count]);
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn copy_path_utf16(path: &Path, buffer: &mut [u16]) -> u32 {
        let units: Vec<u16> = path.as_os_str().encode_wide().collect();
        copy_utf16(&units, buffer)
    }

    #[test]
    fn sibling_library_missing_returns_error() {
        let dir = tempdir().expect("tempdir");
        let module_path = dir.path().join("barepdf_thumbnail.dll");

        let result = resolve_pdfium_path_with(|buf| copy_path_utf16(&module_path, buf));
        assert!(matches!(
            result,
            Err(PdfiumLoadError::SiblingLibraryMissing)
        ));
    }

    #[test]
    fn sibling_library_directory_is_rejected_as_missing() {
        let dir = tempdir().expect("tempdir");
        let module_path = dir.path().join("barepdf_thumbnail.dll");
        fs::create_dir(dir.path().join("pdfium.dll")).expect("create pdfium directory");

        let result = resolve_pdfium_path_with(|buf| copy_path_utf16(&module_path, buf));
        assert!(matches!(
            result,
            Err(PdfiumLoadError::SiblingLibraryMissing)
        ));
    }

    #[test]
    fn sibling_library_present_returns_pdfium_path() {
        let dir = tempdir().expect("tempdir");
        let module_path = dir.path().join("barepdf_thumbnail.dll");
        let expected_pdfium = dir.path().join("pdfium.dll");
        fs::write(&expected_pdfium, b"stub").expect("create sibling pdfium file");

        let result = resolve_pdfium_path_with(|buf| copy_path_utf16(&module_path, buf))
            .expect("sibling pdfium path");
        assert_eq!(result, expected_pdfium);
    }

    #[test]
    fn missing_and_invalid_module_paths_are_rejected() {
        let empty = resolve_pdfium_path_with(|_| 0);
        assert!(matches!(empty, Err(PdfiumLoadError::ModulePathUnavailable)));

        let root_only = resolve_pdfium_path_with(|buf| copy_path_utf16(Path::new("C:\\"), buf));
        assert!(matches!(
            root_only,
            Err(PdfiumLoadError::ModulePathUnavailable)
        ));

        let invalid_utf16 = [0xD800_u16, b'a' as u16];
        let invalid = resolve_pdfium_path_with(|buf| copy_utf16(&invalid_utf16, buf));
        assert!(matches!(invalid, Err(PdfiumLoadError::InvalidModulePath)));
    }

    #[test]
    fn long_module_path_grows_buffer_and_exceeding_max_fails() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("a".repeat(140)).join("b".repeat(140));
        fs::create_dir_all(&nested).expect("create nested dirs");
        let module_path = nested.join("barepdf_thumbnail.dll");
        let expected_pdfium = nested.join("pdfium.dll");
        fs::write(&expected_pdfium, b"stub").expect("create sibling pdfium file");

        let mut attempts = 0usize;
        let resolved = resolve_pdfium_path_with(|buf| {
            attempts += 1;
            copy_path_utf16(&module_path, buf)
        })
        .expect("path should resolve after doubling capacity");
        assert_eq!(resolved, expected_pdfium);
        assert!(attempts >= 2);

        let too_long = resolve_pdfium_path_with(|buf| u32::try_from(buf.len()).unwrap_or(u32::MAX));
        assert!(matches!(too_long, Err(PdfiumLoadError::ModulePathTooLong)));
    }

    #[test]
    fn init_pdfium_propagates_sibling_library_missing_error() {
        let dir = tempdir().expect("tempdir");
        let module_path = dir.path().join("barepdf_thumbnail.dll");

        let result =
            init_pdfium_with(|| resolve_pdfium_path_with(|buf| copy_path_utf16(&module_path, buf)));
        assert!(matches!(
            result,
            Err(PdfiumLoadError::SiblingLibraryMissing)
        ));
        assert!(PDFIUM.get().is_none());
    }
}
