use crate::infrastructure::PreferencesLoadError;
use barepdf_core::PdfError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    Pdf(#[from] PdfError),
    #[error(transparent)]
    Preferences(#[from] PreferencesLoadError),
    #[error(transparent)]
    Slint(#[from] slint::PlatformError),
}
