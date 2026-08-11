use barepdf_platform_windows::{
    executable_file_version, launch_installer, normalize_signer_fingerprint,
    verify_authenticode_signer,
};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const METADATA_URL: &str =
    "https://github.com/Woffluon/BarePDF/releases/latest/download/latest.json";
pub const AUTO_CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALLER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const USER_AGENT: &str = concat!("BarePDF/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub release_url: String,
    pub release_notes: String,
    pub installer_url: String,
    pub installer_sha256: String,
    pub installer_size: u64,
}

#[derive(Debug)]
pub enum UpdateCommand {
    Check,
    Download(UpdateInfo),
    Install { path: PathBuf, update: UpdateInfo },
}

#[derive(Debug)]
pub enum UpdateEvent {
    UpToDate,
    Available(UpdateInfo),
    Downloaded { path: PathBuf, update: UpdateInfo },
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

pub fn start_worker() -> (Sender<UpdateCommand>, Receiver<UpdateEvent>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let (event_sender, event_receiver) = mpsc::channel();
    std::thread::spawn(move || run_worker(command_receiver, &event_sender));
    (command_sender, event_receiver)
}

#[must_use]
pub fn signer_is_configured() -> bool {
    expected_signer().is_ok()
}

fn run_worker(commands: Receiver<UpdateCommand>, events: &Sender<UpdateEvent>) {
    let agent = Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();

    while let Ok(command) = commands.recv() {
        let event = match command {
            UpdateCommand::Check => match check_for_update(&agent) {
                Ok(Some(update)) => UpdateEvent::Available(update),
                Ok(None) => UpdateEvent::UpToDate,
                Err(error) => UpdateEvent::Error(error),
            },
            UpdateCommand::Download(update) => match download_update(&agent, &update) {
                Ok(path) => UpdateEvent::Downloaded { path, update },
                Err(error) => UpdateEvent::Error(error),
            },
            UpdateCommand::Install { path, update } => {
                match verify_download(&path, &update)
                    .and_then(|()| launch_installer(&path).map_err(|error| error.to_string()))
                {
                    Ok(()) => UpdateEvent::InstallerStarted,
                    Err(error) => UpdateEvent::Error(error),
                }
            }
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

fn check_for_update(agent: &Agent) -> Result<Option<UpdateInfo>, String> {
    let mut response = get_with_redirects(
        agent,
        METADATA_URL,
        RequestTarget::Metadata,
        METADATA_TIMEOUT,
    )?;
    let json = response
        .body_mut()
        .with_config()
        .limit(MAX_METADATA_BYTES)
        .read_to_string()
        .map_err(|error| format!("Update metadata could not be read: {error}"))?;
    parse_manifest(&json, CURRENT_VERSION)
}

fn parse_manifest(json: &str, current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let manifest: UpdateManifest =
        serde_json::from_str(json).map_err(|error| format!("Invalid update metadata: {error}"))?;
    if manifest.schema_version != 1 {
        return Err("Unsupported update metadata version".into());
    }

    let current = Version::parse(current_version)
        .map_err(|error| format!("Invalid application version: {error}"))?;
    let available = Version::parse(&manifest.version)
        .map_err(|error| format!("Invalid release version: {error}"))?;
    if !available.pre.is_empty() || !available.build.is_empty() {
        return Err("Preview releases are not accepted on the stable channel".into());
    }
    expected_file_version(&available)?;
    if available <= current {
        return Ok(None);
    }

    validate_release_url(&manifest.release_url, &manifest.version)?;
    validate_installer_url(&manifest.installer.url, &manifest.version)?;
    let hash = manifest.installer.sha256.to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid installer SHA-256".into());
    }
    if manifest.installer.size == 0 || manifest.installer.size > MAX_INSTALLER_BYTES {
        return Err("Installer size is outside the allowed range".into());
    }

    Ok(Some(UpdateInfo {
        version: manifest.version,
        release_url: manifest.release_url,
        release_notes: manifest.release_notes,
        installer_url: manifest.installer.url,
        installer_sha256: hash,
        installer_size: manifest.installer.size,
    }))
}

fn validate_release_url(url: &str, version: &str) -> Result<(), String> {
    let expected = format!("/Woffluon/BarePDF/releases/tag/v{version}");
    validate_github_url(url, &expected)
}

fn validate_installer_url(url: &str, version: &str) -> Result<(), String> {
    let expected =
        format!("/Woffluon/BarePDF/releases/download/v{version}/BarePDF-Setup-x64-v{version}.exe");
    validate_github_url(url, &expected)
}

fn validate_github_url(url: &str, expected_path: &str) -> Result<(), String> {
    let uri: ureq::http::Uri = url.parse().map_err(|_| "Invalid HTTPS URL".to_string())?;
    if uri.scheme_str() != Some("https")
        || uri.host() != Some("github.com")
        || uri.port_u16().is_some()
        || uri.query().is_some()
        || uri.path() != expected_path
    {
        return Err("Update URL is not an approved BarePDF release URL".into());
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
) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut url = initial_url.to_string();
    let started = Instant::now();
    for redirect_count in 0..=MAX_REDIRECTS {
        let uri = validate_request_url(&url, target)?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| "Update request timed out".to_string())?;
        let response = agent
            .get(uri)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(remaining))
            .build()
            .call()
            .map_err(|error| format!("Update request failed: {error}"))?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Err("Update request returned an unsupported redirect".into());
        }
        if redirect_count == MAX_REDIRECTS {
            return Err("Update request exceeded the redirect limit".into());
        }
        let location = response
            .headers()
            .get(ureq::http::header::LOCATION)
            .ok_or_else(|| "Update redirect is missing Location".to_string())?
            .to_str()
            .map_err(|_| "Update redirect Location is invalid".to_string())?;
        validate_request_url(location, target)?;
        url.clear();
        url.push_str(location);
    }
    unreachable!("bounded redirect loop always returns")
}

fn validate_request_url(url: &str, target: RequestTarget<'_>) -> Result<ureq::http::Uri, String> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| "Update URL is invalid".to_string())?;
    if uri.scheme_str() != Some("https") || uri.port_u16().is_some() {
        return Err("Update request requires an approved HTTPS URL".into());
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
        Err("Update request URL is not approved".into())
    }
}

fn metadata_path_is_approved(path: &str) -> bool {
    if path == "/Woffluon/BarePDF/releases/latest/download/latest.json" {
        return true;
    }
    let Some(remainder) = path.strip_prefix("/Woffluon/BarePDF/releases/download/v") else {
        return false;
    };
    let Some((version, filename)) = remainder.split_once('/') else {
        return false;
    };
    filename == "latest.json" && Version::parse(version).is_ok_and(|version| version.pre.is_empty())
}

fn download_update(agent: &Agent, update: &UpdateInfo) -> Result<PathBuf, String> {
    validate_installer_url(&update.installer_url, &update.version)?;
    let signer = expected_signer()?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is unavailable".to_string())?;
    let directory = PathBuf::from(local_app_data)
        .join("BarePDF")
        .join("Updates");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create update directory: {error}"))?;
    let final_path = directory.join(format!("BarePDF-Setup-x64-v{}.exe", update.version));
    let (partial_path, partial_file) = create_unique_partial(&directory, &update.version)?;

    let result = download_to_partial(agent, update, partial_file).and_then(|()| {
        verify_download_with_signer(&partial_path, update, &signer)?;
        if final_path.is_file() {
            fs::remove_file(&final_path)
                .map_err(|error| format!("Could not replace previous update: {error}"))?;
        }
        fs::rename(&partial_path, &final_path)
            .map_err(|error| format!("Could not finalize update: {error}"))?;
        Ok(final_path.clone())
    });
    if result.is_err() {
        let _ = fs::remove_file(partial_path);
    }
    result
}

fn create_unique_partial(directory: &Path, version: &str) -> Result<(PathBuf, File), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before the Unix epoch".to_string())?
        .as_nanos();
    for attempt in 0..16_u8 {
        let path = directory.join(format!(
            ".BarePDF-Setup-x64-v{version}-{}-{nonce}-{attempt}.partial",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Could not create update file: {error}")),
        }
    }
    Err("Could not allocate a unique update file".into())
}

fn download_to_partial(agent: &Agent, update: &UpdateInfo, mut file: File) -> Result<(), String> {
    let mut response = get_with_redirects(
        agent,
        &update.installer_url,
        RequestTarget::Installer(&update.version),
        INSTALLER_TIMEOUT,
    )?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(update.installer_size.saturating_add(1))
        .reader();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Could not download update: {error}"))?;
        if read == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(read as u64)
            .ok_or_else(|| "Installer size overflow".to_string())?;
        if downloaded > update.installer_size {
            return Err("Downloaded installer exceeds the declared size".into());
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| format!("Could not write update file: {error}"))?;
    }
    file.sync_all()
        .map_err(|error| format!("Could not flush update file: {error}"))?;
    if downloaded != update.installer_size {
        return Err("Downloaded installer size does not match metadata".into());
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != update.installer_sha256 {
        return Err("Downloaded installer SHA-256 does not match metadata".into());
    }
    Ok(())
}

fn verify_download(path: &Path, update: &UpdateInfo) -> Result<(), String> {
    let signer = expected_signer()?;
    verify_download_with_signer(path, update, &signer)
}

fn verify_download_with_signer(
    path: &Path,
    update: &UpdateInfo,
    signer: &str,
) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect downloaded update: {error}"))?;
    if metadata.len() != update.installer_size {
        return Err("Downloaded installer size changed after verification".into());
    }
    let mut file =
        File::open(path).map_err(|error| format!("Could not reopen downloaded update: {error}"))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("Could not rehash downloaded update: {error}"))?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != update.installer_sha256 {
        return Err("Downloaded installer changed after verification".into());
    }
    verify_authenticode_signer(path, signer).map_err(|error| error.to_string())?;
    let actual_version = executable_file_version(path).map_err(|error| error.to_string())?;
    let expected_version = Version::parse(&update.version)
        .map_err(|error| format!("Invalid release version: {error}"))
        .and_then(|version| expected_file_version(&version))?;
    if actual_version != expected_version {
        return Err("Downloaded installer version does not match update metadata".into());
    }
    Ok(())
}

fn expected_file_version(version: &Version) -> Result<[u16; 4], String> {
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("Windows installer version must be a stable semantic version".into());
    }
    Ok([
        u16::try_from(version.major)
            .map_err(|_| "Release major version exceeds Windows limits".to_string())?,
        u16::try_from(version.minor)
            .map_err(|_| "Release minor version exceeds Windows limits".to_string())?,
        u16::try_from(version.patch)
            .map_err(|_| "Release patch version exceeds Windows limits".to_string())?,
        0,
    ])
}

fn expected_signer() -> Result<String, String> {
    normalize_signer_fingerprint(option_env!("BAREPDF_UPDATE_SIGNER_SHA256").unwrap_or_default())
        .ok_or_else(|| {
            "Secure updater is unavailable because the publisher identity is not configured".into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(parsed.version, "1.2.0");

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
