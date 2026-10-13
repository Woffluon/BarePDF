use barepdf_platform::PlatformError;
use std::process::Command;

pub fn open_url(url: &str) -> Result<(), PlatformError> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|source| PlatformError::Io {
            operation: "xdg-open",
            source,
        })
}

pub fn launch_package_installer(path: &std::path::Path) -> Result<(), PlatformError> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|source| PlatformError::Io {
            operation: "launch_package_installer",
            source,
        })
}

pub fn trigger_virtual_keyboard() {
    let _ = Command::new("onboard")
        .spawn()
        .or_else(|_| Command::new("maliit-keyboard").spawn())
        .or_else(|_| {
            Command::new("dbus-send")
                .args([
                    "--type=method_call",
                    "--dest=org.onboard.Onboard",
                    "/org/onboard/Onboard/Keyboard",
                    "org.onboard.Onboard.Keyboard.ToggleVisible",
                ])
                .spawn()
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_url_signature() {
        let _ = open_url("https://example.com");
    }

    #[test]
    fn test_launch_package_installer_signature() {
        let _ = launch_package_installer(std::path::Path::new("/tmp/test.deb"));
    }
}
