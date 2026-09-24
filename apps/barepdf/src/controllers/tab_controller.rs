use crate::presentation::state::AppState;
use barepdf_core::DocumentId;
use barepdf_render::RenderCommand;
use barepdf_render::RenderScheduler;
use std::path::PathBuf;

pub fn activate_tab(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    tab_raw_id: u64,
) -> Option<PathBuf> {
    let tab_id = app.application.tabs.find_id(tab_raw_id)?;
    if app.application.tabs.activate(tab_id) {
        app.generation = scheduler.bump_generation();
        if let Some(tab) = app.application.tabs.active() {
            return tab.path.clone();
        }
    }
    None
}

pub fn close_tab(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    tab_raw_id: u64,
) -> Option<DocumentId> {
    let tab_id = app.application.tabs.find_id(tab_raw_id)?;
    let document_id = app
        .application
        .tabs
        .tabs()
        .iter()
        .find(|t| t.id == tab_id)
        .and_then(|t| t.document.as_ref())
        .and_then(|d| match d {
            crate::application::DocumentState::Ready(doc) => Some(doc.id()),
            _ => None,
        });

    if app.application.tabs.close(tab_id) {
        if let Some(doc_id) = document_id {
            app.page_images.remove_document(doc_id);
            app.thumbnail_images.remove_document(doc_id);
            app.text_geometries.remove_document(doc_id);
            let _ = scheduler.send_command(RenderCommand::CloseDocument(doc_id));
        }
        app.generation = scheduler.bump_generation();
        return document_id;
    }
    None
}
