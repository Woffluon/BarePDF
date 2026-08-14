use super::Application;
use barepdf_core::DocumentId;

pub(crate) struct RenderController;

impl RenderController {
    #[must_use]
    pub(crate) fn accepts(
        application: &Application,
        document_id: DocumentId,
        event_generation: Option<u64>,
        current_generation: u64,
    ) -> bool {
        application
            .ready_document()
            .is_some_and(|document| document.id() == document_id)
            && event_generation.is_none_or(|generation| generation == current_generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{DocumentController, OpenTransition};
    use std::path::PathBuf;
    use std::time::Instant;

    #[test]
    fn rejects_stale_document_and_generation() {
        let mut application = Application::default();
        let id = DocumentId::new(7);
        DocumentController::begin_open(
            &mut application,
            id,
            PathBuf::from("fixture.pdf"),
            Instant::now(),
        );
        assert!(matches!(
            DocumentController::opened(&mut application, id, 2, 10_000),
            OpenTransition::Ready(_)
        ));

        assert!(RenderController::accepts(&application, id, Some(4), 4));
        assert!(!RenderController::accepts(&application, id, Some(3), 4));
        assert!(!RenderController::accepts(
            &application,
            DocumentId::new(8),
            None,
            4
        ));
    }

    #[test]
    fn event_after_active_tab_close_is_rejected() {
        let mut application = Application::default();
        let id = DocumentId::new(1);
        DocumentController::begin_open(
            &mut application,
            id,
            PathBuf::from("fixture.pdf"),
            Instant::now(),
        );
        assert!(matches!(
            DocumentController::opened(&mut application, id, 1, 10_000),
            OpenTransition::Ready(_)
        ));
        let active_tab = application.tabs.active_id();
        assert!(active_tab.is_some_and(|tab| application.tabs.close(tab)));

        assert!(!RenderController::accepts(&application, id, Some(1), 1));
    }
}
