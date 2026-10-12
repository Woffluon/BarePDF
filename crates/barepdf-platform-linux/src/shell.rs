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
}
