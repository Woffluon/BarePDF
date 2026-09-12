use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Memory-safe, redactable password wrapper protected against LLVM Dead Store Elimination.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretPassword {
    bytes: Vec<u8>,
}

impl SecretPassword {
    /// Creates a new secret password from an owned string.
    #[must_use]
    pub fn new(password: String) -> Self {
        Self {
            bytes: password.into_bytes(),
        }
    }

    /// Exposes the password as a string slice without unnecessary heap allocation.
    #[must_use]
    pub fn expose(&self) -> &str {
        std::str::from_utf8(&self.bytes).unwrap_or_default()
    }

    /// Explicitly zeroizes and clears the password buffer.
    pub fn clear(&mut self) {
        self.bytes.zeroize();
        self.bytes.clear();
    }

    /// Returns true if the password contains no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the raw internal bytes for unit tests.
    #[doc(hidden)]
    #[must_use]
    pub fn bytes_for_test(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SecretPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretPassword([REDACTED])")
    }
}

impl PartialEq for SecretPassword {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for SecretPassword {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_is_redacted_in_debug_output() {
        let password = SecretPassword::new("sensitive_pass_123".to_string());
        assert_eq!(format!("{password:?}"), "SecretPassword([REDACTED])");
    }

    #[test]
    fn password_expose_returns_original_string() {
        let password = SecretPassword::new("secret_value".to_string());
        assert_eq!(password.expose(), "secret_value");
    }

    #[test]
    fn password_clearing_zeroizes_bytes() {
        let mut password = SecretPassword::new("temporary_token".to_string());
        password.clear();
        assert!(password.bytes_for_test().is_empty());
    }
}
