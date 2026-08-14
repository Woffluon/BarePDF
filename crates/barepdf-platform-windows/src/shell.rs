use crate::ffi;
use barepdf_platform::PlatformError;

/// Opens a trusted URL in the user's default browser.
///
/// # Errors
///
/// Returns an error when Windows cannot open the URL.
pub fn open_url(url: &str) -> Result<(), PlatformError> {
    ffi::open_url(url)
}
