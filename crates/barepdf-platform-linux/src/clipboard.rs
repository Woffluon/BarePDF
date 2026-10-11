use arboard::Clipboard;
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

    pub fn set_text(&self, text: &str) -> bool {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Clipboard::new().ok();
        }
        guard.as_mut().and_then(|c| c.set_text(text).ok()).is_some()
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
