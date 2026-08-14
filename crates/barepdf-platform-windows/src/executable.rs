use crate::ffi;
use barepdf_platform::PlatformError;
use std::path::Path;
use std::process::Command;

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

/// Starts a previously verified installer without silent-install arguments.
///
/// # Errors
///
/// Returns an error when Windows cannot start the installer.
pub fn launch_installer(path: &Path) -> Result<(), PlatformError> {
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
    use super::fixed_file_version;

    #[test]
    fn fixed_windows_version_words_are_decoded_in_order() {
        assert_eq!(
            fixed_file_version(0x000C_0022, 0x0038_0000),
            [12, 34, 56, 0]
        );
    }
}
