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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_url_signature() {
        let _ = open_url("https://example.com");
    }
}
