use super::TabSet;
use barepdf_core::{DocumentId, PageCount};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) struct ReadyDocument {
    pub(super) id: DocumentId,
    pub(super) path: PathBuf,
    pub(super) page_count: PageCount,
    pub(super) started_at: Instant,
}

impl ReadyDocument {
    #[must_use]
    pub(crate) const fn id(&self) -> DocumentId {
        self.id
    }

    #[must_use]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub(crate) const fn page_count(&self) -> PageCount {
        self.page_count
    }

    #[must_use]
    pub(crate) const fn started_at(&self) -> Instant {
        self.started_at
    }
}

#[derive(Debug, Clone)]
pub(crate) enum DocumentState {
    Opening {
        id: DocumentId,
        path: PathBuf,
        started_at: Instant,
        active: Option<ReadyDocument>,
    },
    PasswordRequired {
        id: DocumentId,
        path: PathBuf,
        started_at: Instant,
        active: Option<ReadyDocument>,
    },
    Ready(ReadyDocument),
    Failed {
        path: PathBuf,
        active: Option<ReadyDocument>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct Application {
    pub(crate) tabs: TabSet,
}

impl Application {
    #[must_use]
    pub(crate) fn ready_document(&self) -> Option<&ReadyDocument> {
        match self.tabs.active()?.document.as_ref() {
            Some(DocumentState::Ready(document)) => Some(document),
            Some(
                DocumentState::Opening { active, .. }
                | DocumentState::PasswordRequired { active, .. }
                | DocumentState::Failed { active, .. },
            ) => active.as_ref(),
            _ => None,
        }
    }
}
