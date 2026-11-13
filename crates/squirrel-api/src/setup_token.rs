use std::fmt;

use squirrel_auth::hash_token;
use subtle::ConstantTimeEq;

/// Secret proving the person creating the first administrator can read the
/// server's configuration or log. Never printed through `Debug`.
#[derive(Clone)]
pub struct SetupToken(String);

impl SetupToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// Constant-time comparison. Both sides are hashed first, so neither the
    /// content nor the length of the expected token leaks through timing.
    pub fn matches(&self, candidate: &str) -> bool {
        bool::from(hash_token(candidate).ct_eq(&hash_token(&self.0)))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SetupToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SetupToken(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_exact_token() {
        let token = SetupToken::new("abc-123");
        assert!(token.matches("abc-123"));
        assert!(!token.matches("abc-12"));
        assert!(!token.matches("abc-1234"));
        assert!(!token.matches(""));
    }

    #[test]
    fn debug_does_not_leak_the_token() {
        let token = SetupToken::new("super-secret");
        assert!(!format!("{token:?}").contains("super-secret"));
    }
}
