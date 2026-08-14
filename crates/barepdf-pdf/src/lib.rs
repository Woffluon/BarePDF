#![forbid(unsafe_code)]

pub mod backend;
pub mod pdfium_adapter;
mod pdfium_lifetime;

pub use backend::{OutlineNode, PdfBackend, PdfDocument, RawBitmap, TextSpan};
pub use pdfium_adapter::PdfiumEngine;
