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

    #[cfg(test)]
    fn new_uninitialized() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Writes `text` to the system clipboard, lazily initializing the clipboard handle if it was
    /// temporarily locked at startup.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError`] if the clipboard lock is poisoned, initialization fails, or
    /// writing text fails.
    pub fn copy_text(&self, text: &str) -> Result<(), PlatformError> {
        self.set_text(text)
    }

    fn with_clipboard<T>(
        &self,
        operation: &'static str,
        f: impl FnOnce(&mut Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, PlatformError> {
        let mut lock = self.inner.lock().map_err(|_| PlatformError::Unavailable {
            operation: "Clipboard",
        })?;

        if lock.is_none() {
            match Clipboard::new() {
                Ok(cb) => {
                    *lock = Some(cb);
                }
                Err(source) => {
                    return Err(PlatformError::External {
                        operation: "Could not initialize clipboard",
                        source: Box::new(source),
                    });
                }
            }
        }

        let cb = lock.as_mut().ok_or(PlatformError::Unavailable {
            operation: "Clipboard",
        })?;

        f(cb).map_err(|source| PlatformError::External {
            operation,
            source: Box::new(source),
        })
    }
}

impl ClipboardAccess for WindowsClipboard {
    fn set_text(&self, text: &str) -> Result<(), PlatformError> {
        self.with_clipboard("Could not write clipboard text", |cb| cb.set_text(text))
    }

    fn get_text(&self) -> Result<String, PlatformError> {
        self.with_clipboard("Could not read clipboard text", Clipboard::get_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_retry_recovers_from_uninitialized_state() {
        let clipboard = WindowsClipboard::new_uninitialized();
        assert!(clipboard.inner.lock().unwrap().is_none());

        // Attempting copy_text / set_text lazily initializes the clipboard (or returns an explicit External error if headless/locked)
        let res = clipboard.copy_text("barepdf-lazy-retry-test");
        match res {
            Ok(()) => {
                assert!(clipboard.inner.lock().unwrap().is_some());
            }
            Err(PlatformError::External { operation, .. }) => {
                assert!(
                    operation == "Could not initialize clipboard"
                        || operation == "Could not write clipboard text"
                );
            }
            Err(other) => panic!("Unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn poisoned_mutex_returns_unavailable_error() {
        let clipboard = std::sync::Arc::new(WindowsClipboard::new_uninitialized());
        let clone = std::sync::Arc::clone(&clipboard);
        let _ = std::thread::spawn(move || {
            let _guard = clone.inner.lock().unwrap();
            panic!("poison clipboard mutex");
        })
        .join();

        let err = clipboard.set_text("test").unwrap_err();
        assert!(matches!(
            err,
            PlatformError::Unavailable {
                operation: "Clipboard"
            }
        ));
    }
}
