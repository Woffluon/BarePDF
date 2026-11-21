use crate::application::DocumentController;
use barepdf_core::{DocumentId, MAX_OPEN_TABS};
use barepdf_render::{RenderCommand, RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use super::super::models::{
    refresh_bookmark_model, refresh_page_model, refresh_tab_model, refresh_thumbnail_model,
};
use super::super::state::AppState;
use super::super::ui::{
    begin_open, ensure_layout, refresh_generation_bound_views, refresh_outline_model,
    request_next_dimensions_batch, send_render_command, show_banner, update_zoom_ui,
};
use super::clear_document_password_ui;
use super::print::close_print_preview;

pub(super) fn connect_tab_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_activate = state.clone();
    let scheduler_activate = scheduler.clone();
    window.on_request_activate_tab(move |raw_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let (path, tab_state) = {
            let mut app = state_activate.borrow_mut();
            let Some(id) = app.application.tabs.find_slint_id(raw_id) else {
                return;
            };
            if app.application.tabs.active_id() == Some(id) {
                return;
            }
            snapshot_active_view(&mut app, &window);
            app.generation = scheduler_activate.bump_generation();
            clear_document_transients(&mut app, &window);
            if !app.application.tabs.activate(id) {
                return;
            }
            restore_active_view(&mut app, &window);
            switched_tab_state(&app)
        };
        activate_switched_tab(
            path,
            tab_state,
            &state_activate,
            &scheduler_activate,
            &window,
        );
        refresh_tab_model(&state_activate.borrow(), &window);
    });

    let weak = window.as_weak();
    let state_close = state.clone();
    let scheduler_close = scheduler.clone();
    window.on_request_close_tab(move |raw_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let doc_id = {
            let app = state_close.borrow();
            let tab_id = app.application.tabs.find_slint_id(raw_id);
            tab_id.and_then(|tid| {
                app.application
                    .tabs
                    .tabs()
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| match &t.document {
                        Some(crate::application::DocumentState::Ready(doc)) => Some(doc.id),
                        Some(
                            crate::application::DocumentState::Opening { active, .. }
                            | crate::application::DocumentState::PasswordRequired { active, .. }
                            | crate::application::DocumentState::Failed { active, .. },
                        ) => active.as_ref().map(|doc| doc.id),
                        _ => None,
                    })
            })
        };
        let has_unsaved = if let Some(doc_id) = doc_id {
            state_close
                .borrow()
                .drawing
                .annotations
                .get(&doc_id)
                .is_some_and(|a| !a.is_empty())
        } else {
            false
        };
        if has_unsaved {
            if let Some(doc_id) = doc_id {
                let mut app = state_close.borrow_mut();
                app.pending_close_tab_id = Some(doc_id);
                window.set_unsaved_drawings_dialog_open(true);
                return;
            }
        }
        close_tab_internal(raw_id, &window, &state_close, &scheduler_close);
    });

    let weak = window.as_weak();
    let state_confirm = state.clone();
    let scheduler_confirm = scheduler.clone();
    window.on_confirm_close_tab(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let doc_id = state_confirm.borrow_mut().pending_close_tab_id.take();
        if let Some(doc_id) = doc_id {
            let mut app = state_confirm.borrow_mut();
            app.drawing.annotations.remove(&doc_id);
            let tab = app
                .application
                .tabs
                .tabs()
                .iter()
                .find(|t| match &t.document {
                    Some(crate::application::DocumentState::Ready(doc)) => doc.id == doc_id,
                    Some(
                        crate::application::DocumentState::Opening { active, .. }
                        | crate::application::DocumentState::PasswordRequired { active, .. }
                        | crate::application::DocumentState::Failed { active, .. },
                    ) => active.as_ref().is_some_and(|d| d.id == doc_id),
                    _ => false,
                });
            let raw_id = tab.map(|t| t.id.to_slint_id());
            drop(app);
            if let Some(raw_id) = raw_id {
                close_tab_internal(raw_id, &window, &state_confirm, &scheduler_confirm);
            } else {
                state_confirm.borrow_mut().clear_document_passwords(doc_id);
            }
        }
        window.set_unsaved_drawings_dialog_open(false);
    });

    let weak = window.as_weak();
    let state_cancel = state.clone();
    window.on_cancel_close_tab(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        state_cancel.borrow_mut().pending_close_tab_id = None;
        window.set_unsaved_drawings_dialog_open(false);
    });

    let weak = window.as_weak();
    let state_new = state.clone();
    let scheduler_new = scheduler.clone();
    window.on_request_new_tab(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_new.borrow_mut();
        snapshot_active_view(&mut app, &window);
        app.generation = scheduler_new.bump_generation();
        clear_document_transients(&mut app, &window);
        if app.application.tabs.new_empty().is_none() {
            show_banner(
                &window,
                format!("A maximum of {MAX_OPEN_TABS} tabs can be open."),
                false,
            );
            return;
        }
        restore_active_view(&mut app, &window);
        reset_empty_document(&mut app, &window);
        refresh_tab_model(&app, &window);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchedTabState {
    Ready,
    Opening,
    NeedsOpen,
}

fn switched_tab_state(app: &AppState) -> (Option<PathBuf>, SwitchedTabState) {
    let path = app
        .application
        .tabs
        .active_id()
        .and_then(|id| app.application.tabs.path(id))
        .map(Path::to_path_buf);
    let state = match app
        .application
        .tabs
        .active()
        .and_then(|t| t.document.as_ref())
    {
        Some(crate::application::DocumentState::Ready(_)) => SwitchedTabState::Ready,
        Some(crate::application::DocumentState::Opening { .. }) => SwitchedTabState::Opening,
        _ => SwitchedTabState::NeedsOpen,
    };
    (path, state)
}

fn push_unique_doc_id(ids: &mut Vec<DocumentId>, doc_id: DocumentId) {
    if !ids.contains(&doc_id) {
        ids.push(doc_id);
    }
}

fn collect_tab_document_ids(app: &AppState, raw_tab_id: u64) -> Vec<DocumentId> {
    let mut ids = vec![DocumentId::new(raw_tab_id)];
    if let Some(tab) = app
        .application
        .tabs
        .tabs()
        .iter()
        .find(|t| t.id.get() == raw_tab_id)
    {
        match &tab.document {
            Some(crate::application::DocumentState::Ready(doc)) => {
                push_unique_doc_id(&mut ids, doc.id);
            }
            Some(
                crate::application::DocumentState::Opening { id, active, .. }
                | crate::application::DocumentState::PasswordRequired { id, active, .. },
            ) => {
                push_unique_doc_id(&mut ids, *id);
                if let Some(doc) = active {
                    push_unique_doc_id(&mut ids, doc.id);
                }
            }
            Some(crate::application::DocumentState::Failed {
                active: Some(doc), ..
            }) => {
                push_unique_doc_id(&mut ids, doc.id);
            }
            Some(crate::application::DocumentState::Failed { active: None, .. }) | None => {}
        }
    }
    ids
}

fn restore_ready_tab_ui(
    path: Option<&Path>,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    window: &AppWindow,
) {
    let mut app = state.borrow_mut();
    if let Some(p) = path {
        app.start_file_watcher(p);
    } else {
        app.stop_file_watcher();
    }
    window.set_has_document(true);
    let title = app
        .application
        .tabs
        .active()
        .map(|tab| tab.title.clone())
        .unwrap_or_default();
    window.set_document_title(SharedString::from(title));
    let page_count = app.page_count();
    window.set_total_pages_str(SharedString::from(page_count.to_string()));
    app.layout_key = None;
    ensure_layout(&mut app);
    refresh_thumbnail_model(&mut app, window);
    refresh_page_model(&mut app, window);
    refresh_tab_model(&app, window);
    refresh_generation_bound_views(&mut app, scheduler, window);
    refresh_bookmark_model(&app, window);
    if !app.outline.is_empty() {
        refresh_outline_model(&mut app, window);
    }
    let current_page = app.current_page;
    let active_doc = app.active_document();
    if let Some(image) = active_doc.and_then(|document| {
        app.page_images
            .get(document, current_page, RenderKind::Page)
    }) {
        window.set_page_bitmap(image);
    }
    request_next_dimensions_batch(&mut app, scheduler);
}

fn activate_switched_tab(
    path: Option<PathBuf>,
    tab_state: SwitchedTabState,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    window: &AppWindow,
) {
    match tab_state {
        SwitchedTabState::Ready => {
            restore_ready_tab_ui(path.as_deref(), state, scheduler, window);
        }
        SwitchedTabState::Opening => {
            let has_ready = state.borrow().active_document().is_some();
            if has_ready {
                restore_ready_tab_ui(path.as_deref(), state, scheduler, window);
            } else {
                reset_empty_document(&mut state.borrow_mut(), window);
            }
            window.set_status_text(SharedString::from("Opening document…"));
        }
        SwitchedTabState::NeedsOpen => {
            if let Some(path) = path {
                if path.is_file() {
                    begin_open(path, None, state, scheduler, window);
                } else {
                    let mut app = state.borrow_mut();
                    if let Some(document) = app.active_document() {
                        app.page_images.remove_document(document);
                        app.thumbnail_images.remove_document(document);
                        app.text_geometries.remove_document(document);
                        app.clear_document_passwords(document);
                    }
                    if let Some(tab_id) = app.application.tabs.active_id() {
                        for doc_id in collect_tab_document_ids(&app, tab_id.get()) {
                            app.clear_document_passwords(doc_id);
                        }
                    }
                    DocumentController::fail_active_path(&mut app.application, path);
                    reset_empty_document(&mut app, window);
                    show_banner(window, "This PDF no longer exists.", true);
                }
            } else {
                reset_empty_document(&mut state.borrow_mut(), window);
            }
        }
    }
}

fn close_tab_internal(
    raw_id: i32,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let next_tab = {
        let mut app = state.borrow_mut();
        let Some(id) = app.application.tabs.find_slint_id(raw_id) else {
            return;
        };
        let closing_doc_ids = collect_tab_document_ids(&app, id.get());
        let was_active = app.application.tabs.active_id() == Some(id);
        if was_active {
            snapshot_active_view(&mut app, window);
            let previous_document = app.active_document();
            app.generation = scheduler.bump_generation();
            close_worker_document(&mut app, scheduler, previous_document);
            clear_document_transients(&mut app, window);
        }
        if !app.application.tabs.close(id) {
            return;
        }
        for doc_id in closing_doc_ids {
            app.clear_document_passwords(doc_id);
        }
        if was_active {
            restore_active_view(&mut app, window);
            Some(switched_tab_state(&app))
        } else {
            None
        }
    };
    if let Some((path, tab_state)) = next_tab {
        activate_switched_tab(path, tab_state, state, scheduler, window);
    }
    refresh_tab_model(&state.borrow(), window);
}

pub(crate) fn snapshot_active_view(app: &mut AppState, window: &AppWindow) {
    app.capture_active_tab_view(
        window.get_current_scroll_y(),
        window.get_sidebar_visible(),
        window.get_sidebar_tab(),
    );
}

pub(crate) fn restore_active_view(app: &mut AppState, window: &AppWindow) {
    let Some(view) = app.apply_active_tab_view() else {
        return;
    };
    window.set_current_scroll_y(view.scroll_y);
    window.set_sidebar_visible(view.sidebar_visible);
    window.set_sidebar_tab(view.sidebar_tab);
    update_zoom_ui(window, app.zoom_mode, app.zoom_factor);
}

pub(super) fn reset_empty_document(app: &mut AppState, window: &AppWindow) {
    app.stop_file_watcher();
    app.current_page = 0;
    app.visible_page_indices.clear();
    app.page_dimensions = Arc::new(Vec::new());
    app.selection = None;
    window.set_has_document(false);
    clear_document_password_ui(window);
    window.set_password_required(false);
    window.set_has_selection(false);
    window.set_document_title(SharedString::default());
    window.set_total_pages_str(SharedString::from("0"));
    window.set_page_bitmap(Image::default());
    window.set_visible_pages(ModelRc::new(VecModel::default()));
    window.set_thumbnail_items(ModelRc::new(VecModel::default()));
}

pub(super) fn close_worker_document(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    document: Option<DocumentId>,
) {
    if let Some(document) = document {
        send_render_command(app, scheduler, RenderCommand::CloseDocument(document));
    }
}

pub(crate) fn clear_document_transients(app: &mut AppState, window: &AppWindow) {
    close_print_preview(app, window);
    app.selection = None;
    app.is_selecting = false;
    app.outline = Arc::new(Vec::new());
    app.in_flight.outline_requested = false;
    app.expanded_outline.clear();
    app.flat_outline.clear();
    app.visible_page_indices.clear();
    app.failed_pages.clear();
    window.set_has_selection(false);
    clear_document_password_ui(window);
    window.set_password_required(false);
    window.set_password_error(SharedString::default());
    window.set_outline_items(ModelRc::new(VecModel::default()));
    window.set_visible_pages(ModelRc::new(VecModel::default()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{DocumentState, OpenTab, ReadyDocument};
    use barepdf_core::{
        PageCount, PageIndex, PageTextGeometry, PdfError, RawBitmap, Rotation, SecretPassword,
        UserPreferences,
    };
    use barepdf_pdf::{OutlineNode, PdfBackend, PdfDocument, TextSpan};
    use std::time::Instant;

    struct MockPdfDoc;
    impl PdfDocument for MockPdfDoc {
        fn page_count(&self) -> Result<PageCount, PdfError> {
            Ok(PageCount::ONE)
        }
        fn page_dimensions(&self, _page_index: PageIndex) -> Result<(f32, f32), PdfError> {
            Ok((600.0, 800.0))
        }
        fn render_page(
            &self,
            _page_index: PageIndex,
            _target_width: u32,
            _target_height: u32,
            _rotation: Rotation,
        ) -> Result<RawBitmap, PdfError> {
            RawBitmap::new(1, 1, vec![0, 0, 0, 255])
                .map_err(|e| PdfError::InvalidPdfReason(e.to_string()))
        }
        fn extract_text(&self, _page_index: PageIndex) -> Result<String, PdfError> {
            Ok(String::new())
        }
        fn extract_text_spans(&self, _page_index: PageIndex) -> Result<Vec<TextSpan>, PdfError> {
            Ok(Vec::new())
        }
        fn get_page_text_geometry(
            &self,
            page_index: PageIndex,
        ) -> Result<PageTextGeometry, PdfError> {
            Ok(PageTextGeometry {
                page_index,
                glyphs: Vec::new(),
                links: Vec::new(),
            })
        }
        fn get_outline(&self) -> Result<Vec<OutlineNode>, PdfError> {
            Ok(Vec::new())
        }
    }

    struct MockBackend;
    impl PdfBackend for MockBackend {
        fn open_path(
            &self,
            _path: &Path,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(MockPdfDoc))
        }
        fn open_bytes(
            &self,
            _bytes: Vec<u8>,
            _password: Option<&str>,
        ) -> Result<Box<dyn PdfDocument>, PdfError> {
            Ok(Box::new(MockPdfDoc))
        }
    }

    #[test]
    fn tab_close_and_switch_clears_only_closed_document_passwords() {
        crate::presentation::test_support::run_on_ui_thread(|| {
            let state = Rc::new(RefCell::new(AppState::new(UserPreferences::default())));
            let window = AppWindow::new().expect("slint window");
            let scheduler = Rc::new(RenderScheduler::spawn(
                MockBackend,
                barepdf_core::MemoryBudget::new(1024 * 1024),
            ));
            connect_tab_callbacks(&window, &state, &scheduler);

            let OpenTab::Created(tab1) = state
                .borrow_mut()
                .application
                .tabs
                .open(PathBuf::from("enc1.pdf"), "enc1".into())
            else {
                panic!("expected tab1");
            };
            let doc1 = DocumentId::new(tab1.get());
            {
                let mut app = state.borrow_mut();
                if let Some(tab) = app.application.tabs.active_mut() {
                    tab.document = Some(DocumentState::Ready(ReadyDocument {
                        id: doc1,
                        path: PathBuf::from("enc1.pdf"),
                        page_count: PageCount::ONE,
                        started_at: Instant::now(),
                    }));
                }
                app.set_pending_document_password(doc1, Some(SecretPassword::new("pass-1".into())));
                app.finalize_opened_document_password(doc1);
            }

            let OpenTab::Created(tab2) = state
                .borrow_mut()
                .application
                .tabs
                .open(PathBuf::from("enc2.pdf"), "enc2".into())
            else {
                panic!("expected tab2");
            };
            let doc2 = DocumentId::new(tab2.get());
            {
                let mut app = state.borrow_mut();
                if let Some(tab) = app.application.tabs.active_mut() {
                    tab.document = Some(DocumentState::Ready(ReadyDocument {
                        id: doc2,
                        path: PathBuf::from("enc2.pdf"),
                        page_count: PageCount::ONE,
                        started_at: Instant::now(),
                    }));
                }
                app.set_pending_document_password(doc2, Some(SecretPassword::new("pass-2".into())));
                app.finalize_opened_document_password(doc2);
            }

            let OpenTab::Created(tab3) = state
                .borrow_mut()
                .application
                .tabs
                .open(PathBuf::from("pending3.pdf"), "pending3".into())
            else {
                panic!("expected tab3");
            };
            let doc3 = DocumentId::new(tab3.get());
            {
                let mut app = state.borrow_mut();
                if let Some(tab) = app.application.tabs.active_mut() {
                    tab.document = Some(DocumentState::PasswordRequired {
                        id: doc3,
                        path: PathBuf::from("pending3.pdf"),
                        started_at: Instant::now(),
                        active: None,
                    });
                }
                app.set_pending_document_password(
                    doc3,
                    Some(SecretPassword::new("pending-3".into())),
                );
            }

            // Switching to tab1 preserves passwords of all open tabs.
            window.invoke_request_activate_tab(tab1.to_slint_id());
            assert!(!window.get_password_required());
            assert_eq!(
                state
                    .borrow()
                    .document_password(doc1)
                    .map(SecretPassword::expose),
                Some("pass-1")
            );
            assert_eq!(
                state
                    .borrow()
                    .document_password(doc2)
                    .map(SecretPassword::expose),
                Some("pass-2")
            );
            assert_eq!(
                state
                    .borrow()
                    .pending_document_password(doc3)
                    .map(SecretPassword::expose),
                Some("pending-3")
            );

            // Closing background tab3 clears doc3's pending password while keeping doc1 and doc2.
            close_tab_internal(tab3.to_slint_id(), &window, &state, &scheduler);
            assert!(state.borrow().pending_document_password(doc3).is_none());
            assert!(state.borrow().document_password(doc3).is_none());
            assert_eq!(
                state
                    .borrow()
                    .document_password(doc1)
                    .map(SecretPassword::expose),
                Some("pass-1")
            );
            assert_eq!(
                state
                    .borrow()
                    .document_password(doc2)
                    .map(SecretPassword::expose),
                Some("pass-2")
            );

            // Activate tab2 then close it -> switches back to ready tab1 without losing doc1's password.
            window.invoke_request_activate_tab(tab2.to_slint_id());
            close_tab_internal(tab2.to_slint_id(), &window, &state, &scheduler);
            assert!(state.borrow().document_password(doc2).is_none());
            assert_eq!(
                state.borrow().application.tabs.active_id(),
                Some(tab1),
                "closing tab2 should activate tab1"
            );
            assert!(window.get_has_document());
            assert!(!window.get_password_required());
            assert_eq!(
                state
                    .borrow()
                    .document_password(doc1)
                    .map(SecretPassword::expose),
                Some("pass-1")
            );

            // Closing the last tab clears doc1's password too.
            close_tab_internal(tab1.to_slint_id(), &window, &state, &scheduler);
            assert!(state.borrow().document_password(doc1).is_none());
            assert!(!window.get_has_document());

            let _ = scheduler.shutdown();
        });
    }
}
