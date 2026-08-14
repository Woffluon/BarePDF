use barepdf_platform_windows::{executable_file_version, launch_installer};
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const METADATA_URL: &str =
    "https://github.com/Woffluon/BarePDF/releases/latest/download/latest.json";
pub const METADATA_SIGNATURE_URL: &str =
    "https://github.com/Woffluon/BarePDF/releases/latest/download/latest.json.sig";
pub const AUTO_CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 64;
const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALLER_TIMEOUT: Duration = Duration::from_mins(30);
const NETWORK_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_BODY_TIMEOUT: Duration = Duration::from_mins(2);
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(5);
const USER_AGENT: &str = concat!("BarePDF/", env!("CARGO_PKG_VERSION"));
const UPDATE_PUBLIC_KEY_HEX: &str = include_str!("../../../../assets/update-public-key.hex");

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedUpdate {
    version: String,
    release_url: String,
    release_notes: String,
    installer_url: String,
    installer_sha256: String,
    installer_size: u64,
}

impl VerifiedUpdate {
    fn from_manifest(manifest: UpdateManifest, installer_sha256: String) -> Self {
        Self {
            version: manifest.version,
            release_url: manifest.release_url,
            release_notes: manifest.release_notes,
            installer_url: manifest.installer.url,
            installer_sha256,
            installer_size: manifest.installer.size,
        }
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn release_url(&self) -> &str {
        &self.release_url
    }

    pub(crate) fn release_notes(&self) -> &str {
        &self.release_notes
    }
}

#[derive(Debug, thiserror::Error)]
enum UpdateError {
    #[error("Update operation was cancelled")]
    Cancelled,
    #[error("{0}")]
    Rejected(&'static str),
    #[error("{operation}: {source}")]
    Transport {
        operation: &'static str,
        #[source]
        source: ureq::Error,
    },
    #[error("{operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation}: {source}")]
    Manifest {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{operation}: {source}")]
    Version {
        operation: &'static str,
        #[source]
        source: semver::Error,
    },
    #[error("{operation}: {source}")]
    Platform {
        operation: &'static str,
        #[source]
        source: barepdf_platform::PlatformError,
    },
}

#[derive(Debug)]
pub(crate) enum UpdateCommand {
    Check,
    Download(VerifiedUpdate),
    Install {
        path: PathBuf,
        update: VerifiedUpdate,
    },
    Shutdown,
}

#[derive(Debug)]
pub(crate) enum UpdateEvent {
    UpToDate,
    Available(VerifiedUpdate),
    Downloaded {
        path: PathBuf,
        update: VerifiedUpdate,
    },
    InstallerStarted,
    Error(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    schema_version: u32,
    version: String,
    release_url: String,
    #[serde(default)]
    release_notes: String,
    installer: InstallerManifest,
}

#[derive(Deserialize)]
struct InstallerManifest {
    url: String,
    sha256: String,
    size: u64,
}

pub(crate) struct UpdateWorker {
    commands: Sender<UpdateCommand>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateShutdownError {
    #[error("Update worker did not stop before the shutdown deadline")]
    TimedOut,
    #[error("Update worker panicked during shutdown")]
    Panicked,
}

impl UpdateWorker {
    pub(crate) fn command_sender(&self) -> Sender<UpdateCommand> {
        self.commands.clone()
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), UpdateShutdownError> {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.commands.send(UpdateCommand::Shutdown);
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        let deadline = Instant::now() + SHUTDOWN_JOIN_TIMEOUT;
        while !worker.is_finished() {
            if Instant::now() >= deadline {
                return Err(UpdateShutdownError::TimedOut);
            }
            std::thread::sleep(SHUTDOWN_POLL_INTERVAL);
        }
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| UpdateShutdownError::Panicked)
    }
}

impl Drop for UpdateWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub(crate) fn start_worker() -> (UpdateWorker, Receiver<UpdateEvent>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let worker = std::thread::spawn(move || {
        run_worker(&command_receiver, &event_sender, &worker_cancelled);
    });
    (
        UpdateWorker {
            commands: command_sender,
            cancelled,
            worker: Some(worker),
        },
        event_receiver,
    )
}

fn run_worker(
    commands: &Receiver<UpdateCommand>,
    events: &Sender<UpdateEvent>,
    cancelled: &AtomicBool,
) {
    let agent = Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_connect(Some(NETWORK_CONNECT_TIMEOUT))
        .timeout_recv_response(Some(NETWORK_RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(NETWORK_BODY_TIMEOUT))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();

    while let Ok(command) = commands.recv() {
        let event = match command {
            UpdateCommand::Check => match check_for_update(&agent, cancelled) {
                Ok(Some(update)) => UpdateEvent::Available(update),
                Ok(None) => UpdateEvent::UpToDate,
                Err(error) => UpdateEvent::Error(error.to_string()),
            },
            UpdateCommand::Download(update) => match download_update(&agent, &update, cancelled) {
                Ok(path) => UpdateEvent::Downloaded { path, update },
                Err(error) => UpdateEvent::Error(error.to_string()),
            },
            UpdateCommand::Install { path, update } => {
                match verify_download(&path, &update).and_then(|()| {
                    check_cancelled(cancelled)?;
                    launch_installer(&path).map_err(|source| UpdateError::Platform {
                        operation: "Could not launch verified update",
                        source,
                    })
                }) {
                    Ok(()) => UpdateEvent::InstallerStarted,
                    Err(error) => UpdateEvent::Error(error.to_string()),
                }
            }
            UpdateCommand::Shutdown => break,
        };
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        if events.send(event).is_err() {
            break;
        }
    }
}

fn check_for_update(
    agent: &Agent,
    cancelled: &AtomicBool,
) -> Result<Option<VerifiedUpdate>, UpdateError> {
    let mut response = get_with_redirects(
        agent,
        METADATA_URL,
        RequestTarget::Metadata,
        METADATA_TIMEOUT,
        cancelled,
    )?;
    let manifest = response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA_BYTES)
        .read_to_vec()
        .map_err(|source| UpdateError::Transport {
            operation: "Update metadata could not be read",
            source,
        })?;
    check_cancelled(cancelled)?;
    let mut signature_response = get_with_redirects(
        agent,
        METADATA_SIGNATURE_URL,
        RequestTarget::Metadata,
        METADATA_TIMEOUT,
        cancelled,
    )?;
    let signature = signature_response
        .body_mut()
        .with_config()
        .limit(MAX_SIGNATURE_BYTES + 1)
        .read_to_vec()
        .map_err(|source| UpdateError::Transport {
            operation: "Update signature could not be read",
            source,
        })?;
    check_cancelled(cancelled)?;
    verify_manifest_signature(&manifest, &signature)?;
    let json = std::str::from_utf8(&manifest)
        .map_err(|_| UpdateError::Rejected("Update metadata is not valid UTF-8"))?;
    parse_manifest(json, CURRENT_VERSION)
}

fn verify_manifest_signature(manifest: &[u8], signature: &[u8]) -> Result<(), UpdateError> {
    verify_manifest_signature_with_key(manifest, signature, UPDATE_PUBLIC_KEY_HEX)
}

fn verify_manifest_signature_with_key(
    manifest: &[u8],
    signature: &[u8],
    public_key: &str,
) -> Result<(), UpdateError> {
    let public_key = decode_public_key(public_key)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| UpdateError::Rejected("Update verification key is invalid"))?;
    let signature = Signature::from_slice(signature)
        .map_err(|_| UpdateError::Rejected("Update metadata signature is invalid"))?;
    verifying_key
        .verify_strict(manifest, &signature)
        .map_err(|_| UpdateError::Rejected("Update metadata signature verification failed"))
}

fn decode_public_key(value: &str) -> Result<[u8; 32], UpdateError> {
    let value = value.trim().as_bytes();
    if value.len() != 64 {
        return Err(UpdateError::Rejected("Update verification key is invalid"));
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        let high = decode_hex(value[index * 2])?;
        let low = decode_hex(value[index * 2 + 1])?;
        *byte = (high << 4) | low;
    }
    Ok(key)
}

fn decode_hex(value: u8) -> Result<u8, UpdateError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(UpdateError::Rejected("Update verification key is invalid")),
    }
}

fn parse_manifest(
    json: &str,
    current_version: &str,
) -> Result<Option<VerifiedUpdate>, UpdateError> {
    let manifest: UpdateManifest =
        serde_json::from_str(json).map_err(|source| UpdateError::Manifest {
            operation: "Invalid update metadata",
            source,
        })?;
    if manifest.schema_version != 1 {
        return Err(UpdateError::Rejected("Unsupported update metadata version"));
    }

    let current = Version::parse(current_version).map_err(|source| UpdateError::Version {
        operation: "Invalid application version",
        source,
    })?;
    let available = Version::parse(&manifest.version).map_err(|source| UpdateError::Version {
        operation: "Invalid release version",
        source,
    })?;
    if !available.pre.is_empty() || !available.build.is_empty() {
        return Err(UpdateError::Rejected(
            "Preview releases are not accepted on the stable channel",
        ));
    }
    expected_file_version(&available)?;
    if available <= current {
        return Ok(None);
    }

    validate_release_url(&manifest.release_url, &manifest.version)?;
    validate_installer_url(&manifest.installer.url, &manifest.version)?;
    let hash = manifest.installer.sha256.to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdateError::Rejected("Invalid installer SHA-256"));
    }
    if manifest.installer.size == 0 || manifest.installer.size > MAX_INSTALLER_BYTES {
        return Err(UpdateError::Rejected(
            "Installer size is outside the allowed range",
        ));
    }

    Ok(Some(VerifiedUpdate::from_manifest(manifest, hash)))
}

fn validate_release_url(url: &str, version: &str) -> Result<(), UpdateError> {
    let expected = format!("/Woffluon/BarePDF/releases/tag/v{version}");
    validate_github_url(url, &expected)
}

fn validate_installer_url(url: &str, version: &str) -> Result<(), UpdateError> {
    let expected =
        format!("/Woffluon/BarePDF/releases/download/v{version}/BarePDF-Setup-x64-v{version}.exe");
    validate_github_url(url, &expected)
}

fn validate_github_url(url: &str, expected_path: &str) -> Result<(), UpdateError> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| UpdateError::Rejected("Invalid HTTPS URL"))?;
    if uri.scheme_str() != Some("https")
        || uri.host() != Some("github.com")
        || uri.port_u16().is_some()
        || uri.query().is_some()
        || uri.path() != expected_path
    {
        return Err(UpdateError::Rejected(
            "Update URL is not an approved BarePDF release URL",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum RequestTarget<'a> {
    Metadata,
    Installer(&'a str),
}

fn get_with_redirects(
    agent: &Agent,
    initial_url: &str,
    target: RequestTarget<'_>,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<ureq::http::Response<ureq::Body>, UpdateError> {
    let mut url = initial_url.to_string();
    let started = Instant::now();
    for redirect_count in 0..=MAX_REDIRECTS {
        check_cancelled(cancelled)?;
        let uri = validate_request_url(&url, target)?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(UpdateError::Rejected("Update request timed out"))?;
        let response = agent
            .get(uri)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(remaining))
            .build()
            .call()
            .map_err(|source| UpdateError::Transport {
                operation: "Update request failed",
                source,
            })?;
        check_cancelled(cancelled)?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Err(UpdateError::Rejected(
                "Update request returned an unsupported redirect",
            ));
        }
        if redirect_count == MAX_REDIRECTS {
            return Err(UpdateError::Rejected(
                "Update request exceeded the redirect limit",
            ));
        }
        let location = response
            .headers()
            .get(ureq::http::header::LOCATION)
            .ok_or(UpdateError::Rejected("Update redirect is missing Location"))?
            .to_str()
            .map_err(|_| UpdateError::Rejected("Update redirect Location is invalid"))?;
        validate_request_url(location, target)?;
        url.clear();
        url.push_str(location);
    }
    Err(UpdateError::Rejected(
        "Update request exceeded the redirect limit",
    ))
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), UpdateError> {
    if cancelled.load(Ordering::Acquire) {
        Err(UpdateError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_request_url(
    url: &str,
    target: RequestTarget<'_>,
) -> Result<ureq::http::Uri, UpdateError> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| UpdateError::Rejected("Update URL is invalid"))?;
    if uri.scheme_str() != Some("https") || uri.port_u16().is_some() {
        return Err(UpdateError::Rejected(
            "Update request requires an approved HTTPS URL",
        ));
    }
    let approved = match uri.host() {
        Some("github.com") if uri.query().is_none() => match target {
            RequestTarget::Metadata => metadata_path_is_approved(uri.path()),
            RequestTarget::Installer(version) => validate_installer_url(url, version).is_ok(),
        },
        Some("release-assets.githubusercontent.com") => {
            uri.path().starts_with("/github-production-release-asset/")
        }
        _ => false,
    };
    if approved {
        Ok(uri)
    } else {
        Err(UpdateError::Rejected("Update request URL is not approved"))
    }
}

fn metadata_path_is_approved(path: &str) -> bool {
    if matches!(
        path,
        "/Woffluon/BarePDF/releases/latest/download/latest.json"
            | "/Woffluon/BarePDF/releases/latest/download/latest.json.sig"
    ) {
        return true;
    }
    let Some(remainder) = path.strip_prefix("/Woffluon/BarePDF/releases/download/v") else {
        return false;
    };
    let Some((version, filename)) = remainder.split_once('/') else {
        return false;
    };
    matches!(filename, "latest.json" | "latest.json.sig")
        && Version::parse(version).is_ok_and(|version| version.pre.is_empty())
}

fn download_update(
    agent: &Agent,
    update: &VerifiedUpdate,
    cancelled: &AtomicBool,
) -> Result<PathBuf, UpdateError> {
    validate_installer_url(&update.installer_url, &update.version)?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or(UpdateError::Rejected("LOCALAPPDATA is unavailable"))?;
    let directory = PathBuf::from(local_app_data)
        .join("BarePDF")
        .join("Updates");
    fs::create_dir_all(&directory).map_err(|source| UpdateError::Io {
        operation: "Could not create update directory",
        source,
    })?;
    let final_path = directory.join(format!("BarePDF-Setup-x64-v{}.exe", update.version));
    let (partial_path, partial_file) = create_unique_partial(&directory, &update.version)?;

    let result = download_to_partial(agent, update, partial_file, cancelled).and_then(|()| {
        check_cancelled(cancelled)?;
        verify_download(&partial_path, update)?;
        check_cancelled(cancelled)?;
        if final_path.is_file() {
            fs::remove_file(&final_path).map_err(|source| UpdateError::Io {
                operation: "Could not replace previous update",
                source,
            })?;
        }
        fs::rename(&partial_path, &final_path).map_err(|source| UpdateError::Io {
            operation: "Could not finalize update",
            source,
        })?;
        Ok(final_path.clone())
    });
    if result.is_err() {
        let _ = fs::remove_file(partial_path);
    }
    result
}

fn create_unique_partial(directory: &Path, version: &str) -> Result<(PathBuf, File), UpdateError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| UpdateError::Rejected("System clock is before the Unix epoch"))?
        .as_nanos();
    for attempt in 0..16_u8 {
        let path = directory.join(format!(
            ".BarePDF-Setup-x64-v{version}-{}-{nonce}-{attempt}.partial",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(UpdateError::Io {
                    operation: "Could not create update file",
                    source,
                });
            }
        }
    }
    Err(UpdateError::Rejected(
        "Could not allocate a unique update file",
    ))
}

fn download_to_partial(
    agent: &Agent,
    update: &VerifiedUpdate,
    mut file: File,
    cancelled: &AtomicBool,
) -> Result<(), UpdateError> {
    let mut response = get_with_redirects(
        agent,
        &update.installer_url,
        RequestTarget::Installer(&update.version),
        INSTALLER_TIMEOUT,
        cancelled,
    )?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(update.installer_size.saturating_add(1))
        .reader();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let read = reader.read(&mut buffer).map_err(|source| UpdateError::Io {
            operation: "Could not download update",
            source,
        })?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or(UpdateError::Rejected("Installer size overflow"))?;
        if downloaded > update.installer_size {
            return Err(UpdateError::Rejected(
                "Downloaded installer exceeds the declared size",
            ));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|source| UpdateError::Io {
                operation: "Could not write update file",
                source,
            })?;
    }
    check_cancelled(cancelled)?;
    file.sync_all().map_err(|source| UpdateError::Io {
        operation: "Could not flush update file",
        source,
    })?;
    if downloaded != update.installer_size {
        return Err(UpdateError::Rejected(
            "Downloaded installer size does not match metadata",
        ));
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != update.installer_sha256 {
        return Err(UpdateError::Rejected(
            "Downloaded installer SHA-256 does not match metadata",
        ));
    }
    Ok(())
}

fn verify_download(path: &Path, update: &VerifiedUpdate) -> Result<(), UpdateError> {
    let metadata = fs::metadata(path).map_err(|source| UpdateError::Io {
        operation: "Could not inspect downloaded update",
        source,
    })?;
    if metadata.len() != update.installer_size {
        return Err(UpdateError::Rejected(
            "Downloaded installer size changed after verification",
        ));
    }
    let mut file = File::open(path).map_err(|source| UpdateError::Io {
        operation: "Could not reopen downloaded update",
        source,
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|source| UpdateError::Io {
        operation: "Could not rehash downloaded update",
        source,
    })?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != update.installer_sha256 {
        return Err(UpdateError::Rejected(
            "Downloaded installer changed after verification",
        ));
    }
    let actual_version = executable_file_version(path).map_err(|source| UpdateError::Platform {
        operation: "Could not read downloaded update version",
        source,
    })?;
    let expected_version = Version::parse(&update.version)
        .map_err(|source| UpdateError::Version {
            operation: "Invalid release version",
            source,
        })
        .and_then(|version| expected_file_version(&version))?;
    if actual_version != expected_version {
        return Err(UpdateError::Rejected(
            "Downloaded installer version does not match update metadata",
        ));
    }
    Ok(())
}

fn expected_file_version(version: &Version) -> Result<[u16; 4], UpdateError> {
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(UpdateError::Rejected(
            "Windows installer version must be a stable semantic version",
        ));
    }
    Ok([
        u16::try_from(version.major)
            .map_err(|_| UpdateError::Rejected("Release major version exceeds Windows limits"))?,
        u16::try_from(version.minor)
            .map_err(|_| UpdateError::Rejected("Release minor version exceeds Windows limits"))?,
        u16::try_from(version.patch)
            .map_err(|_| UpdateError::Rejected("Release patch version exceeds Windows limits"))?,
        0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fmt::Write as _;

    fn manifest(version: &str, url_version: &str, hash: &str, size: u64) -> String {
        format!(
            r#"{{
                "schemaVersion":1,
                "version":"{version}",
                "releaseUrl":"https://github.com/Woffluon/BarePDF/releases/tag/v{url_version}",
                "releaseNotes":"Security fixes",
                "installer":{{
                    "url":"https://github.com/Woffluon/BarePDF/releases/download/v{url_version}/BarePDF-Setup-x64-v{url_version}.exe",
                    "sha256":"{hash}",
                    "size":{size}
                }}
            }}"#
        )
    }

    #[test]
    fn accepts_only_newer_matching_stable_release() {
        let hash = "a".repeat(64);
        let parsed = parse_manifest(&manifest("1.2.0", "1.2.0", &hash, 1024), "1.1.0")
            .expect("valid manifest")
            .expect("new update");
        assert_eq!(parsed.version(), "1.2.0");
        assert_eq!(
            parsed.release_url(),
            "https://github.com/Woffluon/BarePDF/releases/tag/v1.2.0"
        );

        assert!(
            parse_manifest(&manifest("1.1.0", "1.1.0", &hash, 1024), "1.1.0")
                .expect("same version is not an error")
                .is_none()
        );
        assert!(parse_manifest(
            &manifest("1.2.0-beta.1", "1.2.0-beta.1", &hash, 1024),
            "1.1.0"
        )
        .is_err());
    }

    #[test]
    fn updater_errors_preserve_their_source() {
        let error = UpdateError::Platform {
            operation: "Could not inspect update",
            source: barepdf_platform::PlatformError::InvalidData {
                operation: "Could not read Windows file version metadata",
                reason: "metadata is truncated",
            },
        };

        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn worker_shutdown_is_idempotent() {
        let (mut worker, events) = start_worker();
        drop(events);

        worker.shutdown().expect("idle worker shuts down");
        worker.shutdown().expect("second shutdown is a no-op");
    }

    #[test]
    fn worker_shutdown_retains_active_work_and_joins_it_on_retry() {
        let (commands, _command_receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (release_sender, release_receiver) = mpsc::channel();
        let (finished_sender, finished_receiver) = mpsc::channel();
        let worker_thread = std::thread::spawn(move || {
            let _ = release_receiver.recv();
            let _ = finished_sender.send(());
        });
        let mut worker = UpdateWorker {
            commands,
            cancelled,
            worker: Some(worker_thread),
        };

        let started = Instant::now();
        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::TimedOut)
        ));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(worker.worker.is_some());
        release_sender
            .send(())
            .expect("active test work can be released");
        finished_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("test worker reaches its completion point");
        worker
            .shutdown()
            .expect("finished worker joins on the next shutdown call");
        assert!(worker.worker.is_none());
    }

    #[test]
    fn worker_panic_after_timeout_is_reported_on_retry() {
        let (commands, _command_receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (release_sender, release_receiver) = mpsc::channel();
        let worker_thread = std::thread::spawn(move || {
            let _ = release_receiver.recv();
            panic!("update worker test panic");
        });
        let mut worker = UpdateWorker {
            commands,
            cancelled,
            worker: Some(worker_thread),
        };

        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::TimedOut)
        ));
        release_sender
            .send(())
            .expect("panicking test worker can be released");
        let deadline = Instant::now() + Duration::from_secs(1);
        while worker
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            worker.worker.as_ref().is_some_and(JoinHandle::is_finished),
            "panicking test worker did not finish"
        );
        assert!(matches!(
            worker.shutdown(),
            Err(UpdateShutdownError::Panicked)
        ));
        assert!(worker.shutdown().is_ok());
    }

    #[test]
    fn accepts_only_metadata_signed_by_the_pinned_key() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let mut public_key = String::with_capacity(64);
        for byte in signing_key.verifying_key().to_bytes() {
            write!(public_key, "{byte:02x}").expect("writing to String cannot fail");
        }
        let manifest = b"signed update metadata";
        let signature = signing_key.sign(manifest).to_bytes();

        assert!(verify_manifest_signature_with_key(manifest, &signature, &public_key).is_ok());
        assert!(verify_manifest_signature_with_key(b"tampered", &signature, &public_key).is_err());
        assert!(
            verify_manifest_signature_with_key(manifest, &signature[..63], &public_key).is_err()
        );
        assert!(verify_manifest_signature_with_key(manifest, &signature, "bad").is_err());
    }

    #[test]
    fn rejects_mismatched_urls_hashes_and_sizes() {
        let hash = "a".repeat(64);
        assert!(parse_manifest(&manifest("1.2.0", "1.3.0", &hash, 1024), "1.1.0").is_err());
        assert!(parse_manifest(&manifest("1.2.0", "1.2.0", "bad", 1024), "1.1.0").is_err());
        assert!(parse_manifest(&manifest("1.2.0", "1.2.0", &hash, 0), "1.1.0").is_err());
        assert!(parse_manifest(
            &manifest("1.2.0", "1.2.0", &hash, MAX_INSTALLER_BYTES + 1),
            "1.1.0"
        )
        .is_err());
        let wrong_filename = manifest("1.2.0", "1.2.0", &hash, 1024)
            .replace("BarePDF-Setup-x64-v1.2.0.exe", "BarePDF-Setup-x64.exe");
        assert!(parse_manifest(&wrong_filename, "1.1.0").is_err());
        let suffixed_filename = manifest("1.2.0", "1.2.0", &hash, 1024).replace(
            "BarePDF-Setup-x64-v1.2.0.exe",
            "BarePDF-Setup-x64-v1.2.0-old.exe",
        );
        assert!(parse_manifest(&suffixed_filename, "1.1.0").is_err());
        let inserted_directory = manifest("1.2.0", "1.2.0", &hash, 1024).replace(
            "/BarePDF-Setup-x64-v1.2.0.exe",
            "/archive/BarePDF-Setup-x64-v1.2.0.exe",
        );
        assert!(parse_manifest(&inserted_directory, "1.1.0").is_err());
        let suffixed_release = manifest("1.2.0", "1.2.0", &hash, 1024)
            .replace("/releases/tag/v1.2.0\"", "/releases/tag/v1.2.0-old\"");
        assert!(parse_manifest(&suffixed_release, "1.1.0").is_err());
        assert!(parse_manifest(
            &manifest("1.2.0+replay", "1.2.0+replay", &hash, 1024),
            "1.1.0"
        )
        .is_err());
    }

    #[test]
    fn semantic_versions_map_exactly_to_windows_file_versions() {
        assert_eq!(
            expected_file_version(&Version::parse("12.34.56").expect("version"))
                .expect("Windows version"),
            [12, 34, 56, 0]
        );
        assert!(expected_file_version(&Version::parse("65536.0.0").expect("version")).is_err());
        assert!(expected_file_version(&Version::parse("1.2.3+build").expect("version")).is_err());
    }

    #[test]
    fn requests_accept_only_approved_https_hosts_and_paths() {
        assert!(validate_request_url(METADATA_URL, RequestTarget::Metadata).is_ok());
        assert!(validate_request_url(METADATA_SIGNATURE_URL, RequestTarget::Metadata).is_ok());
        assert!(validate_request_url(
            "https://github.com/Woffluon/BarePDF/releases/download/v1.2.0/latest.json",
            RequestTarget::Metadata
        )
        .is_ok());
        assert!(validate_request_url(
            "https://release-assets.githubusercontent.com/github-production-release-asset/123/abc?token=signed",
            RequestTarget::Metadata
        )
        .is_ok());

        for url in [
            "http://github.com/Woffluon/BarePDF/releases/latest/download/latest.json",
            "https://example.com/Woffluon/BarePDF/releases/latest/download/latest.json",
            "https://github.com/Woffluon/Other/releases/latest/download/latest.json",
            "https://release-assets.githubusercontent.com/unapproved/file",
        ] {
            assert!(validate_request_url(url, RequestTarget::Metadata).is_err());
        }
    }

    #[test]
    fn partial_update_files_are_unique_and_created_exclusively() {
        let directory = std::env::temp_dir().join(format!(
            "barepdf-update-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create test directory");

        let (first_path, first_file) =
            create_unique_partial(&directory, "1.2.0").expect("first partial");
        let (second_path, second_file) =
            create_unique_partial(&directory, "1.2.0").expect("second partial");
        assert_ne!(first_path, second_path);
        assert!(first_path.is_file());
        assert!(second_path.is_file());

        drop((first_file, second_file));
        let _ = fs::remove_dir_all(directory);
    }
}
