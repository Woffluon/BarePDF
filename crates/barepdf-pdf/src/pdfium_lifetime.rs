use barepdf_core::PdfError;
use pdfium_render::prelude::Pdfium;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static PDFIUM_INIT: Mutex<()> = Mutex::new(());
static PDFIUM_FFI_GATE: PriorityFfiGate = PriorityFfiGate::new();

/// Priority tier for acquiring the process-wide PDFium FFI gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiPriority {
    /// Interactive UI-facing operations (document open, visible render, text/geometry, outline, form/annotation reads).
    Interactive,
    /// Batch background operations (`operations.rs` tools) that yield between pages when interactive work waits.
    Background,
}

/// Process-wide FFI gate that gives waiting [`FfiPriority::Interactive`] tasks precedence over [`FfiPriority::Background`] batch work.
pub struct PriorityFfiGate {
    state: Mutex<bool>,
    interactive_waiters: AtomicUsize,
    cvar: Condvar,
}

/// RAII guard releasing [`PriorityFfiGate`] when dropped.
pub struct PriorityFfiGuard<'a> {
    gate: &'a PriorityFfiGate,
}

impl PriorityFfiGate {
    /// Creates an unlocked priority gate.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(false),
            interactive_waiters: AtomicUsize::new(0),
            cvar: Condvar::new(),
        }
    }

    /// Acquires exclusive access to the FFI gate according to `priority`.
    #[must_use]
    pub fn acquire(&self, priority: FfiPriority) -> PriorityFfiGuard<'_> {
        match priority {
            FfiPriority::Interactive => {
                self.interactive_waiters.fetch_add(1, Ordering::AcqRel);
                let mut held = lock_unpoison(&self.state);
                while *held {
                    held = wait_unpoison(&self.cvar, held);
                }
                *held = true;
                self.interactive_waiters.fetch_sub(1, Ordering::AcqRel);
            }
            FfiPriority::Background => {
                let mut held = lock_unpoison(&self.state);
                while *held || self.interactive_waiters.load(Ordering::Acquire) > 0 {
                    held = wait_unpoison(&self.cvar, held);
                }
                *held = true;
            }
        }
        PriorityFfiGuard { gate: self }
    }

    /// Returns `true` when at least one interactive caller is waiting to acquire the gate.
    #[must_use]
    pub fn has_interactive_waiters(&self) -> bool {
        self.interactive_waiters.load(Ordering::Acquire) > 0
    }
}

impl Default for PriorityFfiGate {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PriorityFfiGuard<'_> {
    fn drop(&mut self) {
        let mut held = lock_unpoison(&self.gate.state);
        *held = false;
        self.gate.cvar.notify_all();
    }
}

fn lock_unpoison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wait_unpoison<'a, T>(cvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    cvar.wait(guard)
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Yields the FFI gate between pages when a background operation detects a waiting interactive task.
pub fn yield_ffi_between_pages<'a>(
    gate: &'a PriorityFfiGate,
    guard: &mut Option<PriorityFfiGuard<'a>>,
) {
    if gate.has_interactive_waiters() {
        drop(guard.take());
        std::thread::yield_now();
        *guard = Some(gate.acquire(FfiPriority::Background));
    }
}

/// Returns the global process-wide PDFium priority FFI gate.
#[must_use]
pub fn pdfium_ffi_gate() -> &'static PriorityFfiGate {
    &PDFIUM_FFI_GATE
}

/// Acquires the global PDFium FFI gate with the specified priority.
#[must_use]
pub fn acquire_pdfium_ffi(priority: FfiPriority) -> PriorityFfiGuard<'static> {
    PDFIUM_FFI_GATE.acquire(priority)
}

/// Acquires the global process-wide lock for thread-unsafe PDFium FFI interactions at [`FfiPriority::Background`] priority.
///
/// Automatically recovers from mutex poisoning if a previous thread panicked while
/// holding the lock.
#[must_use]
pub fn pdfium_ffi_lock() -> PriorityFfiGuard<'static> {
    acquire_pdfium_ffi(FfiPriority::Background)
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

    if sibling.is_some() {
        return sibling;
    }

    #[cfg(target_os = "linux")]
    {
        let appimage_lib = exe
            .parent()
            .and_then(|d| d.parent())
            .map(|d| d.join("lib").join(dll_name))
            .filter(|path| path.exists());
        if appimage_lib.is_some() {
            return appimage_lib;
        }

        let system_path = PathBuf::from("/usr/lib/barepdf").join(dll_name);
        if system_path.exists() {
            return Some(system_path);
        }
    }

    if !allow_fallbacks {
        return None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_pdfium_library_path_with_policy_resolves_candidate_when_given_paths() {
        let temp = tempdir().expect("create temp dir");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let fake_exe = bin_dir.join("barepdf");
        fs::write(&fake_exe, b"").expect("write fake exe");

        let dll_name = "libpdfium.so";

        // When no candidate exists and fallbacks are disabled: None
        assert_eq!(
            resolve_pdfium_library_path_with_policy(&fake_exe, dll_name, false),
            None
        );

        // When sibling candidate exists: returns sibling path even with fallbacks disabled
        let sibling = bin_dir.join(dll_name);
        fs::write(&sibling, b"test-lib").expect("write sibling");
        assert_eq!(
            resolve_pdfium_library_path_with_policy(&fake_exe, dll_name, false),
            Some(sibling.clone())
        );

        // When fallbacks are enabled: returns sibling path
        assert_eq!(
            resolve_pdfium_library_path_with_policy(&fake_exe, dll_name, true),
            Some(sibling)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_pdfium_library_path_with_policy_resolves_linux_appimage_lib_path() {
        let temp = tempdir().expect("create temp dir");
        let bin_dir = temp.path().join("usr/bin");
        let lib_dir = temp.path().join("usr/lib");
        fs::create_dir_all(&bin_dir).expect("create usr/bin dir");
        fs::create_dir_all(&lib_dir).expect("create usr/lib dir");
        let fake_exe = bin_dir.join("barepdf");
        fs::write(&fake_exe, b"").expect("write fake exe");
        let bundled_lib = lib_dir.join("libpdfium.so");
        fs::write(&bundled_lib, b"test-lib").expect("write bundled lib");

        assert_eq!(
            resolve_pdfium_library_path_with_policy(&fake_exe, "libpdfium.so", false),
            Some(bundled_lib)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_pdfium_library_path_with_policy_resolves_linux_system_path_when_present() {
        let exe = Path::new("/usr/bin/barepdf");
        let dll_name = "libpdfium.so";
        let system_candidate = PathBuf::from("/usr/lib/barepdf").join(dll_name);
        if system_candidate.exists() {
            assert_eq!(
                resolve_pdfium_library_path_with_policy(exe, dll_name, false),
                Some(system_candidate)
            );
        }
    }
}
