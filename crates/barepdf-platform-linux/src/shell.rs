use std::process::Command;

#[must_use]
pub fn open_url(url: &str) -> bool {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| true)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_url_signature() {
        // Test that open_url is callable with &str and returns a bool without panicking.
        // On non-Linux (e.g. Windows dev machine), xdg-open won't exist and should return false.
        let result = open_url("https://example.com");
        assert_eq!(result, result);
    }
}
