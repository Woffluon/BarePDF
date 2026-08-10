use arboard::Clipboard;
use barepdf_core::PdfError;
use barepdf_platform::{ClipboardAccess, FileDialogs, PrinterAccess};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, ShellExecuteW, HDROP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, SW_SHOWNORMAL,
    WM_DROPFILES, WM_NCDESTROY, WNDPROC,
};

pub struct WindowsFileDialogs;

pub fn show_fatal_error(title: &str, description: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(title)
        .set_description(description)
        .set_buttons(MessageButtons::Ok)
        .show();
}

struct DropTarget {
    sender: SyncSender<Vec<PathBuf>>,
    previous_window_proc: isize,
}

static DROP_TARGETS: OnceLock<Mutex<std::collections::HashMap<usize, DropTarget>>> =
    OnceLock::new();

const MAX_DROP_FILES: u32 = 64;
const MAX_DROP_PATH_UNITS: usize = 32_768;

struct OwnedDropHandle(HDROP);

impl Drop for OwnedDropHandle {
    fn drop(&mut self) {
        // SAFETY: WM_DROPFILES transfers ownership of this handle to the window procedure.
        unsafe { DragFinish(self.0) };
    }
}

/// Enables Windows Explorer file drops for a Slint Win32 window. The receiver is intentionally
/// polled by the Slint event timer so the native window procedure never touches UI state.
///
/// # Safety
///
/// `hwnd` must be a valid, live window handle owned by the calling process.
pub unsafe fn install_file_drop(hwnd: HWND) -> Option<Receiver<Vec<PathBuf>>> {
    if hwnd.is_null() {
        return None;
    }
    let (sender, receiver) = mpsc::sync_channel(8);
    let previous_window_proc =
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, drop_window_proc as *const () as isize) };
    if previous_window_proc == 0 {
        return None;
    }
    let inserted = DROP_TARGETS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .ok()
        .map(|mut targets| {
            targets.insert(
                hwnd as usize,
                DropTarget {
                    sender,
                    previous_window_proc,
                },
            );
        });
    if inserted.is_none() {
        // SAFETY: Restore the exact window procedure replaced above when registration fails.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, previous_window_proc) };
        return None;
    }
    unsafe { DragAcceptFiles(hwnd, 1) };
    Some(receiver)
}

unsafe extern "system" fn drop_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DROPFILES {
        let _ = std::panic::catch_unwind(|| handle_drop_message(hwnd, message, wparam));
        return 0;
    }

    let _ = std::panic::catch_unwind(|| handle_drop_message(hwnd, message, wparam));

    forward_window_proc(hwnd, message, wparam, lparam)
}

fn handle_drop_message(hwnd: HWND, message: u32, wparam: WPARAM) -> Result<(), ()> {
    if message == WM_NCDESTROY {
        if let Some(targets) = DROP_TARGETS.get() {
            let mut targets = targets.lock().map_err(|_| ())?;
            targets.remove(&(hwnd as usize));
        }
        return Err(());
    }
    if message != WM_DROPFILES {
        return Err(());
    }

    let drop_handle = OwnedDropHandle(wparam as HDROP);
    // SAFETY: OwnedDropHandle owns the valid HDROP supplied by WM_DROPFILES.
    let count = unsafe { DragQueryFileW(drop_handle.0, u32::MAX, std::ptr::null_mut(), 0) };
    if count > MAX_DROP_FILES {
        return Err(());
    }
    let capacity = usize::try_from(count).map_err(|_| ())?;
    let mut paths = Vec::new();
    paths.try_reserve(capacity).map_err(|_| ())?;
    for index in 0..count {
        // SAFETY: Querying length does not write through the null buffer pointer.
        let length = unsafe { DragQueryFileW(drop_handle.0, index, std::ptr::null_mut(), 0) };
        let length = usize::try_from(length).map_err(|_| ())?;
        let buffer_len = length.checked_add(1).filter(|len| *len <= MAX_DROP_PATH_UNITS).ok_or(())?;
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(buffer_len).map_err(|_| ())?;
        buffer.resize(buffer_len, 0);
        let buffer_len_u32 = u32::try_from(buffer.len()).map_err(|_| ())?;
        // SAFETY: Buffer has exactly the declared writable UTF-16 capacity.
        let copied = unsafe {
            DragQueryFileW(drop_handle.0, index, buffer.as_mut_ptr(), buffer_len_u32)
        };
        if usize::try_from(copied).map_err(|_| ())? != length {
            return Err(());
        }
        buffer.truncate(length);
        paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
    }
    let targets = DROP_TARGETS.get().ok_or(())?.lock().map_err(|_| ())?;
    let target = targets.get(&(hwnd as usize)).ok_or(())?;
    target.sender.try_send(paths).map_err(|_| ())
}

unsafe fn forward_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let previous_window_proc = DROP_TARGETS
        .get()
        .and_then(|targets| targets.lock().ok())
        .and_then(|targets| {
            targets
                .get(&(hwnd as usize))
                .map(|target| target.previous_window_proc)
        });
    match previous_window_proc {
        Some(previous) => {
            // SAFETY: The procedure is retained only while its HWND registry entry is alive.
            let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
            unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
        }
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

impl FileDialogs for WindowsFileDialogs {
    fn pick_file(&self) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter("PDF Document", &["pdf"])
            .pick_file()
    }
}

pub struct WindowsClipboard {
    inner: Mutex<Option<Clipboard>>,
}

impl Default for WindowsClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsClipboard {
    #[must_use]
    pub fn new() -> Self {
        let cb = Clipboard::new().ok();
        Self {
            inner: Mutex::new(cb),
        }
    }
}

impl ClipboardAccess for WindowsClipboard {
    fn set_text(&self, text: &str) -> Result<(), PdfError> {
        if let Ok(mut lock) = self.inner.lock() {
            if let Some(ref mut cb) = *lock {
                return cb
                    .set_text(text)
                    .map_err(|e| PdfError::PlatformError(format!("Clipboard write error: {e}")));
            }
        }
        Err(PdfError::PlatformError("Clipboard unavailable".into()))
    }

    fn get_text(&self) -> Result<String, PdfError> {
        if let Ok(mut lock) = self.inner.lock() {
            if let Some(ref mut cb) = *lock {
                return cb
                    .get_text()
                    .map_err(|e| PdfError::PlatformError(format!("Clipboard read error: {e}")));
            }
        }
        Err(PdfError::PlatformError("Clipboard unavailable".into()))
    }
}

pub struct WindowsPrinter;

impl PrinterAccess for WindowsPrinter {
    fn print_file(&self, path: &Path) -> Result<(), PdfError> {
        let path = wide_null(path.as_os_str());
        let verb = wide_null(OsStr::new("print"));
        // SAFETY: Both strings are NUL-terminated and remain alive for the call.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if (result as usize) <= 32 {
            return Err(PdfError::PrintingFailed(format!(
                "ShellExecuteW failed with code {}",
                result as usize
            )));
        }

        Ok(())
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
