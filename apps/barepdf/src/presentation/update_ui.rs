use super::state::AppState;
use super::ui::persist_preferences;
use crate::application::UpdateUiState;
use crate::diagnostics::{self, DiagnosticEvent};
use crate::infrastructure::{
    UpdateCheckCanceller, UpdateCommand, UpdateEvent, AUTO_CHECK_INTERVAL_SECONDS,
};
use barepdf_core::UserPreferences;
use barepdf_platform_windows::is_installed_build;
use barepdf_ui::AppWindow;
use slint::SharedString;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn queue_update_check(
    sender: &std::sync::mpsc::Sender<UpdateCommand>,
    canceller: &UpdateCheckCanceller,
    state: &Rc<RefCell<AppState>>,
    window: &AppWindow,
    preferences_path: &Path,
) {
    let mut app = state.borrow_mut();
    if !app.update.begin_check() {
        return;
    }
    let cancellation = canceller.begin_check();
    app.wake_pump();
    render_update_ui(window, &app);
    if sender.send(UpdateCommand::Check(cancellation)).is_ok() {
        app.preferences.last_update_check_unix = Some(unix_timestamp());
        persist_preferences(&app.preferences, preferences_path, Some(window));
    } else {
        app.update.mark_failed();
        render_update_ui(window, &app);
    }
}

pub(super) fn handle_update_event(
    event: UpdateEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
) {
    let mut app = state.borrow_mut();
    match event {
        UpdateEvent::UpToDate => app.update.mark_current(),
        UpdateEvent::Available(update) => app.update.mark_available(update),
        UpdateEvent::Downloaded { path, update } => app.update.mark_downloaded(path, update),
        UpdateEvent::InstallerStarted => {
            let _ = slint::quit_event_loop();
        }
        UpdateEvent::Error(error) => {
            diagnostics::warn_redacted(DiagnosticEvent::Update, &error);
            app.update.mark_failed();
        }
    }
    render_update_ui(window, &app);
}

pub(super) fn render_update_ui(window: &AppWindow, app: &AppState) {
    let language = app.preferences.language.resolve();
    let (status, action, enabled, show_banner) = match app.update.ui_state() {
        UpdateUiState::Ready => (
            barepdf_i18n::t(language, "updates.status.ready").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Checking => (
            barepdf_i18n::t(language, "updates.status.checking").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Current => (
            barepdf_i18n::t(language, "updates.status.current").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Downloading => (
            barepdf_i18n::t(language, "updates.status.downloading").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Installing => (
            barepdf_i18n::t(language, "updates.status.installing").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Verified => (
            barepdf_i18n::t(language, "updates.status.verified").to_string(),
            barepdf_i18n::t(language, "updates.action.install"),
            true,
            true,
        ),
        UpdateUiState::Error => (
            barepdf_i18n::t(language, "updates.status.error").to_string(),
            "",
            false,
            false,
        ),
        UpdateUiState::Available => {
            let Some((status, action)) = available_update_ui(app, language) else {
                return;
            };
            (status, action, true, true)
        }
    };
    window.set_update_status(SharedString::from(status.as_str()));
    window.set_update_action_label(SharedString::from(action));
    window.set_update_action_enabled(enabled);
    if show_banner {
        window.set_banner_text(SharedString::from(status.as_str()));
        window.set_banner_can_retry(false);
        window.set_banner_update_action(true);
        window.set_banner_action_label(SharedString::from(action));
        window.set_banner_action_enabled(enabled);
        window.set_banner_visible(true);
    } else if window.get_banner_update_action() {
        window.set_banner_update_action(false);
        window.set_banner_action_label(SharedString::default());
        window.set_banner_action_enabled(false);
        window.set_banner_visible(false);
    }
}

fn available_update_ui(
    app: &AppState,
    language: barepdf_i18n::ResolvedLanguage,
) -> Option<(String, &'static str)> {
    let update = app.update.available_update()?;
    let note = update
        .release_notes()
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(160)
        .collect::<String>();
    let status = if note.is_empty() {
        format!(
            "{} v{}",
            barepdf_i18n::t(language, "updates.status.available"),
            update.version()
        )
    } else {
        format!(
            "{} v{} — {}",
            barepdf_i18n::t(language, "updates.status.available"),
            update.version(),
            note
        )
    };
    let action = barepdf_i18n::t(
        language,
        if is_installed_build() {
            "updates.action.download"
        } else {
            "updates.action.release"
        },
    );
    Some((status, action))
}

fn update_check_is_due(preferences: &UserPreferences, now: u64) -> bool {
    preferences
        .last_update_check_unix
        .is_none_or(|last| last > now || now - last >= AUTO_CHECK_INTERVAL_SECONDS)
}

pub(super) fn startup_update_check_should_run(app: &AppState, now: u64) -> bool {
    app.update
        .allows_automatic_checks(app.preferences.update_checks_enabled)
        && update_check_is_due(&app.preferences, now)
}

pub(super) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_update_checks_are_evaluated_only_at_startup() {
        let mut preferences = UserPreferences::default();
        assert!(update_check_is_due(&preferences, 100_000));
        preferences.last_update_check_unix = Some(100_000);
        assert!(!update_check_is_due(&preferences, 100_001));
        assert!(update_check_is_due(&preferences, 99_999));
        assert!(update_check_is_due(
            &preferences,
            100_000 + AUTO_CHECK_INTERVAL_SECONDS
        ));

        let mut app = AppState::new(preferences);
        app.preferences.update_checks_enabled = Some(true);
        assert!(startup_update_check_should_run(
            &app,
            100_000 + AUTO_CHECK_INTERVAL_SECONDS
        ));
        assert!(app.update.begin_check());
        assert!(!startup_update_check_should_run(
            &app,
            100_000 + AUTO_CHECK_INTERVAL_SECONDS
        ));
    }
}
