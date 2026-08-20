use super::manifest::{expected_file_version, validate_installer_url, VerifiedUpdate};
use super::transport::{check_cancelled, get_with_redirects, RequestTarget};
use super::UpdateFailure;
use barepdf_platform_windows::executable_file_version;
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ureq::Agent;

const INSTALLER_TIMEOUT: Duration = Duration::from_mins(30);

pub(super) fn download_update(
    agent: &Agent,
    update: &VerifiedUpdate,
    cancelled: &AtomicBool,
) -> Result<PathBuf, UpdateFailure> {
    validate_installer_url(&update.installer_url, update.version())?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or(UpdateFailure::Rejected("LOCALAPPDATA is unavailable"))?;
    let directory = PathBuf::from(local_app_data)
        .join("BarePDF")
        .join("Updates");
    fs::create_dir_all(&directory).map_err(|source| UpdateFailure::Io {
        operation: "Could not create update directory",
        source,
    })?;
    let final_path = directory.join(format!("BarePDF-Setup-x64-v{}.exe", update.version()));
    let (partial_path, partial_file) = create_unique_partial(&directory, update.version())?;
    let result = download_to_partial(agent, update, partial_file, cancelled).and_then(|()| {
        check_cancelled(cancelled)?;
        verify_download(&partial_path, update)?;
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
        Ok(final_path.clone())
    });
    finish_download(result, &partial_path)
}

pub(super) fn verify_download(path: &Path, update: &VerifiedUpdate) -> Result<(), UpdateFailure> {
    verify_file_integrity(path, update.installer_size, &update.installer_sha256)?;
    let actual_version =
        executable_file_version(path).map_err(|source| UpdateFailure::Platform {
            operation: "Could not read downloaded update version",
            source,
        })?;
    let expected_version = Version::parse(update.version())
        .map_err(|source| UpdateFailure::Version {
            operation: "Invalid release version",
            source,
        })
        .and_then(|version| expected_file_version(&version))?;
    if actual_version != expected_version {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer version does not match update metadata",
        ));
    }
    Ok(())
}

fn verify_file_integrity(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateFailure> {
    let metadata = fs::metadata(path).map_err(|source| UpdateFailure::Io {
        operation: "Could not inspect downloaded update",
        source,
    })?;
    validate_download_size(metadata.len(), expected_size)?;
    let mut file = File::open(path).map_err(|source| UpdateFailure::Io {
        operation: "Could not reopen downloaded update",
        source,
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|source| UpdateFailure::Io {
        operation: "Could not rehash downloaded update",
        source,
    })?;
    if format!("{:x}", hasher.finalize()) != expected_sha256 {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer changed after verification",
        ));
    }
    Ok(())
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
        let path = directory.join(format!(
            ".BarePDF-Setup-x64-v{version}-{}-{nonce}-{attempt}.partial",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
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

fn download_to_partial(
    agent: &Agent,
    update: &VerifiedUpdate,
    mut file: File,
    cancelled: &AtomicBool,
) -> Result<(), UpdateFailure> {
    let mut response = get_with_redirects(
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
    }
    check_cancelled(cancelled)?;
    file.sync_all().map_err(|source| UpdateFailure::Io {
        operation: "Could not flush update file",
        source,
    })?;
    validate_download_size(downloaded, update.installer_size)?;
    if format!("{:x}", hasher.finalize()) != update.installer_sha256 {
        return Err(UpdateFailure::Rejected(
            "Downloaded installer SHA-256 does not match metadata",
        ));
    }
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
        format!("{:x}", Sha256::digest(bytes))
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
}
