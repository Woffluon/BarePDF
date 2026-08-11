use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Invalid or malformed PDF: {source}")]
    InvalidPdf {
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },

    #[error("Invalid or malformed PDF: {0}")]
    InvalidPdfReason(String),

    #[error("PDF file access failed: {source}")]
    FileAccess {
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },

    #[error("PDF backend failed: {source}")]
    Backend {
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },

    #[error("Corrupted document header or structure: {0}")]
    CorruptedPdf(String),

    #[error("Password required to open document")]
    PasswordRequired,

    #[error("Incorrect password specified")]
    IncorrectPassword,

    #[error("Unsupported security or encryption algorithm: {source}")]
    UnsupportedEncryption {
        #[source]
        source: Arc<dyn std::error::Error + Send + Sync>,
    },

    #[error("Failed to render page {page_index}: {reason}")]
    RenderingFailed { page_index: u32, reason: String },

    #[error("Text extraction failed for page {page_index}: {reason}")]
    TextExtractionFailed { page_index: u32, reason: String },

    #[error("Operation cancelled due to memory budget ceiling ({0} bytes)")]
    OutOfMemoryBudget(usize),

    #[error("Print operation failed: {0}")]
    PrintingFailed(String),

    #[error("Cache access error: {0}")]
    CacheError(String),

    #[error("Platform API failure: {0}")]
    PlatformError(String),

    #[error("Document worker terminated unexpectedly")]
    WorkerTerminated,
}
