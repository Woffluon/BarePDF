use std::error::Error;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation}: {source}")]
    External {
        operation: &'static str,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("{operation} failed with Windows error code {code}")]
    Windows { operation: &'static str, code: u32 },
    #[error("{operation}: {reason}")]
    InvalidData {
        operation: &'static str,
        reason: &'static str,
    },
    #[error("{operation} is unavailable")]
    Unavailable { operation: &'static str },
}

#[cfg(test)]
mod tests {
    use super::PlatformError;
    use std::error::Error as _;

    #[test]
    fn io_error_preserves_operation_and_source() {
        let error = PlatformError::Io {
            operation: "Could not launch process",
            source: std::io::Error::other("blocked"),
        };

        assert_eq!(error.to_string(), "Could not launch process: blocked");
        assert!(error.source().is_some());
    }
}
