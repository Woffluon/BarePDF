#[cfg(windows)]
use super::manifest::expected_file_version;
use super::manifest::{validate_installer_url, VerifiedUpdate};
use super::transport::{check_cancelled, get_with_redirects, RequestTarget};
use super::UpdateFailure;
#[cfg(windows)]
use barepdf_platform_windows::executable_file_version;
#[cfg(windows)]
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ureq::Agent;

const INSTALLER_TIMEOUT: Duration = Duration::from_mins(30);
const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const UI_PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(80);
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x0000_0001;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

static VERIFIED_INSTALLER_LOCK: Mutex<Option<(PathBuf, File)>> = Mutex::new(None);

fn retain_verified_installer_lock(path: &Path, file: File) {
    let mut guard = match VERIFIED_INSTALLER_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = Some((path.to_path_buf(), file));
}

fn release_verified_installer_lock(path: &Path) {
    let mut guard = match VERIFIED_INSTALLER_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard
        .as_ref()
        .is_some_and(|(locked_path, _)| locked_path == path)
    {
        *guard = None;
    }
}

#[cfg(windows)]
fn update_directory() -> Result<PathBuf, UpdateFailure> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or(UpdateFailure::Rejected("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(local_app_data)
        .join("BarePDF")
        .join("Updates"))
}

#[cfg(not(windows))]
fn update_directory() -> Result<PathBuf, UpdateFailure> {
    if let Some(cache_home) = std::env::var_os("XDG_CACHE_HOME") {
        Ok(PathBuf::from(cache_home).join("barepdf").join("updates"))
    } else if let Some(home) = std::env::var_os("HOME") {
        Ok(PathBuf::from(home)
            .join(".cache")
            .join("barepdf")
            .join("updates"))
    } else {
        Ok(std::env::temp_dir().join("barepdf-updates"))
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_MIB
}

fn safe_file_label(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("installer")
}

fn sha256_hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DownloadMilestone {
    percent: u64,
    downloaded_mb: f64,
    total_mb: f64,
    elapsed_ms: u64,
    elapsed_secs: f64,
    speed_mb_s: f64,
}

struct DownloadProgressTracker {
    total_bytes: u64,
    started_at: Instant,
    last_milestone_at: Instant,
    last_milestone_bytes: u64,
    next_milestone_percent: u64,
}

impl DownloadProgressTracker {
    fn new(total_bytes: u64, started_at: Instant) -> Self {
        Self {
            total_bytes,
            started_at,
            last_milestone_at: started_at,
            last_milestone_bytes: 0,
            next_milestone_percent: 10,
        }
    }

    fn record(&mut self, downloaded: u64, now: Instant) -> Option<DownloadMilestone> {
        if self.total_bytes == 0 || self.next_milestone_percent > 100 {
            return None;
        }
        let percent = u64::try_from(
            u128::from(downloaded).saturating_mul(100) / u128::from(self.total_bytes),
        )
        .unwrap_or(100);
        let bucket = (percent / 10).saturating_mul(10).min(100);
        if bucket < self.next_milestone_percent {
            return None;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        let delta_bytes = downloaded.saturating_sub(self.last_milestone_bytes);
        let delta_secs = now
            .saturating_duration_since(self.last_milestone_at)
            .as_secs_f64();
        let speed_mb_s = if delta_secs > 1e-6 {
            bytes_to_mb(delta_bytes) / delta_secs
        } else {
            0.0
        };
        self.next_milestone_percent = bucket + 10;
        self.last_milestone_at = now;
        self.last_milestone_bytes = downloaded;
        Some(DownloadMilestone {
            percent: bucket,
            downloaded_mb: bytes_to_mb(downloaded),
            total_mb: bytes_to_mb(self.total_bytes),
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            elapsed_secs: elapsed.as_secs_f64(),
            speed_mb_s,
        })
    }
}

pub(super) fn download_update<F>(
    agent: &Agent,
    update: &VerifiedUpdate,
    cancelled: &AtomicBool,
    mut on_progress: F,
) -> Result<PathBuf, UpdateFailure>
where
    F: FnMut(u64, u64, bool),
{
    crate::diagnostics::ensure_update_logging();
    tracing::info!(
        event = "update_download_started",
        version = %update.version(),
        expected_bytes = update.installer_size,
        "Starting update installer download"
    );
    on_progress(0, update.installer_size, false);
    let result = download_update_inner(agent, update, cancelled, &mut on_progress);
    match &result {
        Ok(path) => {
            tracing::info!(
                event = "update_download_completed",
                version = %update.version(),
                file = safe_file_label(path),
                "Update installer download and verification completed"
            );
        }
        Err(UpdateFailure::Cancelled) => {
            tracing::warn!(
                event = "update_download_cancelled",
                version = %update.version(),
                "Update installer download cancelled"
            );
        }
        Err(error) => {
            let reason = crate::diagnostics::redact_sensitive_info(&error.to_string());
            tracing::warn!(
                event = "update_download_failed",
                version = %update.version(),
                reason = %reason,
                "Update installer download failed"
            );
        }
    }
    result
}

fn download_update_inner<F>(
    agent: &Agent,
    update: &VerifiedUpdate,
    cancelled: &AtomicBool,
    on_progress: &mut F,
) -> Result<PathBuf, UpdateFailure>
where
    F: FnMut(u64, u64, bool),
{
    validate_installer_url(&update.installer_url, update.version())?;
    let directory = update_directory()?;
    fs::create_dir_all(&directory).map_err(|source| UpdateFailure::Io {
        operation: "Could not create update directory",
        source,
    })?;
    #[cfg(target_os = "linux")]
    let final_path = directory.join(format!("barepdf_{}_amd64.deb", update.version()));
    #[cfg(not(target_os = "linux"))]
    let final_path = directory.join(format!("BarePDF-Setup-x64-v{}.exe", update.version()));
    let (partial_path, partial_file) = create_unique_partial(&directory, update.version())?;
    let result =
        download_to_partial(agent, update, partial_file, cancelled, on_progress).and_then(|()| {
            check_cancelled(cancelled)?;
            on_progress(update.installer_size, update.installer_size, true);
            verify_download(&partial_path, update)?;
            release_verified_installer_lock(&partial_path);
            release_verified_installer_lock(&final_path);
            check_cancelled(cancelled)?;
            if final_path.is_file() {
                fs::remove_file(&final_path).map_err(|source| UpdateFailure::Io {
                    operation: "Could not replace previous update",
                    source,
                })?;
            }
            fs::rename(&partial_path, &final_path).map_err(|source| UpdateFailure::Io {
                operation: "Could not finalize update",
                source,
            })?;
            tracing::info!(
                event = "update_atomic_rename_succeeded",
                version = %update.version(),
                file = safe_file_label(&final_path),
                "Atomic update file rename succeeded"
            );
            verify_download(&final_path, update)?;
            Ok(final_path.clone())
        });
    finish_download(result, &partial_path)
}

pub(super) fn verify_download(path: &Path, update: &VerifiedUpdate) -> Result<(), UpdateFailure> {
    let locked_file = verify_file_integrity(path, update.installer_size, &update.installer_sha256)?;
    #[cfg(windows)]
    {
        let actual_version = executable_file_version(path).map_err(|source| {
            tracing::warn!(
                event = "update_pe_version_failed",
                version = %update.version(),
                file = safe_file_label(path),
                "Windows PE file version verification failed"
            );
            UpdateFailure::Platform {
                operation: "Could not read downloaded update version",
                source,
            }
        })?;
        let expected_version = Version::parse(update.version())
            .map_err(|source| UpdateFailure::Version {
                operation: "Invalid release version",
                source,
            })
            .and_then(|version| expected_file_version(&version))?;
        if actual_version != expected_version {
            tracing::warn!(
                event = "update_pe_version_mismatch",
                version = %update.version(),
                file = safe_file_label(path),
                actual_pe_version = ?actual_version,
                expected_pe_version = ?expected_version,
                "Downloaded Windows PE file version does not match metadata"
            );
            return Err(UpdateFailure::Rejected(
                "Downloaded installer version does not match update metadata",
            ));
        }
        tracing::info!(
            event = "update_pe_version_verified",
            version = %update.version(),
            file = safe_file_label(path),
            pe_version = ?actual_version,
            "Windows PE file version verified"
        );
    }
    retain_verified_installer_lock(path, locked_file);
    Ok(())
}

fn verify_file_integrity(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<File, UpdateFailure> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.share_mode(FILE_SHARE_READ);
    let mut file = options.open(path).map_err(|source| UpdateFailure::Io {
        operation: "Could not reopen downloaded update",
        source,
    })?;
    let metadata = file.metadata().map_err(|source| UpdateFailure::Io {
        operation: "Could not inspect downloaded update",
        source,
    })?;
    #[cfg(windows)]
    if !metadata.is_file() || (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer must be a regular non-reparse file",
        ));
    }
    #[cfg(not(windows))]
    if !metadata.is_file() {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer must be a regular file",
        ));
    }
    validate_download_size(metadata.len(), expected_size)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| UpdateFailure::Io {
            operation: "Could not rehash downloaded update",
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if sha256_hex_digest(hasher.finalize().as_ref()) != expected_sha256 {
        tracing::warn!(
            event = "update_file_sha256_mismatch",
            file = safe_file_label(path),
            "Installer file SHA-256 integrity verification failed"
        );
        return Err(UpdateFailure::Rejected(
            "Downloaded installer changed after verification",
        ));
    }
    tracing::info!(
        event = "update_file_sha256_verified",
        file = safe_file_label(path),
        bytes = expected_size,
        "Installer file SHA-256 integrity verified"
    );
    Ok(file)
}

fn validate_download_size(actual: u64, expected: u64) -> Result<(), UpdateFailure> {
    if actual < expected {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer size does not match metadata",
        ));
    }
    if actual > expected {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer exceeds the declared size",
        ));
    }
    Ok(())
}

fn reject_oversized_download(actual: u64, expected: u64) -> Result<(), UpdateFailure> {
    if actual > expected {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer exceeds the declared size",
        ));
    }
    Ok(())
}

fn discard_partial(path: &Path) {
    release_verified_installer_lock(path);
    let _ = fs::remove_file(path);
}

fn finish_download(
    result: Result<PathBuf, UpdateFailure>,
    partial_path: &Path,
) -> Result<PathBuf, UpdateFailure> {
    if result.is_err() {
        discard_partial(partial_path);
    }
    result
}

fn create_unique_partial(
    directory: &Path,
    version: &str,
) -> Result<(PathBuf, File), UpdateFailure> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UpdateFailure::Rejected("System clock is before the Unix epoch"))?
        .as_nanos();
    for attempt in 0..16_u8 {
        #[cfg(target_os = "linux")]
        let filename = format!(
            ".barepdf_{version}_amd64-{}-{nonce}-{attempt}.partial",
            std::process::id()
        );
        #[cfg(not(target_os = "linux"))]
        let filename = format!(
            ".BarePDF-Setup-x64-v{version}-{}-{nonce}-{attempt}.partial",
            std::process::id()
        );
        let path = directory.join(filename);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        options.share_mode(0);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(UpdateFailure::Io {
                    operation: "Could not create update file",
                    source,
                });
            }
        }
    }
    Err(UpdateFailure::Rejected(
        "Could not allocate a unique update file",
    ))
}

fn download_to_partial<F>(
    agent: &Agent,
    update: &VerifiedUpdate,
    mut file: File,
    cancelled: &AtomicBool,
    on_progress: &mut F,
) -> Result<(), UpdateFailure>
where
    F: FnMut(u64, u64, bool),
{
    let (mut response, _) = get_with_redirects(
        agent,
        &update.installer_url,
        RequestTarget::Installer(update.version()),
        INSTALLER_TIMEOUT,
        cancelled,
    )?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(update.installer_size.saturating_add(1))
        .reader();
    stream_reader_to_partial(&mut reader, update, &mut file, cancelled, on_progress)
}

fn stream_reader_to_partial<R, F>(
    reader: &mut R,
    update: &VerifiedUpdate,
    file: &mut File,
    cancelled: &AtomicBool,
    on_progress: &mut F,
) -> Result<(), UpdateFailure>
where
    R: Read,
    F: FnMut(u64, u64, bool),
{
    let started_at = Instant::now();
    let mut tracker = DownloadProgressTracker::new(update.installer_size, started_at);
    let mut last_ui_emit = started_at;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let read = reader
            .read(&mut buffer)
            .map_err(|source| UpdateFailure::Io {
                operation: "Could not download update",
                source,
            })?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or(UpdateFailure::Rejected("Installer size overflow"))?;
        reject_oversized_download(downloaded, update.installer_size)?;
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|source| UpdateFailure::Io {
                operation: "Could not write update file",
                source,
            })?;
        let now = Instant::now();
        if let Some(milestone) = tracker.record(downloaded, now) {
            tracing::info!(
                event = "update_download_progress",
                version = %update.version(),
                percent = milestone.percent,
                downloaded_mb = format_args!("{:.2}", milestone.downloaded_mb),
                total_mb = format_args!("{:.2}", milestone.total_mb),
                elapsed_ms = milestone.elapsed_ms,
                elapsed_s = format_args!("{:.2}", milestone.elapsed_secs),
                speed_mb_s = format_args!("{:.2}", milestone.speed_mb_s),
                "Update download progress"
            );
        }
        if downloaded == update.installer_size
            || now.saturating_duration_since(last_ui_emit) >= UI_PROGRESS_EMIT_INTERVAL
        {
            on_progress(downloaded, update.installer_size, false);
            last_ui_emit = now;
        }
    }
    check_cancelled(cancelled)?;
    file.sync_all().map_err(|source| UpdateFailure::Io {
        operation: "Could not flush update file",
        source,
    })?;
    validate_download_size(downloaded, update.installer_size)?;
    if sha256_hex_digest(hasher.finalize().as_ref()) != update.installer_sha256 {
        tracing::warn!(
            event = "update_sha256_mismatch",
            stage = "download_stream",
            version = %update.version(),
            "Downloaded installer SHA-256 verification failed"
        );
        return Err(UpdateFailure::Rejected(
            "Downloaded installer SHA-256 does not match metadata",
        ));
    }
    tracing::info!(
        event = "update_sha256_verified",
        stage = "download_stream",
        version = %update.version(),
        bytes = downloaded,
        "Downloaded installer SHA-256 verified"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "barepdf-update-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        sha256_hex_digest(Sha256::digest(bytes).as_ref())
    }

    struct ChunkedReader<'a> {
        data: &'a [u8],
        chunk_size: usize,
        offset: usize,
    }

    impl Read for ChunkedReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.offset >= self.data.len() {
                return Ok(0);
            }
            let remaining = self.data.len() - self.offset;
            let count = remaining.min(self.chunk_size).min(buf.len());
            buf[..count].copy_from_slice(&self.data[self.offset..self.offset + count]);
            self.offset += count;
            Ok(count)
        }
    }

    #[test]
    fn partial_update_files_are_unique_and_created_exclusively() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let first = create_unique_partial(&directory, "1.2.0");
        let second = create_unique_partial(&directory, "1.2.0");
        assert!(
            matches!((&first, &second), (Ok((first_path, _)), Ok((second_path, _))) if first_path != second_path)
        );
        drop((first, second));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn file_integrity_enforces_declared_size_and_hash() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let path = directory.join("update.exe");
        let bytes = b"abc";
        assert!(fs::write(&path, bytes).is_ok());
        let hash = sha256_hex(bytes);

        assert!(verify_file_integrity(&path, 2, &hash).is_err());
        assert!(verify_file_integrity(&path, 3, &hash).is_ok());
        assert!(verify_file_integrity(&path, 4, &hash).is_err());
        assert!(matches!(
            verify_file_integrity(&path, 3, &"0".repeat(64)),
            Err(UpdateFailure::Rejected(
                "Downloaded installer changed after verification"
            ))
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn modified_installer_is_rejected_before_launch() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let path = directory.join("update.exe");
        let original = b"abc";
        assert!(fs::write(&path, original).is_ok());
        let update = VerifiedUpdate::for_test("1.2.3", sha256_hex(original), 3);
        assert!(fs::write(&path, b"xyz").is_ok());

        assert!(matches!(
            verify_download(&path, &update),
            Err(UpdateFailure::Rejected(
                "Downloaded installer changed after verification"
            ))
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn failed_download_cleanup_removes_partial_file() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let partial = create_unique_partial(&directory, "1.2.3");
        assert!(matches!(&partial, Ok((path, _)) if path.is_file()));
        let (path, file) = partial.expect("partial file can be created");
        drop(file);

        let error = UpdateFailure::Rejected("download test failure");
        assert!(finish_download(Err(error), &path).is_err());

        assert!(!path.exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(windows)]
    #[test]
    fn verified_installer_lock_prevents_concurrent_modification_and_rejects_open_writers() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let path = directory.join("update-toctou.exe");
        let bytes = b"verified-installer-bytes";
        assert!(fs::write(&path, bytes).is_ok());
        let hash = sha256_hex(bytes);

        let locked_file = verify_file_integrity(&path, bytes.len() as u64, &hash)
            .expect("integrity check locks file with FILE_SHARE_READ");
        retain_verified_installer_lock(&path, locked_file);

        assert!(
            OpenOptions::new().write(true).open(&path).is_err(),
            "modifying verified installer must fail while lock is retained"
        );
        assert!(
            fs::remove_file(&path).is_err(),
            "deleting verified installer must fail while lock is retained"
        );

        release_verified_installer_lock(&path);

        let active_writer = OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("writer opens once lock is released");
        assert!(
            verify_file_integrity(&path, bytes.len() as u64, &hash).is_err(),
            "integrity check must fail if another handle has write access"
        );
        drop(active_writer);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn download_progress_tracker_emits_only_10_percent_milestones_with_speed() {
        let start = Instant::now();
        let total = 10 * 1024 * 1024_u64;
        let mut tracker = DownloadProgressTracker::new(total, start);

        // 5% does not trigger
        assert!(tracker
            .record(total / 20, start + Duration::from_millis(50))
            .is_none());

        // 10% triggers first milestone
        let m10 = tracker
            .record(total / 10, start + Duration::from_millis(100))
            .expect("10% milestone");
        assert_eq!(m10.percent, 10);
        assert!((m10.downloaded_mb - 1.0).abs() < 1e-6);
        assert!((m10.total_mb - 10.0).abs() < 1e-6);
        assert_eq!(m10.elapsed_ms, 100);
        assert!((m10.speed_mb_s - 10.0).abs() < 0.05);

        // Duplicate read inside 10-19% bucket does not emit again
        assert!(tracker
            .record((total * 19) / 100, start + Duration::from_millis(150))
            .is_none());

        // Remaining 10% buckets up to 100%
        let mut percents = vec![m10.percent];
        for pct in (20..=100).step_by(10) {
            let milestone = tracker
                .record((total * pct) / 100, start + Duration::from_millis(pct * 10))
                .expect("expected milestone");
            percents.push(milestone.percent);
        }
        assert_eq!(percents, vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
        assert!(tracker
            .record(total, start + Duration::from_millis(1100))
            .is_none());
    }

    #[test]
    fn stream_reader_to_partial_logs_milestones_and_sha256_verification() {
        let directory = temporary_directory();
        assert!(fs::create_dir_all(&directory).is_ok());
        let log_path = directory.join("update-stream.log");
        let log_file = crate::diagnostics::open_log_file_at(&log_path).expect("open log");
        let payload = vec![0x5A_u8; 20 * 1024];
        let update = VerifiedUpdate::for_test(
            "2.1.0",
            sha256_hex(&payload),
            u64::try_from(payload.len()).expect("payload len"),
        );
        let (partial_path, mut partial_file) =
            create_unique_partial(&directory, update.version()).expect("create partial");
        let cancelled = AtomicBool::new(false);
        let mut reader = ChunkedReader {
            data: &payload,
            chunk_size: 2048,
            offset: 0,
        };
        let mut progress_events = Vec::new();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(log_file))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            stream_reader_to_partial(
                &mut reader,
                &update,
                &mut partial_file,
                &cancelled,
                &mut |downloaded, total, verifying| {
                    progress_events.push((downloaded, total, verifying));
                },
            )
            .expect("stream download succeeds");
        });
        drop(partial_file);

        assert!(!progress_events.is_empty());
        assert_eq!(
            progress_events.last().copied(),
            Some((payload.len() as u64, payload.len() as u64, false))
        );
        let logs = fs::read_to_string(&log_path).expect("read log");
        for pct in (10..=100).step_by(10) {
            assert!(
                logs.contains(&format!("percent={pct} ")),
                "missing percent={pct} in logs:\n{logs}"
            );
        }
        assert!(logs.contains("downloaded_mb="));
        assert!(logs.contains("total_mb="));
        assert!(logs.contains("elapsed_ms="));
        assert!(logs.contains("speed_mb_s="));
        assert!(logs.contains("update_sha256_verified"));

        let _ = fs::remove_file(&partial_path);
        let _ = fs::remove_dir_all(directory);
    }
}
