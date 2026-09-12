use barepdf_core::{DocumentId, PageIndex, SecretPassword};
use std::path::PathBuf;

/// Side-effects and asynchronous commands produced by pure `update` transitions.
#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    RequestPageRender {
        document_id: DocumentId,
        page_index: PageIndex,
    },
    OpenDocumentPath {
        path: PathBuf,
        password: Option<SecretPassword>,
    },
    SavePreferences,
    SyncWindowChrome,
    InvalidateCanvas,
    ExecutePrintDialog,
    ShowBanner {
        message: String,
        can_retry: bool,
    },
}
