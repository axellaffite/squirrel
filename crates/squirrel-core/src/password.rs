use std::fmt;

/// A candidate password that passed length policy. Never printed by `Debug`.
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
        let len = raw.chars().count();
        if len < Self::MIN_LEN {
            return Err(PasswordError::TooShort);
        }
        if len > Self::MAX_LEN {
            return Err(PasswordError::TooLong);
        }
        Ok(Self(raw.to_owned()))
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
    fn debug_never_prints_the_secret() {
        let p = Password::parse("super secret value").unwrap();
        assert!(!format!("{p:?}").contains("secret"));
    }
}
