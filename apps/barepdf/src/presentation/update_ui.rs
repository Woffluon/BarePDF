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

pub(crate) fn queue_update_check(
    sender: &std::sync::mpsc::Sender<UpdateCommand>,
    canceller: &UpdateCheckCanceller,
    state: &Rc<RefCell<AppState>>,
    window: &AppWindow,
) {
    let mut app = state.borrow_mut();
    if !app.update.begin_check() {
        return;
    }
    let cancellation = canceller.begin_check();
    app.wake_pump();
    render_update_ui(window, &app);
    if sender.send(UpdateCommand::Check(cancellation)).is_err() {
        app.update.mark_failed();
        render_update_ui(window, &app);
    }
}

pub(crate) fn handle_update_event(
    event: UpdateEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    preferences_path: &Path,
) {
    let mut app = state.borrow_mut();
    match event {
        UpdateEvent::UpToDate => {
            app.preferences.last_update_check_unix = Some(unix_timestamp());
            persist_preferences(&app.preferences, preferences_path, Some(window));
            app.update.mark_current();
        }
        UpdateEvent::Available(update) => {
            app.preferences.last_update_check_unix = Some(unix_timestamp());
            persist_preferences(&app.preferences, preferences_path, Some(window));
            app.update.mark_available(update);
        }
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

pub(crate) fn render_update_ui(window: &AppWindow, app: &AppState) {
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
            "{} v{}: {}",
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

#[allow(dead_code)]
pub(crate) fn update_check_is_due(preferences: &UserPreferences, now: u64) -> bool {
    preferences
        .last_update_check_unix
        .is_none_or(|last| last > now || now - last >= AUTO_CHECK_INTERVAL_SECONDS)
}

pub(crate) fn startup_update_check_should_run(app: &AppState) -> bool {
    app.update
        .allows_automatic_checks(app.preferences.update_checks_enabled)
}

pub(crate) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{UpdateFailure, VerifiedUpdate};

    #[test]
    fn startup_update_check_runs_when_enabled_regardless_of_last_check_timestamp() {
        // Even if checked 1 second ago (or right now), startup check runs if enabled
        let preferences = UserPreferences {
            last_update_check_unix: Some(100_000),
            update_checks_enabled: Some(true),
            ..Default::default()
        };

        let mut app = AppState::new(preferences);
        assert!(startup_update_check_should_run(&app));

        // When busy (e.g. check already in progress), it does not start another check
        assert!(app.update.begin_check());
        assert!(!startup_update_check_should_run(&app));
    }

    #[test]
    fn startup_update_check_does_not_run_when_disabled_or_unset() {
        let preferences = UserPreferences {
            update_checks_enabled: Some(false),
            ..Default::default()
        };
        let app = AppState::new(preferences);
        assert!(!startup_update_check_should_run(&app));

        let preferences_unset = UserPreferences {
            update_checks_enabled: None,
            ..Default::default()
        };
        let app_unset = AppState::new(preferences_unset);
        assert!(!startup_update_check_should_run(&app_unset));
    }

    #[test]
    fn update_check_is_due_respects_throttle_interval() {
        let mut preferences = UserPreferences::default();
        assert!(update_check_is_due(&preferences, 100_000));
        preferences.last_update_check_unix = Some(100_000);
        assert!(!update_check_is_due(&preferences, 100_001));
        assert!(update_check_is_due(&preferences, 99_999));
        assert!(update_check_is_due(
            &preferences,
            100_000 + AUTO_CHECK_INTERVAL_SECONDS
        ));
    }

    #[test]
    fn queue_update_check_does_not_record_timestamp() {
        crate::presentation::test_support::run_on_ui_thread(|| {
            let window = AppWindow::new().expect("slint app window");
            let (sender, _receiver) = std::sync::mpsc::channel();
            let canceller = UpdateCheckCanceller::default();
            let preferences = UserPreferences::default();
            let state = Rc::new(RefCell::new(AppState::new(preferences)));

            queue_update_check(&sender, &canceller, &state, &window);

            assert_eq!(state.borrow().preferences.last_update_check_unix, None);
            assert_eq!(state.borrow().update.ui_state(), UpdateUiState::Checking);
        });
    }

    #[test]
    fn handle_update_event_only_records_timestamp_on_success() {
        crate::presentation::test_support::run_on_ui_thread(|| {
            let window = AppWindow::new().expect("slint app window");
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let prefs_path = temp_dir.path().join("preferences.json");
            let preferences = UserPreferences::default();
            let state = Rc::new(RefCell::new(AppState::new(preferences)));

            // 1. Error does NOT record timestamp
            let err = UpdateFailure::Rejected("test failure");
            handle_update_event(UpdateEvent::Error(err), &window, &state, &prefs_path);
            assert_eq!(state.borrow().preferences.last_update_check_unix, None);
            assert_eq!(state.borrow().update.ui_state(), UpdateUiState::Error);

            // 2. UpToDate records timestamp
            handle_update_event(UpdateEvent::UpToDate, &window, &state, &prefs_path);
            let first_ts = state.borrow().preferences.last_update_check_unix;
            assert!(first_ts.is_some());
            assert_eq!(state.borrow().update.ui_state(), UpdateUiState::Current);

            // 3. Error afterwards does not erase or overwrite the timestamp
            let err2 = UpdateFailure::Rejected("subsequent failure");
            handle_update_event(UpdateEvent::Error(err2), &window, &state, &prefs_path);
            assert_eq!(state.borrow().preferences.last_update_check_unix, first_ts);
            assert_eq!(state.borrow().update.ui_state(), UpdateUiState::Error);

            // 4. Available records new timestamp
            let update = VerifiedUpdate::for_test("2.0.0", "a".repeat(64), 1024);
            handle_update_event(UpdateEvent::Available(update), &window, &state, &prefs_path);
            assert!(state.borrow().preferences.last_update_check_unix.is_some());
            assert_eq!(state.borrow().update.ui_state(), UpdateUiState::Available);
        });
    }
}
