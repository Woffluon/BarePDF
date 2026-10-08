#![forbid(unsafe_code)]

pub mod backend;
pub mod conversion;
pub mod operations;
pub mod pdfium_adapter;
pub mod pdfium_lifetime;
mod text;

pub use backend::{InvalidBitmap, OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
pub use barepdf_core::{CancellationToken, EncodedImageFormat, ImageEncodeError, ImageEncoder};
pub use operations::{PdfOperationInput, PdfOperations};
pub use pdfium_adapter::PdfiumEngine;
pub use pdfium_lifetime::pdfium_ffi_lock;
