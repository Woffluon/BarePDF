use std::ffi::OsStr;
use std::fmt::Display;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticEvent {
    ClipboardWrite,
    PreferencesSave,
    PrintShutdown,
    PrintWorkerStart,
    ReleasePageOpen,
    RenderShutdown,
    Update,
    UpdaterShutdown,
}

impl DiagnosticEvent {
    const fn code(self) -> &'static str {
        match self {
            Self::ClipboardWrite => "clipboard_write_failed",
            Self::PreferencesSave => "preferences_save_failed",
            Self::PrintShutdown => "print_shutdown_failed",
            Self::PrintWorkerStart => "print_worker_start_failed",
            Self::ReleasePageOpen => "release_page_open_failed",
            Self::RenderShutdown => "render_shutdown_failed",
            Self::Update => "update_failed",
            Self::UpdaterShutdown => "updater_shutdown_failed",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::ClipboardWrite => "selected text could not be copied",
            Self::PreferencesSave => "preferences could not be saved",
            Self::PrintShutdown => "print worker shutdown was incomplete",
            Self::PrintWorkerStart => "print worker could not be started",
            Self::ReleasePageOpen => "release page could not be opened",
            Self::RenderShutdown => "render worker shutdown was incomplete",
            Self::Update => "update operation failed",
            Self::UpdaterShutdown => "updater shutdown was incomplete",
        }
    }
}

pub(crate) fn init() {
    let Some(level) = parse_opt_in(std::env::var_os("BAREPDF_LOG").as_deref()) else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .try_init();
}

pub(crate) fn warn_redacted(event: DiagnosticEvent, sensitive_detail: &dyn Display) {
    let (code, message) = redacted_event(event, sensitive_detail);
    tracing::warn!(event = code, "{message}");
}

fn parse_opt_in(value: Option<&OsStr>) -> Option<tracing::Level> {
    let value = value?.to_str()?.trim();
    if value == "1" || value.eq_ignore_ascii_case("true") {
        return Some(tracing::Level::INFO);
    }
    value.parse().ok()
}

fn redacted_event(
    event: DiagnosticEvent,
    _sensitive_detail: &dyn Display,
) -> (&'static str, &'static str) {
    (event.code(), event.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_requires_an_explicit_supported_opt_in() {
        assert_eq!(parse_opt_in(None), None);
        assert_eq!(parse_opt_in(Some(OsStr::new(""))), None);
        assert_eq!(parse_opt_in(Some(OsStr::new("off"))), None);
        assert_eq!(
            parse_opt_in(Some(OsStr::new("1"))),
            Some(tracing::Level::INFO)
        );
        assert_eq!(
            parse_opt_in(Some(OsStr::new("debug"))),
            Some(tracing::Level::DEBUG)
        );
    }

    #[test]
    fn diagnostic_events_discard_sensitive_details() {
        let secret = r"C:\Users\private\installer.exe?token=secret";
        let rendered = format!("{:?}", redacted_event(DiagnosticEvent::Update, &secret));
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("installer.exe"));
        assert!(!rendered.contains("token=secret"));
    }
}
