#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unchecked_time_subtraction,
    unused_must_use
)]

use crate::application::{DocumentController, PrintController, PrintControllerError};
use crate::diagnostics::{self, DiagnosticEvent};
use crate::infrastructure::{PrintEvent, UpdateCheckCanceller, UpdateCommand};

use barepdf_core::{
    selection::SelectionEngine, DocumentId, PageIndex, TextPosition, TextSelection, ViewingMode,
    WindowMode, ZoomMode, MAX_OPEN_TABS, MAX_PASSWORD_BYTES,
};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_platform::printing::PrinterDialog;
use barepdf_platform::{ClipboardAccess, FileDialogs};
use barepdf_platform_windows::{
    is_installed_build, open_url, WindowsClipboard, WindowsFileDialogs, WindowsPrinterDialog,
};
use barepdf_render::{RenderCommand, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::models::{refresh_page_model, refresh_tab_model, refresh_thumbnail_model};
use super::state::AppState;
use super::ui::{
    apply_theme, begin_open, invalidate_layout_and_render, native_window_handle, navigate_to_page,
    navigate_to_page_inner, parse_drop_paths, persist_preferences, pointer_to_pdf,
    refresh_outline_model, render_visible_pages, request_visible_thumbnails, save_zoom_preference,
    send_render_command, show_banner, sync_effective_zoom, theme_from_index, update_ui_strings,
    validated_page_input, view_mode_index, view_mode_label,
};
use super::update_ui::{queue_update_check, render_update_ui};
pub(super) fn wire_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
    clipboard: Arc<WindowsClipboard>,
    preferences_path: &Path,
    background: (
        std::sync::mpsc::Sender<UpdateCommand>,
        UpdateCheckCanceller,
        Option<Rc<RefCell<PrintController>>>,
    ),
) {
    let (update_sender, update_check_canceller, print_controller) = background;
    let weak = window.as_weak();
    let state_open = state.clone();
    let scheduler_open = scheduler.clone();
    window.on_request_open_file(move || {
        if let (Some(path), Some(window)) = (dialogs.pick_file(), weak.upgrade()) {
            begin_open(path, None, &state_open, &scheduler_open, &window);
        }
    });

    connect_navigation_callbacks(window, state, scheduler);
    connect_zoom_callbacks(window, state, scheduler);
    connect_view_callbacks(window, state, scheduler, preferences_path);
    connect_selection_callbacks(window, state, scheduler, clipboard);
    connect_tab_callbacks(window, state, scheduler);
    connect_print_callbacks(window, state, print_controller);

    let weak = window.as_weak();
    let state_password = state.clone();
    let scheduler_password = scheduler.clone();
    window.on_request_unlock_password(move |password| {
        if password.as_str().len() > MAX_PASSWORD_BYTES {
            let language = state_password.borrow().preferences.language.resolve();
            if let Some(window) = weak.upgrade() {
                window.set_password_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "password.error.too_long",
                )));
                window.set_password_required(true);
            }
            return;
        }
        let path = DocumentController::pending_path(&state_password.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            let password = password.to_string();
            begin_open(
                path,
                Some(password),
                &state_password,
                &scheduler_password,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_language = state.clone();
    let preferences_path_language = preferences_path.to_path_buf();
    window.on_request_change_language(move |index| {
        let language = match index {
            1 => Language::English,
            2 => Language::Turkish,
            _ => Language::System,
        };
        let mut app = state_language.borrow_mut();
        app.preferences.language = language;
        let window = weak.upgrade();
        persist_preferences(
            &app.preferences,
            &preferences_path_language,
            window.as_ref(),
        );
        if let Some(window) = window {
            window.set_current_language(index);
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                language.resolve(),
            )));
            update_ui_strings(&window, language.resolve());
            refresh_thumbnail_model(&mut app, &window);
            render_update_ui(&window, &app);
        }
    });

    let weak = window.as_weak();
    let state_theme = state.clone();
    let preferences_path_theme = preferences_path.to_path_buf();
    window.on_request_change_theme(move |index| {
        let theme = theme_from_index(index);
        let mut app = state_theme.borrow_mut();
        app.preferences.theme = theme;
        let window = weak.upgrade();
        persist_preferences(&app.preferences, &preferences_path_theme, window.as_ref());
        if let Some(window) = window {
            apply_theme(&window, theme);
        }
    });

    let weak = window.as_weak();
    let state_update_consent = state.clone();
    let update_check_canceller_consent = update_check_canceller.clone();
    let preferences_path_updates = preferences_path.to_path_buf();
    window.on_request_change_update_checks(move |enabled| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if !enabled {
            update_check_canceller_consent.cancel_pending_check();
        }
        {
            let mut app = state_update_consent.borrow_mut();
            app.preferences.update_checks_enabled = Some(enabled);
            persist_preferences(&app.preferences, &preferences_path_updates, Some(&window));
        }
        window.set_update_checks_enabled(enabled);
    });

    let weak = window.as_weak();
    let state_update_check = state.clone();
    let preferences_path_check = preferences_path.to_path_buf();
    let update_sender_check = update_sender.clone();
    let update_check_canceller_check = update_check_canceller.clone();
    window.on_request_check_update(move || {
        if let Some(window) = weak.upgrade() {
            queue_update_check(
                &update_sender_check,
                &update_check_canceller_check,
                &state_update_check,
                &window,
                &preferences_path_check,
            );
        }
    });

    let weak = window.as_weak();
    let state_update_action = state.clone();
    window.on_request_update_action(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_update_action.borrow_mut();
        if app.update.is_busy() {
            return;
        }
        if let Some((path, update)) = app.update.begin_install() {
            app.wake_pump();
            render_update_ui(&window, &app);
            if update_sender
                .send(UpdateCommand::Install { path, update })
                .is_err()
            {
                app.update.mark_failed();
                render_update_ui(&window, &app);
            }
            return;
        }
        if !is_installed_build() {
            let Some(release_url) = app.update.release_url().map(str::to_owned) else {
                return;
            };
            if let Err(error) = open_url(&release_url) {
                diagnostics::warn_redacted(DiagnosticEvent::ReleasePageOpen, &error);
                app.update.mark_failed();
                render_update_ui(&window, &app);
            }
            return;
        }
        let Some(update) = app.update.begin_download() else {
            return;
        };
        app.wake_pump();
        render_update_ui(&window, &app);
        if update_sender.send(UpdateCommand::Download(update)).is_err() {
            app.update.mark_failed();
            render_update_ui(&window, &app);
        }
    });

    let weak = window.as_weak();
    let state_recent = state.clone();
    let scheduler_recent = scheduler.clone();
    window.on_request_open_recent(move |path| {
        if let Some(window) = weak.upgrade() {
            begin_open(
                PathBuf::from(path.as_str()),
                None,
                &state_recent,
                &scheduler_recent,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_drop = state.clone();
    let scheduler_drop = scheduler.clone();
    window.on_request_drop(move |transfer| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        match transfer
            .plain_text()
            .ok()
            .and_then(|text| parse_drop_paths(text.as_str()))
        {
            Some(Ok(path)) => begin_open(path, None, &state_drop, &scheduler_drop, &window),
            Some(Err(message)) => show_banner(&window, message, false),
            None => show_banner(&window, "The dropped item is not a PDF file.", false),
        }
    });

    let weak = window.as_weak();
    window.on_request_dismiss_banner(move || {
        if let Some(window) = weak.upgrade() {
            window.set_banner_visible(false);
            window.set_banner_update_action(false);
            window.set_banner_action_label(SharedString::default());
            window.set_banner_action_enabled(false);
        }
    });

    let weak = window.as_weak();
    let state_retry = state.clone();
    let scheduler_retry = scheduler.clone();
    window.on_request_retry(move || {
        let path = DocumentController::failed_path(&state_retry.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(path, None, &state_retry, &scheduler_retry, &window);
        }
    });
}

fn connect_print_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    controller: Option<Rc<RefCell<PrintController>>>,
) {
    let weak = window.as_weak();
    let state_print = state.clone();
    let controller_print = controller.clone();
    window.on_request_print(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let language = window_language(&window);
        let Some(controller) = controller_print.as_ref() else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.unavailable"),
                false,
            );
            return;
        };
        let target = state_print
            .borrow()
            .application
            .ready_document()
            .map(|document| {
                let path = document.path().to_path_buf();
                let title = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(barepdf_i18n::t(language, "print.default_document"))
                    .to_string();
                (path, title, document.page_count())
            });
        let Some((path, title, page_count)) = target else {
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.open_document"),
                false,
            );
            return;
        };
        let job_id = match controller.borrow_mut().reserve_job() {
            Ok(job_id) => job_id,
            Err(PrintControllerError::Busy) => {
                show_banner(&window, barepdf_i18n::t(language, "print.busy"), false);
                return;
            }
            Err(_) => {
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.start_failed"),
                    false,
                );
                return;
            }
        };
        let Some(hwnd) = native_window_handle(&window) else {
            controller.borrow_mut().release_reservation(job_id);
            show_banner(
                &window,
                barepdf_i18n::t(language, "print.dialog_unavailable"),
                false,
            );
            return;
        };
        let selection = WindowsPrinterDialog::new(hwnd as _).select(job_id, page_count);
        let selection = match selection {
            Ok(Some(selection)) => selection,
            Ok(None) => {
                controller.borrow_mut().release_reservation(job_id);
                return;
            }
            Err(_) => {
                controller.borrow_mut().release_reservation(job_id);
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.dialog_failed"),
                    false,
                );
                return;
            }
        };
        match controller.borrow_mut().submit(
            job_id,
            path,
            title,
            selection.range,
            selection.copies,
            Box::new(selection.sink),
        ) {
            Ok(()) => {
                window.set_print_active(true);
                window.set_print_progress(0.0);
                window.set_print_status(SharedString::from(barepdf_i18n::t(
                    language,
                    "print.status.preparing",
                )));
                state_print.borrow_mut().wake_pump();
            }
            Err(_) => {
                show_banner(
                    &window,
                    barepdf_i18n::t(language, "print.queue_failed"),
                    false,
                );
            }
        }
    });

    let weak = window.as_weak();
    let state_cancel = state.clone();
    window.on_request_cancel_print(move || {
        let Some(controller) = controller.as_ref() else {
            return;
        };
        if controller.borrow().cancel() {
            if let Some(window) = weak.upgrade() {
                window.set_print_status(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "print.status.cancelling",
                )));
                state_cancel.borrow_mut().wake_pump();
            }
        }
    });
}

pub(super) fn handle_print_event(event: PrintEvent, window: &AppWindow) {
    match event {
        PrintEvent::Progress {
            completed, total, ..
        } => {
            let progress = if total == 0 {
                0.0
            } else {
                completed as f32 / total as f32
            };
            window.set_print_progress(progress);
            window.set_print_status(SharedString::from(format!(
                "{} {completed} / {total}…",
                barepdf_i18n::t(window_language(window), "print.status.progress")
            )));
        }
        PrintEvent::Finished { .. } => {
            window.set_print_active(false);
            window.set_print_progress(1.0);
            window.set_print_status(SharedString::from(barepdf_i18n::t(
                window_language(window),
                "print.status.complete",
            )));
        }
        PrintEvent::Cancelled { .. } => {
            window.set_print_active(false);
            window.set_print_progress(0.0);
            window.set_print_status(SharedString::from(barepdf_i18n::t(
                window_language(window),
                "print.status.cancelled",
            )));
        }
        PrintEvent::Failed { message, .. } => {
            window.set_print_active(false);
            window.set_print_progress(0.0);
            drop(message);
            let message = barepdf_i18n::t(window_language(window), "print.status.failed");
            window.set_print_status(SharedString::from(message));
            show_banner(window, message, false);
        }
    }
}

fn window_language(window: &AppWindow) -> ResolvedLanguage {
    match window.get_current_language() {
        1 => Language::English.resolve(),
        2 => Language::Turkish.resolve(),
        _ => Language::System.resolve(),
    }
}

fn connect_tab_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_activate = state.clone();
    let scheduler_activate = scheduler.clone();
    window.on_request_activate_tab(move |raw_id| {
        let Ok(raw_id) = u64::try_from(raw_id) else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_activate.borrow_mut();
            let Some(id) = app.application.tabs.find_id(raw_id) else {
                return;
            };
            if app.application.tabs.active_id() == Some(id) {
                return;
            }
            snapshot_active_view(&mut app, &window);
            let previous_document = app.active_document();
            app.generation = scheduler_activate.bump_generation();
            close_worker_document(&mut app, &scheduler_activate, previous_document);
            clear_document_transients(&mut app, &window);
            if !app.application.tabs.activate(id) {
                return;
            }
            restore_active_view(&mut app, &window);
            app.application.tabs.path(id).map(Path::to_path_buf)
        };
        if let Some(path) = path {
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
        let Ok(raw_id) = u64::try_from(raw_id) else {
            return;
        };
        let Some(window) = weak.upgrade() else {
            return;
        };
        let path = {
            let mut app = state_close.borrow_mut();
            let Some(id) = app.application.tabs.find_id(raw_id) else {
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
        let previous_document = app.active_document();
        app.generation = scheduler_new.bump_generation();
        close_worker_document(&mut app, &scheduler_new, previous_document);
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

pub(super) fn snapshot_active_view(app: &mut AppState, window: &AppWindow) {
    let view = crate::application::ViewState {
        current_page: PageIndex::from_raw(app.current_page),
        zoom_mode: app.zoom_mode,
        zoom_factor: app.zoom_factor,
        rotation: app.rotation,
        scroll_y: window.get_current_scroll_y(),
        sidebar_visible: window.get_sidebar_visible(),
        sidebar_tab: window.get_sidebar_tab(),
    };
    if let Some(tab) = app.application.tabs.active_mut() {
        tab.view = view;
    }
}

pub(super) fn restore_active_view(app: &mut AppState, window: &AppWindow) {
    let Some(view) = app.application.tabs.active().map(|tab| tab.view.clone()) else {
        return;
    };
    app.current_page = view.current_page.get();
    app.zoom_mode = view.zoom_mode;
    app.zoom_factor = view.zoom_factor;
    app.rotation = view.rotation;
    app.last_scroll_y = view.scroll_y;
    window.set_current_scroll_y(view.scroll_y);
    window.set_sidebar_visible(view.sidebar_visible);
    window.set_sidebar_tab(view.sidebar_tab);
}

fn reset_empty_document(app: &mut AppState, window: &AppWindow) {
    app.current_page = 0;
    app.visible_page_indices.clear();
    app.page_dimensions.clear();
    app.selection = None;
    window.set_has_document(false);
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

pub(super) fn clear_document_transients(app: &mut AppState, window: &AppWindow) {
    app.selection = None;
    app.is_selecting = false;
    app.outline.clear();
    app.outline_requested = false;
    app.expanded_outline.clear();
    app.flat_outline.clear();
    app.visible_page_indices.clear();
    window.set_has_selection(false);
    window.set_password_required(false);
    window.set_password_error(SharedString::default());
    window.set_outline_items(ModelRc::new(VecModel::default()));
    window.set_visible_pages(ModelRc::new(VecModel::default()));
}

fn connect_navigation_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let connect = |register: fn(&AppWindow, Box<dyn Fn()>), target: NavigationTarget| {
        let weak = window.as_weak();
        let state = state.clone();
        let scheduler = scheduler.clone();
        register(
            window,
            Box::new(move || {
                if let Some(window) = weak.upgrade() {
                    let page = {
                        let app = state.borrow();
                        match target {
                            NavigationTarget::Previous => app.current_page.saturating_sub(1),
                            NavigationTarget::Next => {
                                (app.current_page + 1).min(app.page_count().saturating_sub(1))
                            }
                            NavigationTarget::First => 0,
                            NavigationTarget::Last => app.page_count().saturating_sub(1),
                        }
                    };
                    navigate_to_page(page, &state, &scheduler, &window);
                }
            }),
        );
    };
    connect(
        |w, cb| w.on_request_prev_page(cb),
        NavigationTarget::Previous,
    );
    connect(|w, cb| w.on_request_next_page(cb), NavigationTarget::Next);
    connect(|w, cb| w.on_request_first_page(cb), NavigationTarget::First);
    connect(|w, cb| w.on_request_last_page(cb), NavigationTarget::Last);

    let weak = window.as_weak();
    let state_select = state.clone();
    let scheduler_select = scheduler.clone();
    window.on_request_select_page(move |page| {
        if page >= 0 {
            if let Some(window) = weak.upgrade() {
                navigate_to_page(page as u32, &state_select, &scheduler_select, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_entry = state.clone();
    let scheduler_entry = scheduler.clone();
    window.on_request_go_to_page(move |text| {
        let count = state_entry.borrow().page_count();
        if let Some(page) = validated_page_input(text.as_str(), count) {
            if let Some(window) = weak.upgrade() {
                navigate_to_page(page, &state_entry, &scheduler_entry, &window);
            }
        } else if let Some(window) = weak.upgrade() {
            let current = state_entry.borrow().current_page + 1;
            window.set_current_page_str(SharedString::from(current.to_string()));
        }
    });
}

#[derive(Clone, Copy)]
enum NavigationTarget {
    Previous,
    Next,
    First,
    Last,
}

fn connect_zoom_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let weak = window.as_weak();
    let state_in = state.clone();
    let scheduler_in = scheduler.clone();
    window.on_request_zoom_in(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_in.borrow_mut();
            sync_effective_zoom(&mut app);
            app.zoom_factor = app.zoom_factor.zoom_in();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_in, &window, true);
        }
    });

    let weak = window.as_weak();
    let state_out = state.clone();
    let scheduler_out = scheduler.clone();
    window.on_request_zoom_out(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_out.borrow_mut();
            sync_effective_zoom(&mut app);
            app.zoom_factor = app.zoom_factor.zoom_out();
            app.zoom_mode = ZoomMode::Custom(app.zoom_factor);
            save_zoom_preference(&mut app);
            invalidate_layout_and_render(&mut app, &scheduler_out, &window, true);
        }
    });

    connect_zoom_mode(window, state, scheduler, ZoomMode::FitWidth, |w, cb| {
        w.on_request_fit_width(cb)
    });
    connect_zoom_mode(window, state, scheduler, ZoomMode::FitPage, |w, cb| {
        w.on_request_fit_page(cb)
    });
    connect_zoom_mode(window, state, scheduler, ZoomMode::ActualSize, |w, cb| {
        w.on_request_actual_size(cb)
    });
}

fn connect_zoom_mode<F>(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    mode: ZoomMode,
    register: F,
) where
    F: FnOnce(&AppWindow, Box<dyn Fn()>),
{
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    register(
        window,
        Box::new(move || {
            if let Some(window) = weak.upgrade() {
                let mut app = state.borrow_mut();
                app.zoom_mode = mode;
                save_zoom_preference(&mut app);
                invalidate_layout_and_render(&mut app, &scheduler, &window, true);
            }
        }),
    );
}

fn connect_view_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    _preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_view = state.clone();
    let scheduler_view = scheduler.clone();
    window.on_request_toggle_view_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_view.borrow_mut();
            app.viewing_mode = match app.viewing_mode {
                ViewingMode::ContinuousVertical => ViewingMode::SinglePage,
                _ => ViewingMode::ContinuousVertical,
            };
            app.preferences.viewing_mode = app.viewing_mode;
            window.set_view_mode(view_mode_index(app.viewing_mode));
            window.set_view_mode_label(SharedString::from(view_mode_label(
                app.viewing_mode,
                app.preferences.language.resolve(),
            )));
            navigate_to_page_inner(app.current_page, &mut app, &scheduler_view, &window);
        }
    });

    let weak = window.as_weak();
    let state_sidebar = state.clone();
    let scheduler_sidebar = scheduler.clone();
    window.on_request_toggle_sidebar(move || {
        if let Some(window) = weak.upgrade() {
            let visible = !window.get_sidebar_visible();
            window.set_sidebar_visible(visible);
            let mut app = state_sidebar.borrow_mut();
            app.preferences.sidebar_visible = visible;
            app.layout_key = None;
            if visible {
                request_visible_thumbnails(&mut app, &scheduler_sidebar, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_tab = state.clone();
    let scheduler_tab = scheduler.clone();
    window.on_request_sidebar_tab(move |tab| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_tab.borrow_mut();
            if tab == 1 && !app.outline_requested {
                if let Some(document_id) = app.active_document() {
                    app.outline_requested = send_render_command(
                        &mut app,
                        &scheduler_tab,
                        RenderCommand::FetchOutline { document_id },
                    );
                }
            } else if tab == 0 {
                request_visible_thumbnails(&mut app, &scheduler_tab, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_outline = state.clone();
    let scheduler_outline = scheduler.clone();
    window.on_request_toggle_outline(move |index| {
        let mut target = None;
        {
            let mut app = state_outline.borrow_mut();
            if let Some(entry) = app.flat_outline.get(index as usize).cloned() {
                if entry.has_children {
                    if !app.expanded_outline.remove(&entry.path) {
                        app.expanded_outline.insert(entry.path.clone());
                    }
                    if let Some(window) = weak.upgrade() {
                        refresh_outline_model(&mut app, &window);
                    }
                }
                target = entry.page_index;
            }
        }
        if let (Some(page), Some(window)) = (target, weak.upgrade()) {
            navigate_to_page(page, &state_outline, &scheduler_outline, &window);
        }
    });

    let weak = window.as_weak();
    let state_fullscreen = state.clone();
    window.on_request_toggle_fullscreen(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_fullscreen.borrow_mut();
            let enabled = app.window_mode != WindowMode::FullScreen;
            app.window_mode = if enabled {
                WindowMode::FullScreen
            } else {
                WindowMode::Normal
            };
            window.set_window_mode(if enabled { 1 } else { 0 });
            window.window().set_fullscreen(enabled);
        }
    });

    let weak = window.as_weak();
    let state_presentation = state.clone();
    let scheduler_presentation = scheduler.clone();
    window.on_request_presentation_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_presentation.borrow_mut();
            app.window_mode = WindowMode::Presentation;
            window.set_window_mode(2);
            window.window().set_fullscreen(true);
            app.generation = scheduler_presentation.bump_generation();
            render_visible_pages(&mut app, &scheduler_presentation, &window);
        }
    });

    let weak = window.as_weak();
    let state_exit = state.clone();
    window.on_request_exit_special_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_exit.borrow_mut();
            if app.window_mode != WindowMode::Normal {
                app.window_mode = WindowMode::Normal;
                window.set_window_mode(0);
                window.window().set_fullscreen(false);
            }
        }
    });
}

fn connect_selection_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    clipboard: Arc<WindowsClipboard>,
) {
    let state_copy = state.clone();
    window.on_request_copy(move || {
        let app = state_copy.borrow();
        if let (Some(selection), Some(document)) = (app.selection, app.active_document()) {
            let geometries = app.text_geometries.in_page_order(document);
            let text = SelectionEngine::get_selected_text_in_page_order(&selection, &geometries);
            if !text.is_empty() {
                if let Err(error) = clipboard.set_text(&text) {
                    diagnostics::warn_redacted(DiagnosticEvent::ClipboardWrite, &error);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_all = state.clone();
    window.on_request_select_all(move || {
        let mut app = state_all.borrow_mut();
        if app.page_count() == 0 {
            return;
        }
        let Some(document) = app.active_document() else {
            return;
        };
        let last_page = app.page_count() - 1;
        let last_character = app
            .text_geometries
            .get(document, last_page)
            .map(|geometry| geometry.glyphs.len() as u32)
            .unwrap_or(u32::MAX);
        app.selection = Some(TextSelection::new(
            TextPosition::new(PageIndex::zero(), 0),
            TextPosition::new(PageIndex::from_raw(last_page), last_character),
        ));
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(true);
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_down = state.clone();
    let scheduler_down = scheduler.clone();
    window.on_pointer_down(move |page, x, y, _| {
        if page < 0 {
            return;
        }
        let mut app = state_down.borrow_mut();
        let page = page as u32;
        let Some(page_index) = DocumentController::page_index(&app.application, page) else {
            return;
        };
        if let Some(document_id) = app.active_document() {
            if !app.text_geometries.contains_key(document_id, page) {
                let generation = app.generation;
                send_render_command(
                    &mut app,
                    &scheduler_down,
                    RenderCommand::FetchTextGeometry {
                        document_id,
                        generation,
                        page_index,
                    },
                );
            }
        }
        let (pdf_x, pdf_y) = pointer_to_pdf(&app, page, x, y);
        let now = Instant::now();
        app.click_count = if now.duration_since(app.last_click_time) < Duration::from_millis(400) {
            app.click_count + 1
        } else {
            1
        };
        app.last_click_time = now;
        let click_count = app.click_count;
        let geometry = app
            .active_document()
            .and_then(|document| app.text_geometries.get(document, page).cloned());
        if let Some(geometry) = geometry.as_ref() {
            let character = SelectionEngine::hit_test(geometry, pdf_x, pdf_y);
            app.selection = Some(match click_count {
                2 => SelectionEngine::select_word(geometry, page_index, character),
                count if count >= 3 => {
                    SelectionEngine::select_line(geometry, page_index, character)
                }
                _ => {
                    app.is_selecting = true;
                    let position = TextPosition::new(page_index, character);
                    TextSelection::new(position, position)
                }
            });
        }
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_move = state.clone();
    window.on_pointer_move(move |page, x, y| {
        if page < 0 {
            return;
        }
        let mut app = state_move.borrow_mut();
        if !app.is_selecting {
            return;
        }
        let page = page as u32;
        let Some(page_index) = DocumentController::page_index(&app.application, page) else {
            return;
        };
        let Some(document) = app.active_document() else {
            return;
        };
        let (pdf_x, pdf_y) = pointer_to_pdf(&app, page, x, y);
        let character = app
            .text_geometries
            .get(document, page)
            .map(|geometry| SelectionEngine::hit_test(geometry, pdf_x, pdf_y))
            .unwrap_or(0);
        if let Some(selection) = app.selection.as_mut() {
            selection.focus = TextPosition::new(page_index, character);
        }
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_up = state.clone();
    window.on_pointer_up(move |_, _, _| {
        let mut app = state_up.borrow_mut();
        app.is_selecting = false;
        if let Some(window) = weak.upgrade() {
            window.set_has_selection(app.selection.is_some_and(|selection| !selection.is_empty()));
        }
    });
}
