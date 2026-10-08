#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unchecked_time_subtraction,
    unused_must_use
)]

mod drawing;
mod features;
mod navigation;
mod print;
mod tabs;
mod tools;

#[cfg(test)]
mod tests;

pub(super) use self::drawing::handle_background_ui_event;
pub(super) use self::print::{handle_print_event, requeue_print_preview_for_generation};
pub(super) use self::tabs::{clear_document_transients, restore_active_view, snapshot_active_view};
pub(super) use self::tools::{
    handle_tool_event, refresh_merge_files, selected_tool_pages, set_tool_source,
};

use crate::application::{DocumentController, PrintController};
use crate::diagnostics::{self, DiagnosticEvent};
use crate::infrastructure::{UpdateCheckCanceller, UpdateCommand};
use barepdf_core::{SecretPassword, MAX_PASSWORD_BYTES};
use barepdf_i18n::{Language, ResolvedLanguage};
use barepdf_platform_windows::{
    is_installed_build, open_url, WindowsClipboard, WindowsFileDialogs,
};
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use super::models::refresh_thumbnail_model;
use super::state::AppState;
use super::ui::{
    apply_theme, begin_open, parse_drop_paths, persist_preferences, show_banner, theme_from_index,
    update_ui_strings, view_mode_label,
};
use super::update_ui::{queue_update_check, render_update_ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolKind {
    Merge,
    SingleSource,
}

impl ToolKind {
    #[must_use]
    pub(crate) fn from_slint_id(id: i32) -> Self {
        match id {
            0 => Self::Merge,
            _ => Self::SingleSource,
        }
    }
}

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
    let dialogs_open = dialogs.clone();
    window.on_request_open_file(move || {
        if let (Some(path), Some(window)) = (dialogs_open.pick_file(), weak.upgrade()) {
            begin_open(path, None, &state_open, &scheduler_open, &window);
        }
    });

    navigation::connect_navigation_callbacks(window, state, scheduler);
    navigation::connect_zoom_callbacks(window, state, scheduler);
    navigation::connect_view_callbacks(window, state, scheduler);
    navigation::connect_selection_callbacks(window, state, scheduler, clipboard);
    tabs::connect_tab_callbacks(window, state, scheduler);
    print::connect_print_callbacks(window, state, scheduler, print_controller);
    tools::connect_tools_callbacks(window, state, scheduler, dialogs.clone());
    features::connect_niche_feature_callbacks(window, state, scheduler, preferences_path);
    drawing::connect_annotation_and_signature_callbacks(window, state, scheduler, dialogs);
    super::window_chrome::connect_window_chrome_callbacks(window);

    let weak = window.as_weak();
    let state_password = state.clone();
    let scheduler_password = scheduler.clone();
    window.on_request_unlock_password(move |password| {
        if let Some(window) = weak.upgrade() {
            clear_document_password_ui(&window);
        }
        let Ok(mut password) = consume_ui_password(password) else {
            let language = state_password.borrow().preferences.language.resolve();
            if let Some(window) = weak.upgrade() {
                window.set_password_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "password.error.too_long",
                )));
                window.set_password_required(true);
            }
            return;
        };
        let path = DocumentController::pending_path(&state_password.borrow().application)
            .map(Path::to_path_buf);
        if let (Some(path), Some(window)) = (path, weak.upgrade()) {
            begin_open(
                path,
                Some(password),
                &state_password,
                &scheduler_password,
                &window,
            );
        } else {
            password.clear();
        }
    });

    let weak = window.as_weak();
    window.on_request_cancel_unlock_password(move || {
        if let Some(window) = weak.upgrade() {
            clear_document_password_ui(&window);
            window.set_password_error(SharedString::default());
            window.set_password_required(false);
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

pub(super) fn window_language(window: &AppWindow) -> ResolvedLanguage {
    match window.get_current_language() {
        1 => Language::English.resolve(),
        2 => Language::Turkish.resolve(),
        _ => Language::System.resolve(),
    }
}

pub(super) fn consume_ui_password(raw: SharedString) -> Result<SecretPassword, ()> {
    let mut password = SecretPassword::new(raw.to_string());
    if password.expose().len() > MAX_PASSWORD_BYTES {
        password.clear();
        return Err(());
    }
    Ok(password)
}

pub(super) fn clear_document_password_ui(window: &AppWindow) {
    window.set_document_password_input(SharedString::default());
}

pub(super) fn clear_tool_password_ui(window: &AppWindow) {
    window.set_tool_password_input(SharedString::default());
}
