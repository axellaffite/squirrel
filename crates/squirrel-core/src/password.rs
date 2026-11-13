use std::fmt;

use unicode_normalization::UnicodeNormalization;

/// Normalize a raw password to NFKC so visually identical inputs typed on
/// different platforms (e.g. composed vs. decomposed accents) hash the same.
/// Apply it to every password before hashing or verifying.
pub fn normalize_password(raw: &str) -> String {
    raw.nfkc().collect()
}

/// A NFKC-normalized password that passed length policy. Never printed by `Debug`.
#[derive(Clone)]
pub struct Password(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password must be at least {} characters", Password::MIN_LEN)]
    TooShort,
    #[error("password must be at most {} characters", Password::MAX_LEN)]
    TooLong,
}

impl Password {
    pub const MIN_LEN: usize = 10;
    /// Upper bound protects the server from hashing huge inputs.
    pub const MAX_LEN: usize = 256;

    pub fn parse(raw: &str) -> Result<Self, PasswordError> {
        let normalized = normalize_password(raw);
        let len = normalized.chars().count();
        if len < Self::MIN_LEN {
            return Err(PasswordError::TooShort);
        }
        if len > Self::MAX_LEN {
            return Err(PasswordError::TooLong);
        }
        Ok(Self(normalized))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_length_bounds() {
        assert_eq!(
            Password::parse("short").unwrap_err(),
            PasswordError::TooShort
        );
        assert_eq!(
            Password::parse(&"x".repeat(257)).unwrap_err(),
            PasswordError::TooLong
        );
        assert!(Password::parse("long enough!").is_ok());
    }

    #[test]
    fn length_boundaries() {
        assert_eq!(
            Password::parse(&"x".repeat(9)).unwrap_err(),
            PasswordError::TooShort
        );
        assert!(Password::parse(&"x".repeat(10)).is_ok());
        assert!(Password::parse(&"x".repeat(256)).is_ok());
        assert!(Password::parse(&"é".repeat(10)).is_ok());
    }

    #[test]
    fn normalizes_to_nfkc() {
        let nfd = Password::parse("cafe\u{301}-password1").unwrap();
        assert_eq!(nfd.expose(), "café-password1");
        assert_eq!(normalize_password("\u{FB01}"), "fi");
    }

    #[test]
    fn counts_length_after_normalization() {
        // 9 base letters + 9 combining accents compose to 9 chars: too short.
        let decomposed = "e\u{301}".repeat(9);
        assert_eq!(
            Password::parse(&decomposed).unwrap_err(),
            PasswordError::TooShort
        );
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let p = Password::parse("super secret value").unwrap();
        assert!(!format!("{p:?}").contains("secret"));
    }
}
