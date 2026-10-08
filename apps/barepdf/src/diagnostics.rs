use std::ffi::OsStr;
use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticEvent {
    ClipboardWrite,
    PreferencesLoad,
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
            Self::PreferencesLoad => "preferences_load_failed",
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
            Self::PreferencesLoad => "preferences could not be loaded",
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

#[derive(Clone, Default)]
struct TeeWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl TeeWriter {
    fn new(file: Option<File>) -> Self {
        Self {
            file: file.map(|f| Arc::new(Mutex::new(f))),
        }
    }
}

impl<'a> MakeWriter<'a> for TeeWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(file) = &self.file {
            if let Ok(mut guard) = file.lock() {
                let _ = guard.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        if let Some(file) = &self.file {
            if let Ok(mut guard) = file.lock() {
                let _ = guard.flush();
            }
        }
        Ok(())
    }
}

pub(crate) fn init() {
    let has_log_flag = std::env::args_os().any(|arg| arg == "--log");
    let Some(level) =
        resolve_opt_in_with_args(std::env::var_os("BAREPDF_LOG").as_deref(), has_log_flag)
    else {
        return;
    };
    let log_file = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .and_then(|base_dir| init_log_file_in_dir(&base_dir).ok());
    let writer = TeeWriter::new(log_file);
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .with_writer(writer)
        .try_init();
}

pub(crate) fn init_log_file_in_dir(base_dir: &Path) -> io::Result<File> {
    let log_dir = base_dir.join("BarePDF").join("logs");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("barepdf.log");
    OpenOptions::new().create(true).append(true).open(log_path)
}

pub(crate) fn resolve_opt_in_with_args(
    env_val: Option<&OsStr>,
    has_log_flag: bool,
) -> Option<tracing::Level> {
    if let Some(level) = parse_opt_in(env_val) {
        return Some(level);
    }
    if has_log_flag {
        return Some(tracing::Level::INFO);
    }
    None
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

fn redact_sensitive_token(token: &str) -> String {
    let trimmed = token.trim_matches(|c: char| {
        c == '"' || c == '\'' || c == '(' || c == ')' || c == ':' || c == ','
    });
    if trimmed.is_empty() {
        return token.to_string();
    }
    let is_windows_path = (trimmed.len() >= 3
        && trimmed.as_bytes()[1] == b':'
        && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/'))
        || trimmed.starts_with(r"\\");
    let is_unix_path = trimmed.starts_with('/') && trimmed.contains('/') && trimmed.len() > 1;
    let is_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    let is_sensitive_query = trimmed.contains("token=") || trimmed.contains("key=");

    if is_windows_path || is_unix_path {
        token.replace(trimmed, "[path]")
    } else if is_url {
        token.replace(trimmed, "[url]")
    } else if is_sensitive_query {
        token.replace(trimmed, "[redacted]")
    } else {
        token.to_string()
    }
}

pub(crate) fn redact_sensitive_info(raw: &str) -> String {
    raw.split_whitespace()
        .map(redact_sensitive_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redacted_event(
    event: DiagnosticEvent,
    sensitive_detail: &dyn Display,
) -> (&'static str, String) {
    let sanitized = redact_sensitive_info(&sensitive_detail.to_string());
    let message = if sanitized.is_empty() {
        event.message().to_string()
    } else {
        format!("{}: {}", event.message(), sanitized)
    };
    (event.code(), message)
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
    fn resolve_opt_in_with_args_honors_flag_and_env() {
        assert_eq!(resolve_opt_in_with_args(None, false), None);
        assert_eq!(resolve_opt_in_with_args(Some(OsStr::new("")), false), None);
        assert_eq!(
            resolve_opt_in_with_args(Some(OsStr::new("off")), false),
            None
        );
        assert_eq!(
            resolve_opt_in_with_args(None, true),
            Some(tracing::Level::INFO)
        );
        assert_eq!(
            resolve_opt_in_with_args(Some(OsStr::new("off")), true),
            Some(tracing::Level::INFO)
        );
        assert_eq!(
            resolve_opt_in_with_args(Some(OsStr::new("debug")), false),
            Some(tracing::Level::DEBUG)
        );
        assert_eq!(
            resolve_opt_in_with_args(Some(OsStr::new("debug")), true),
            Some(tracing::Level::DEBUG)
        );
    }

    #[test]
    fn init_log_file_in_dir_creates_and_appends_log_lines() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let file = init_log_file_in_dir(temp.path()).expect("log file should be created");
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(TeeWriter::new(Some(file)))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            warn_redacted(
                DiagnosticEvent::Update,
                &r"failed to verify C:\Users\Alice\setup.exe",
            );
        });

        let log_path = temp.path().join("BarePDF").join("logs").join("barepdf.log");
        assert!(log_path.is_file());
        let contents = fs::read_to_string(&log_path).expect("log file should be readable");
        assert!(contents.contains("update_failed"));
        assert!(contents.contains("update operation failed"));
        assert!(contents.contains("[path]"));
        assert!(!contents.contains("Alice"));
        assert!(!contents.contains("\u{1b}["));
    }

    #[test]
    fn diagnostic_events_discard_sensitive_details() {
        let secret = r"C:\Users\private\installer.exe?token=secret";
        let rendered = format!("{:?}", redacted_event(DiagnosticEvent::Update, &secret));
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("installer.exe"));
        assert!(!rendered.contains("token=secret"));
    }

    #[test]
    fn diagnostic_events_preserve_error_kind_without_leaking_paths() {
        let error = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "access denied to C:\\Users\\Administrator\\secret.json",
        );
        let (code, message) = redacted_event(DiagnosticEvent::PreferencesSave, &error);
        assert_eq!(code, "preferences_save_failed");
        assert!(!message.contains("Administrator"));
        assert!(!message.contains("secret.json"));
        assert!(message.contains("[path]"));
        assert!(message.contains("access denied to"));
    }
}
