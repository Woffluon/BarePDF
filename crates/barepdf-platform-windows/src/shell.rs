use crate::ffi;
use barepdf_platform::PlatformError;

/// Validates that a URL strictly uses http or https scheme and contains no control characters,
/// whitespace, quotes, backslashes, or shell metacharacters.
///
/// # Errors
///
/// Returns [`PlatformError::InvalidData`] if validation fails.
pub fn validate_url(url: &str) -> Result<(), PlatformError> {
    let is_https = url
        .get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("https://"));
    let is_http = url
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("http://"));
    if !is_https && !is_http {
        return Err(PlatformError::InvalidData {
            operation: "Could not open URL",
            reason: "URL must use http or https scheme",
        });
    }

    for ch in url.chars() {
        if ch.is_ascii_control() || ch.is_ascii_whitespace() {
            return Err(PlatformError::InvalidData {
                operation: "Could not open URL",
                reason: "URL contains control characters or whitespace",
            });
        }
        if matches!(ch, '"' | '\'' | '`' | '^' | '<' | '>' | '|' | '\\') {
            return Err(PlatformError::InvalidData {
                operation: "Could not open URL",
                reason: "URL contains disallowed characters or shell metacharacters",
            });
        }
    }

    let scheme_len = if is_https { 8 } else { 7 };
    let rest = &url[scheme_len..];
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') || host.starts_with('@') {
        return Err(PlatformError::InvalidData {
            operation: "Could not open URL",
            reason: "URL host is missing or invalid",
        });
    }

    Ok(())
}

/// Opens a trusted URL in the user's default browser.
///
/// # Errors
///
/// Returns an error when the URL is invalid or Windows cannot open the URL.
pub fn open_url(url: &str) -> Result<(), PlatformError> {
    validate_url(url)?;
    ffi::open_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls_pass_validation() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://localhost:8080/test").is_ok());
        assert!(validate_url("https://barepdf.com/docs?query=test&page=1#intro").is_ok());
    }

    #[test]
    fn test_invalid_schemes_are_rejected() {
        let cases = [
            "file:///C:/Windows/System32/calc.exe",
            "cmd.exe /c dir",
            "powershell -c ls",
            "ftp://example.com",
            "javascript:alert(1)",
            "relative/path/index.html",
            "C:\\path\\to\\file",
            "//network-share/file",
        ];
        for case in cases {
            let res = open_url(case);
            assert!(
                matches!(res, Err(PlatformError::InvalidData { .. })),
                "Expected InvalidData for {case:?}, got {res:?}"
            );
        }
    }

    #[test]
    fn test_empty_host_is_rejected() {
        let cases = [
            "https://",
            "http://",
            "https:///path",
            "https://:8080",
            "https://@host",
        ];
        for case in cases {
            let res = open_url(case);
            assert!(
                matches!(res, Err(PlatformError::InvalidData { .. })),
                "Expected InvalidData for {case:?}, got {res:?}"
            );
        }
    }

    #[test]
    fn test_control_chars_and_whitespace_are_rejected() {
        let cases = [
            "https://example.com/\0evil",
            "https://example.com/\revil",
            "https://example.com/\nevil",
            "https://example.com/\tevil",
            "https://example.com/evil path",
        ];
        for case in cases {
            let res = open_url(case);
            assert!(
                matches!(res, Err(PlatformError::InvalidData { .. })),
                "Expected InvalidData for {case:?}, got {res:?}"
            );
        }
    }

    #[test]
    fn test_shell_metacharacters_are_rejected() {
        let cases = [
            "https://example.com/\"injection",
            "https://example.com/'injection",
            "https://example.com/`injection`",
            "https://example.com/^injection",
            "https://example.com/<injection>",
            "https://example.com/|calc.exe",
            "https://example.com/\\path\\traversal",
        ];
        for case in cases {
            let res = open_url(case);
            assert!(
                matches!(res, Err(PlatformError::InvalidData { .. })),
                "Expected InvalidData for {case:?}, got {res:?}"
            );
        }
    }
}
