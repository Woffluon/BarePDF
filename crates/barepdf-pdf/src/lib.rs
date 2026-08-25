#![forbid(unsafe_code)]

pub mod backend;
pub mod operations;
pub mod pdfium_adapter;
mod pdfium_lifetime;

pub use backend::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
pub use operations::PdfOperations;
pub use pdfium_adapter::PdfiumEngine;
