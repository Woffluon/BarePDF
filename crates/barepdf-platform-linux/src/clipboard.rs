use arboard::Clipboard;
use barepdf_platform::PlatformError;
use std::sync::Mutex;

pub struct LinuxClipboard {
    inner: Mutex<Option<Clipboard>>,
}

impl LinuxClipboard {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Clipboard::new().ok()),
        }
    }

    pub fn set_text(&self, text: &str) -> Result<(), PlatformError> {
        let mut guard = self.inner.lock().map_err(|_| PlatformError::Unavailable {
            operation: "Clipboard",
        })?;
        if guard.is_none() {
            *guard = Clipboard::new().ok();
        }
        let cb = guard.as_mut().ok_or(PlatformError::Unavailable {
            operation: "Clipboard",
        })?;
        cb.set_text(text).map_err(|source| PlatformError::External {
            operation: "Could not write clipboard text",
            source: Box::new(source),
        })
    }

    pub fn get_text(&self) -> Result<String, PlatformError> {
        let mut guard = self.inner.lock().map_err(|_| PlatformError::Unavailable {
            operation: "Clipboard",
        })?;
        if guard.is_none() {
            *guard = Clipboard::new().ok();
        }
        let cb = guard.as_mut().ok_or(PlatformError::Unavailable {
            operation: "Clipboard",
        })?;
        cb.get_text().map_err(|source| PlatformError::External {
            operation: "Could not read clipboard text",
            source: Box::new(source),
        })
    }
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_clipboard_construction() {
        let clipboard = LinuxClipboard::new();
        let default_clipboard = LinuxClipboard::default();
        let _ = clipboard.set_text("test");
        let _ = default_clipboard.set_text("test");
    }
}
