use super::{Application, DocumentState, ReadyDocument};
use barepdf_core::{DocumentId, PageCount, PageIndex};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub(crate) enum OpenTransition {
    Ready(ReadyDocument),
    InvalidPageCount,
    Stale,
}

pub(crate) struct DocumentController;

impl DocumentController {
    pub(crate) fn begin_open(
        application: &mut Application,
        id: DocumentId,
        path: PathBuf,
        started_at: Instant,
    ) {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.pdf")
            .to_string();
        if application.tabs.active().and_then(|tab| tab.path.as_ref()) != Some(&path) {
            let _ = application.tabs.open(path.clone(), title);
        }
        let active = Self::take_active(application);
        if let Some(tab) = application.tabs.active_mut() {
            tab.document = Some(DocumentState::Opening {
                id,
                path,
                started_at,
                active,
            });
        }
    }

    #[must_use]
    pub(crate) fn opened(
        application: &mut Application,
        id: DocumentId,
        raw_page_count: u32,
        max_page_count: u32,
    ) -> OpenTransition {
        let Some((path, started_at)) = Self::pending(application, id) else {
            return OpenTransition::Stale;
        };
        let Some(page_count) =
            PageCount::new(raw_page_count).filter(|page_count| page_count.get() <= max_page_count)
        else {
            let active = Self::take_active(application);
            if let Some(tab) = application.tabs.active_mut() {
                tab.document = Some(DocumentState::Failed { path, active });
            }
            return OpenTransition::InvalidPageCount;
        };
        let ready = ReadyDocument {
            id,
            path,
            page_count,
            started_at,
        };
        if let Some(tab) = application.tabs.active_mut() {
            tab.document = Some(DocumentState::Ready(ready.clone()));
        }
        OpenTransition::Ready(ready)
    }

    #[must_use]
    pub(crate) fn require_password(application: &mut Application, id: DocumentId) -> bool {
        let Some((path, started_at)) = Self::pending(application, id) else {
            return false;
        };
        let active = Self::take_active(application);
        if let Some(tab) = application.tabs.active_mut() {
            tab.document = Some(DocumentState::PasswordRequired {
                id,
                path,
                started_at,
                active,
            });
        }
        true
    }

    #[must_use]
    pub(crate) fn fail(application: &mut Application, id: DocumentId) -> Option<PathBuf> {
        let (path, _) = Self::pending(application, id)?;
        let active = Self::take_active(application);
        if let Some(tab) = application.tabs.active_mut() {
            tab.document = Some(DocumentState::Failed {
                path: path.clone(),
                active,
            });
        }
        Some(path)
    }

    pub(crate) fn cancel_open(application: &mut Application, id: DocumentId) {
        if Self::pending(application, id).is_some() {
            let active = Self::take_active(application).map(DocumentState::Ready);
            if let Some(tab) = application.tabs.active_mut() {
                tab.document = active;
            }
        }
    }

    pub(crate) fn fail_active_path(application: &mut Application, path: PathBuf) {
        if let Some(tab) = application.tabs.active_mut() {
            tab.document = Some(DocumentState::Failed { path, active: None });
        }
    }

    #[must_use]
    pub(crate) fn pending_path(application: &Application) -> Option<&Path> {
        match application.tabs.active()?.document.as_ref() {
            Some(
                DocumentState::Opening { path, .. } | DocumentState::PasswordRequired { path, .. },
            ) => Some(path),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn is_pending(application: &Application, id: DocumentId) -> bool {
        Self::pending(application, id).is_some()
    }

    #[must_use]
    pub(crate) fn failed_path(application: &Application) -> Option<&Path> {
        match application.tabs.active()?.document.as_ref() {
            Some(DocumentState::Failed { path, .. }) => Some(path),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn page_index(application: &Application, raw: u32) -> Option<PageIndex> {
        PageIndex::new(raw, application.ready_document()?.page_count())
    }

    fn pending(application: &Application, id: DocumentId) -> Option<(PathBuf, Instant)> {
        match application.tabs.active()?.document.as_ref() {
            Some(
                DocumentState::Opening {
                    id: pending_id,
                    path,
                    started_at,
                    ..
                }
                | DocumentState::PasswordRequired {
                    id: pending_id,
                    path,
                    started_at,
                    ..
                },
            ) if *pending_id == id => Some((path.clone(), *started_at)),
            _ => None,
        }
    }

    fn take_active(application: &mut Application) -> Option<ReadyDocument> {
        match application
            .tabs
            .active_mut()
            .and_then(|tab| tab.document.take())
        {
            Some(DocumentState::Ready(document)) => Some(document),
            Some(
                DocumentState::Opening { active, .. }
                | DocumentState::PasswordRequired { active, .. }
                | DocumentState::Failed { active, .. },
            ) => active,
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening() -> (Application, DocumentId, PathBuf) {
        let mut application = Application::default();
        let id = DocumentId::new(7);
        let path = PathBuf::from("fixture.pdf");
        DocumentController::begin_open(&mut application, id, path.clone(), Instant::now());
        (application, id, path)
    }

    #[test]
    fn stale_open_event_does_not_replace_pending_document() {
        let (mut application, _, path) = opening();

        assert!(matches!(
            DocumentController::opened(&mut application, DocumentId::new(8), 3, 10_000),
            OpenTransition::Stale
        ));
        assert_eq!(
            DocumentController::pending_path(&application),
            Some(path.as_path())
        );
    }

    #[test]
    fn password_flow_preserves_pending_document_until_opened() {
        let (mut application, id, path) = opening();

        assert!(DocumentController::require_password(&mut application, id));
        assert_eq!(
            DocumentController::pending_path(&application),
            Some(path.as_path())
        );
        assert!(matches!(
            DocumentController::opened(&mut application, id, 3, 10_000),
            OpenTransition::Ready(_)
        ));
        assert!(DocumentController::pending_path(&application).is_none());
        assert_eq!(
            application.ready_document().map(ReadyDocument::path),
            Some(path.as_path())
        );
    }

    #[test]
    fn invalid_page_count_becomes_retryable_failure() {
        let (mut application, id, path) = opening();

        assert!(matches!(
            DocumentController::opened(&mut application, id, 10_001, 10_000),
            OpenTransition::InvalidPageCount
        ));
        assert_eq!(
            DocumentController::failed_path(&application),
            Some(path.as_path())
        );
        assert!(application.ready_document().is_none());
    }

    #[test]
    fn page_index_is_bounded_by_ready_document() {
        let (mut application, id, _) = opening();
        assert!(matches!(
            DocumentController::opened(&mut application, id, 2, 10_000),
            OpenTransition::Ready(_)
        ));

        assert_eq!(
            DocumentController::page_index(&application, 1).map(PageIndex::get),
            Some(1)
        );
        assert!(DocumentController::page_index(&application, 2).is_none());
    }

    #[test]
    fn opening_replacement_keeps_previous_tab_ready() {
        let (mut application, id, original_path) = opening();
        assert!(matches!(
            DocumentController::opened(&mut application, id, 2, 10_000),
            OpenTransition::Ready(_)
        ));

        DocumentController::begin_open(
            &mut application,
            DocumentId::new(8),
            PathBuf::from("replacement.pdf"),
            Instant::now(),
        );

        assert!(application.tabs.tabs().iter().any(|tab| {
            tab.path.as_deref() == Some(original_path.as_path())
                && matches!(tab.document, Some(DocumentState::Ready(_)))
        }));
    }

    #[test]
    fn event_from_background_tab_is_stale() {
        let (mut application, first_id, _) = opening();
        assert!(matches!(
            DocumentController::opened(&mut application, first_id, 2, 10_000),
            OpenTransition::Ready(_)
        ));
        let second_id = DocumentId::new(8);
        DocumentController::begin_open(
            &mut application,
            second_id,
            PathBuf::from("second.pdf"),
            Instant::now(),
        );

        assert!(matches!(
            DocumentController::opened(&mut application, first_id, 2, 10_000),
            OpenTransition::Stale
        ));
    }

    #[test]
    fn missing_dormant_path_becomes_failed_without_active_document() {
        let (mut application, id, path) = opening();
        assert!(matches!(
            DocumentController::opened(&mut application, id, 1, 10_000),
            OpenTransition::Ready(_)
        ));

        DocumentController::fail_active_path(&mut application, path.clone());

        assert_eq!(
            DocumentController::failed_path(&application),
            Some(path.as_path())
        );
        assert!(application.ready_document().is_none());
    }
}
