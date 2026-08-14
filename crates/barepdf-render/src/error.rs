use barepdf_core::PdfError;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Render worker stopped")]
    WorkerStopped,
    #[error("Render worker did not terminate cleanly")]
    WorkerTerminated,
    #[error(transparent)]
    Pdf(#[from] PdfError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_error_conversion_preserves_the_source_variant() {
        assert!(matches!(
            RenderError::from(PdfError::PasswordRequired),
            RenderError::Pdf(PdfError::PasswordRequired)
        ));
    }
}
