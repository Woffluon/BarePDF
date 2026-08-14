use raw_window_handle::WindowHandle;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// Enables Windows Explorer file drops for a live Win32 window.
///
/// Returns `None` for non-Win32 handles or when native registration fails.
#[must_use]
pub fn install_file_drop(window: WindowHandle<'_>) -> Option<Receiver<Vec<PathBuf>>> {
    crate::ffi::install_file_drop(window)
}
