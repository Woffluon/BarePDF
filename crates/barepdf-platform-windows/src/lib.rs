use arboard::Clipboard;
use barepdf_core::PdfError;
use barepdf_platform::{ClipboardAccess, FileDialogs, PrinterAccess};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, WM_DROPFILES, WNDPROC,
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
    DROP_TARGETS
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .ok()?
        .insert(
            hwnd as usize,
            DropTarget {
                sender,
                previous_window_proc,
            },
        );
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
        let drop_handle = wparam as HDROP;
        let count = unsafe { DragQueryFileW(drop_handle, u32::MAX, std::ptr::null_mut(), 0) };
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop_handle, index, std::ptr::null_mut(), 0) };
            let mut buffer = vec![0u16; length as usize + 1];
            unsafe { DragQueryFileW(drop_handle, index, buffer.as_mut_ptr(), buffer.len() as u32) };
            buffer.truncate(length as usize);
            paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
        }
        unsafe { DragFinish(drop_handle) };
        if let Some(targets) = DROP_TARGETS.get() {
            if let Ok(targets) = targets.lock() {
                if let Some(target) = targets.get(&(hwnd as usize)) {
                    let _ = target.sender.try_send(paths);
                }
            }
        }
        return 0;
    }

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
            let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
            unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
        }
        None => 0,
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
        let path_str = path
            .to_str()
            .ok_or_else(|| PdfError::PrintingFailed("Invalid path".into()))?;
        std::process::Command::new("powershell")
            .args([
                "-Command",
                &format!("Start-Process -FilePath \"{}\" -Verb Print", path_str),
            ])
            .spawn()
            .map_err(|e| PdfError::PrintingFailed(e.to_string()))?;

        Ok(())
    }
}
