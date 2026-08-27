#![forbid(unsafe_code)]

pub mod backend;
pub mod conversion;
pub mod operations;
pub mod pdfium_adapter;
mod pdfium_lifetime;
mod text;

pub use backend::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
pub use operations::{PdfOperationInput, PdfOperations};
pub use pdfium_adapter::PdfiumEngine;
