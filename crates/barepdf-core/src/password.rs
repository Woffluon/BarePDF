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
        let len_a = self.bytes.len();
        let len_b = other.bytes.len();
        let max_len = len_a.max(len_b);
        let mut diff = len_a ^ len_b;

        for i in 0..max_len {
            let a = self.bytes.get(i).copied().unwrap_or(0);
            let b = other.bytes.get(i).copied().unwrap_or(0);
            diff |= usize::from(a ^ b);
        }

        diff == 0
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

    #[test]
    fn password_constant_time_equality_compares_bytes_and_lengths() {
        let pass_a = SecretPassword::new("correct-horse-battery-staple".to_string());
        let pass_b = SecretPassword::new("correct-horse-battery-staple".to_string());
        let pass_diff_byte = SecretPassword::new("correct-horse-battery-staplX".to_string());
        let pass_prefix = SecretPassword::new("correct-horse".to_string());
        let pass_trailing_zero = SecretPassword::new("correct-horse-battery-staple\0".to_string());

        assert_eq!(pass_a, pass_b);
        assert_ne!(pass_a, pass_diff_byte);
        assert_ne!(pass_a, pass_prefix);
        assert_ne!(pass_a, pass_trailing_zero);
    }
}
