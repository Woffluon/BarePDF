use arboard::Clipboard;
use barepdf_core::PdfError;
use barepdf_platform::{ClipboardAccess, FileDialogs, PrinterAccess};
use rfd::FileDialog;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct WindowsFileDialogs;

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
            if lock.is_none() {
                *lock = Clipboard::new().ok();
            }
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
            if lock.is_none() {
                *lock = Clipboard::new().ok();
            }
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
