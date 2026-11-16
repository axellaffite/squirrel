use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

/// 256-bit random token, base64url without padding (43 chars).
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Tokens are stored only as their SHA-256 digest.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Prefix of device access tokens (Phase 6).
pub const ACCESS_PREFIX: &str = "sqa_";
/// Prefix of device refresh tokens.
pub const REFRESH_PREFIX: &str = "sqr_";
/// Characters of a [`generate_token`] value.
const TOKEN_CHARS: usize = 43;

/// A device token, prefixed so it cannot be mistaken for a session id or for
/// the other kind, and so secret scanners can recognise it. Stored as
/// [`hash_token`] of the whole string, prefix included.
pub struct DeviceToken;

impl DeviceToken {
    /// A new access token: `sqa_` + 43 base64url characters.
    pub fn access() -> String {
        format!("{ACCESS_PREFIX}{}", generate_token())
    }

    /// A new refresh token: `sqr_` + 43 base64url characters.
    pub fn refresh() -> String {
        format!("{REFRESH_PREFIX}{}", generate_token())
    }
}

/// `token` if it is shaped like an access token; checked before any hashing
/// or database lookup.
pub fn parse_access(token: &str) -> Option<&str> {
    parse_prefixed(token, ACCESS_PREFIX)
}

/// `token` if it is shaped like a refresh token.
pub fn parse_refresh(token: &str) -> Option<&str> {
    parse_prefixed(token, REFRESH_PREFIX)
}

fn parse_prefixed<'a>(token: &'a str, prefix: &str) -> Option<&'a str> {
    let body = token.strip_prefix(prefix)?;
    let well_formed = body.len() == TOKEN_CHARS
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    well_formed.then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_tokens_are_prefixed_and_unique() {
        let access = DeviceToken::access();
        let refresh = DeviceToken::refresh();
        assert!(access.starts_with("sqa_"));
        assert!(refresh.starts_with("sqr_"));
        assert_eq!(access.len(), 4 + 43);
        assert_eq!(refresh.len(), 4 + 43);
        assert_ne!(DeviceToken::access(), access);
    }

    #[test]
    fn parse_access_accepts_only_access_tokens() {
        let access = DeviceToken::access();
        assert_eq!(parse_access(&access), Some(access.as_str()));
        assert_eq!(parse_access(&DeviceToken::refresh()), None);
        assert_eq!(parse_access(&generate_token()), None);
        assert_eq!(parse_access(""), None);
        assert_eq!(parse_access("sqa_"), None);
        assert_eq!(parse_access(&format!("{access}x")), None);
        assert_eq!(parse_access(&format!("sqa_{}", "!".repeat(43))), None);
        assert_eq!(parse_access(&"sqa_a".repeat(1000)), None);
    }

    #[test]
    fn parse_refresh_accepts_only_refresh_tokens() {
        let refresh = DeviceToken::refresh();
        assert_eq!(parse_refresh(&refresh), Some(refresh.as_str()));
        assert_eq!(parse_refresh(&DeviceToken::access()), None);
    }

    #[test]
    fn the_stored_hash_covers_the_prefix() {
        let access = DeviceToken::access();
        assert_ne!(hash_token(&access), hash_token(&access[4..]));
        assert_eq!(hash_token(&access), hash_token(&access.clone()));
    }

    #[test]
    fn tokens_are_43_url_safe_chars_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_deterministic_sha256() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_eq!(hash_token("abc").len(), 32);
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }
}
