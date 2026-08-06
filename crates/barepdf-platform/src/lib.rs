use barepdf_core::PdfError;
use std::path::{Path, PathBuf};

pub trait FileDialogs: Send + Sync {
    fn pick_file(&self) -> Option<PathBuf>;
}

pub trait ClipboardAccess: Send + Sync {
    fn set_text(&self, text: &str) -> Result<(), PdfError>;
    fn get_text(&self) -> Result<String, PdfError>;
}

pub trait PrinterAccess: Send + Sync {
    fn print_file(&self, path: &Path) -> Result<(), PdfError>;
}
