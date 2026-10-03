use crate::ffi;
use barepdf_platform::PlatformError;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::process::Command;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ};

#[must_use]
pub fn is_installed_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("unins000.exe")))
        .is_some_and(|path| path.is_file())
}

/// Reads the fixed four-part version embedded in a Windows executable.
///
/// # Errors
///
/// Returns an error when the file has no valid fixed PE version resource.
pub fn executable_file_version(path: &Path) -> Result<[u16; 4], PlatformError> {
    let (ms, ls) = ffi::executable_file_version_words(path)?;
    Ok(fixed_file_version(ms, ls))
}

fn fixed_file_version(ms: u32, ls: u32) -> [u16; 4] {
    let ms = ms.to_be_bytes();
    let ls = ls.to_be_bytes();
    [
        u16::from_be_bytes([ms[0], ms[1]]),
        u16::from_be_bytes([ms[2], ms[3]]),
        u16::from_be_bytes([ls[0], ls[1]]),
        u16::from_be_bytes([ls[2], ls[3]]),
    ]
}

fn lock_executable_for_launch(path: &Path) -> Result<File, PlatformError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(|source| PlatformError::Io {
            operation: "Could not lock installer for launch",
            source,
        })?;
    let metadata = file.metadata().map_err(|source| PlatformError::Io {
        operation: "Could not inspect locked installer",
        source,
    })?;
    if !metadata.is_file() || (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(PlatformError::InvalidData {
            operation: "Could not validate locked installer",
            reason: "Installer must be a regular non-reparse file",
        });
    }
    Ok(file)
}

/// Starts a previously verified installer without silent-install arguments.
///
/// # Errors
///
/// Returns an error when Windows cannot start the installer.
pub fn launch_installer(path: &Path) -> Result<(), PlatformError> {
    let _locked_file = lock_executable_for_launch(path)?;
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|source| PlatformError::Io {
            operation: "Could not start installer",
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::{fixed_file_version, lock_executable_for_launch};
    use std::fs::{self, OpenOptions};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn fixed_windows_version_words_are_decoded_in_order() {
        assert_eq!(
            fixed_file_version(0x000C_0022, 0x0038_0000),
            [12, 34, 56, 0]
        );
    }

    #[test]
    fn executable_launch_lock_denies_concurrent_writes_and_rejects_active_writers() {
        let dir = std::env::temp_dir().join(format!(
            "barepdf-exe-lock-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        assert!(fs::create_dir_all(&dir).is_ok());
        let path = dir.join("setup.exe");
        assert!(fs::write(&path, b"MZ-verified-payload").is_ok());

        let lock = lock_executable_for_launch(&path).expect("read-share lock succeeds");
        assert!(
            OpenOptions::new().write(true).open(&path).is_err(),
            "concurrent write must be denied while launch lock is held"
        );
        assert!(
            fs::remove_file(&path).is_err(),
            "concurrent delete must be denied while launch lock is held"
        );
        drop(lock);

        let writer = OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("writer opens after lock release");
        assert!(
            lock_executable_for_launch(&path).is_err(),
            "launch lock must reject file with active writer handle"
        );
        drop(writer);
        let _ = fs::remove_dir_all(&dir);
    }
}
