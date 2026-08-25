#![forbid(unsafe_code)]

use std::path::PathBuf;

mod error;
pub mod printing;

pub use error::PlatformError;

pub trait FileDialogs: Send + Sync {
    fn pick_file(&self) -> Option<PathBuf>;
    fn pick_multiple_files(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn save_file(&self, default_name: &str) -> Option<PathBuf> {
        let _ = default_name;
        None
    }
    fn pick_directory(&self) -> Option<PathBuf> {
        None
    }
}

pub trait ClipboardAccess: Send + Sync {
    /// # Errors
    ///
    /// Returns a platform error when clipboard ownership or data conversion fails.
    fn set_text(&self, text: &str) -> Result<(), PlatformError>;
    /// # Errors
    ///
    /// Returns a platform error when clipboard data cannot be read as text.
    fn get_text(&self) -> Result<String, PlatformError>;
}
