use arboard::Clipboard;
use barepdf_platform::{ClipboardAccess, PlatformError};
use std::sync::Mutex;

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
    fn set_text(&self, text: &str) -> Result<(), PlatformError> {
        if let Ok(mut lock) = self.inner.lock() {
            if let Some(ref mut cb) = *lock {
                return cb.set_text(text).map_err(|source| PlatformError::External {
                    operation: "Could not write clipboard text",
                    source: Box::new(source),
                });
            }
        }
        Err(PlatformError::Unavailable {
            operation: "Clipboard",
        })
    }

    fn get_text(&self) -> Result<String, PlatformError> {
        if let Ok(mut lock) = self.inner.lock() {
            if let Some(ref mut cb) = *lock {
                return cb.get_text().map_err(|source| PlatformError::External {
                    operation: "Could not read clipboard text",
                    source: Box::new(source),
                });
            }
        }
        Err(PlatformError::Unavailable {
            operation: "Clipboard",
        })
    }
}
