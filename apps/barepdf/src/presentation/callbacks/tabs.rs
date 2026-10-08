use crate::application::DocumentController;
use barepdf_core::{DocumentId, MAX_OPEN_TABS};
use barepdf_render::{RenderCommand, RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::Path;
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
        let (path, already_ready) = {
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
            let already_ready = matches!(
                app.application
                    .tabs
                    .active()
                    .and_then(|t| t.document.as_ref()),
                Some(crate::application::DocumentState::Ready(_))
            );
            (
                app.application.tabs.path(id).map(Path::to_path_buf),
                already_ready,
            )
        };
        if already_ready {
            let mut app = state_activate.borrow_mut();
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
            refresh_thumbnail_model(&mut app, &window);
            refresh_page_model(&mut app, &window);
            refresh_tab_model(&app, &window);
            refresh_generation_bound_views(&mut app, &scheduler_activate, &window);
            refresh_bookmark_model(&app, &window);
            if !app.outline.is_empty() {
                refresh_outline_model(&mut app, &window);
            }
            let current_page = app.current_page;
            let active_doc = app.active_document();
            if let Some(image) = active_doc.and_then(|document| {
                app.page_images
                    .get(document, current_page, RenderKind::Page)
            }) {
                window.set_page_bitmap(image);
            }
            request_next_dimensions_batch(&mut app, &scheduler_activate);
        } else if let Some(path) = path {
            if path.is_file() {
                begin_open(path, None, &state_activate, &scheduler_activate, &window);
            } else {
                let mut app = state_activate.borrow_mut();
                if let Some(document) = app.active_document() {
                    app.page_images.remove_document(document);
                    app.thumbnail_images.remove_document(document);
                    app.text_geometries.remove_document(document);
                }
                DocumentController::fail_active_path(&mut app.application, path);
                reset_empty_document(&mut app, &window);
                show_banner(&window, "This PDF no longer exists.", true);
            }
        } else {
            reset_empty_document(&mut state_activate.borrow_mut(), &window);
        }
        refresh_tab_model(&state_activate.borrow(), &window);
    });

    let weak = window.as_weak();
    let state_close = state.clone();
    let scheduler_close = scheduler.clone();
    window.on_request_close_tab(move |raw_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_close.borrow_mut();
            let Some(id) = app.application.tabs.find_slint_id(raw_id) else {
                return;
            };
            let was_active = app.application.tabs.active_id() == Some(id);
            if was_active {
                snapshot_active_view(&mut app, &window);
                let previous_document = app.active_document();
                app.generation = scheduler_close.bump_generation();
                close_worker_document(&mut app, &scheduler_close, previous_document);
                clear_document_transients(&mut app, &window);
            }
            if !app.application.tabs.close(id) {
                return;
            }
            if was_active {
                restore_active_view(&mut app, &window);
            }
            app.application
                .tabs
                .active()
                .and_then(|tab| tab.path.clone())
                .filter(|_| was_active)
        };
        if let Some(path) = path {
            begin_open(path, None, &state_close, &scheduler_close, &window);
        } else if state_close.borrow().application.tabs.active_id().is_none() {
            reset_empty_document(&mut state_close.borrow_mut(), &window);
        }
        refresh_tab_model(&state_close.borrow(), &window);
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
