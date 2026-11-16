use std::ffi::OsStr;
use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, Registry};

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

static RELOAD_HANDLE: OnceLock<reload::Handle<LevelFilter, Registry>> = OnceLock::new();
static INIT_MUTEX: Mutex<()> = Mutex::new(());

pub(crate) fn init() {
    let has_log_flag = std::env::args_os().any(|arg| arg == "--log");
    let Some(level) =
        resolve_opt_in_with_args(std::env::var_os("BAREPDF_LOG").as_deref(), has_log_flag)
    else {
        return;
    };
    install_or_raise_level(LevelFilter::from_level(level));
}

/// Ensures structured `tracing` output is active (at least `INFO` level) to both
/// `stderr` and the platform log file during update downloads, even when `--log`
/// or `BAREPDF_LOG` was not supplied on startup.
pub(crate) fn ensure_update_logging() {
    install_or_raise_level(LevelFilter::INFO);
}

fn install_or_raise_level(min_level: LevelFilter) {
    let _guard = match INIT_MUTEX.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(handle) = RELOAD_HANDLE.get() {
        let _ = handle.modify(|current| {
            if *current < min_level {
                *current = min_level;
            }
        });
        return;
    }
    let log_file = default_log_path().and_then(|path| open_log_file_at(&path).ok());
    let writer = TeeWriter::new(log_file);
    let (filter_layer, reload_handle) = reload::Layer::new(min_level);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(writer);
    let subscriber = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer);
    if subscriber.try_init().is_ok() {
        let _ = RELOAD_HANDLE.set(reload_handle);
    }
}

#[cfg(any(windows, test))]
pub(crate) fn windows_log_path(local_app_data: Option<&OsStr>) -> Option<PathBuf> {
    let base = local_app_data.filter(|value| !value.is_empty())?;
    Some(
        PathBuf::from(base)
            .join("BarePDF")
            .join("logs")
            .join("barepdf.log"),
    )
}

#[cfg(any(not(windows), test))]
pub(crate) fn linux_log_path(
    xdg_cache_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(cache_home) = xdg_cache_home.filter(|value| !value.is_empty()) {
        return Some(
            PathBuf::from(cache_home)
                .join("barepdf")
                .join("logs")
                .join("barepdf.log"),
        );
    }
    let home = home.filter(|value| !value.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".cache")
            .join("barepdf")
            .join("logs")
            .join("barepdf.log"),
    )
}

pub(crate) fn default_log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        windows_log_path(std::env::var_os("LOCALAPPDATA").as_deref())
    }
    #[cfg(not(windows))]
    {
        linux_log_path(
            std::env::var_os("XDG_CACHE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }
}

pub(crate) fn open_log_file_at(log_path: &Path) -> io::Result<File> {
    if let Some(parent) = log_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(log_path)
}

#[cfg(test)]
pub(crate) fn init_log_file_in_dir(base_dir: &Path) -> io::Result<File> {
    let log_path = base_dir.join("BarePDF").join("logs").join("barepdf.log");
    open_log_file_at(&log_path)
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

    #[test]
    fn windows_log_path_resolves_localappdata_hierarchy() {
        assert_eq!(windows_log_path(None), None);
        assert_eq!(windows_log_path(Some(OsStr::new(""))), None);
        let path = windows_log_path(Some(OsStr::new(r"C:\Users\Alice\AppData\Local")))
            .expect("windows log path");
        assert_eq!(
            path,
            PathBuf::from(r"C:\Users\Alice\AppData\Local")
                .join("BarePDF")
                .join("logs")
                .join("barepdf.log")
        );
    }

    #[test]
    fn linux_log_path_prefers_xdg_cache_home_then_dot_cache() {
        assert_eq!(linux_log_path(None, None), None);
        assert_eq!(
            linux_log_path(Some(OsStr::new("")), Some(OsStr::new(""))),
            None
        );
        let xdg_path = linux_log_path(
            Some(OsStr::new("/var/cache/custom")),
            Some(OsStr::new("/home/alice")),
        )
        .expect("xdg log path");
        assert_eq!(
            xdg_path,
            PathBuf::from("/var/cache/custom")
                .join("barepdf")
                .join("logs")
                .join("barepdf.log")
        );
        let home_path =
            linux_log_path(Some(OsStr::new("")), Some(OsStr::new("/home/alice"))).expect("home");
        assert_eq!(
            home_path,
            PathBuf::from("/home/alice")
                .join(".cache")
                .join("barepdf")
                .join("logs")
                .join("barepdf.log")
        );
    }

    #[test]
    fn reloadable_filter_upgrades_warn_to_info_for_update_logging() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let log_path = temp.path().join("barepdf").join("logs").join("barepdf.log");
        let file = open_log_file_at(&log_path).expect("log file should open");
        let (filter_layer, reload_handle) = reload::Layer::new(LevelFilter::WARN);
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(TeeWriter::new(Some(file)));
        let subscriber = tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(event = "hidden_before_upgrade", "should not appear");
            let _ = reload_handle.modify(|current| {
                if *current < LevelFilter::INFO {
                    *current = LevelFilter::INFO;
                }
            });
            tracing::info!(event = "update_download_started", "visible after upgrade");
        });

        let contents = fs::read_to_string(&log_path).expect("log file should be readable");
        assert!(!contents.contains("hidden_before_upgrade"));
        assert!(contents.contains("update_download_started"));
        assert!(contents.contains("visible after upgrade"));
    }
}
