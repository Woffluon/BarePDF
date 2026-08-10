use barepdf_core::PdfError;
use std::path::{Path, PathBuf};

pub trait FileDialogs: Send + Sync {
    fn pick_file(&self) -> Option<PathBuf>;
}

pub trait ClipboardAccess: Send + Sync {
    /// # Errors
    ///
    /// Returns a platform error when clipboard ownership or data conversion fails.
    fn set_text(&self, text: &str) -> Result<(), PdfError>;
    /// # Errors
    ///
    /// Returns a platform error when clipboard data cannot be read as text.
    fn get_text(&self) -> Result<String, PdfError>;
}

pub trait PrinterAccess: Send + Sync {
    /// # Errors
    ///
    /// Returns a platform error when the operating system rejects the print request.
    fn print_file(&self, path: &Path) -> Result<(), PdfError>;
}
