use std::fmt;

/// A SHA-256 digest as 64 lowercase hex characters: the blob identifier.
/// Parsing is strict so a hash can be used safely as a path component.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("sha256 must be 64 lowercase hexadecimal characters")]
pub struct ContentHashError;

impl ContentHash {
    pub const HEX_LEN: usize = 64;

    pub fn parse(raw: &str) -> Result<Self, ContentHashError> {
        let valid = raw.len() == Self::HEX_LEN
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(ContentHashError)
        }
    }

    pub fn from_digest(digest: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let hex = digest
            .iter()
            .flat_map(|b| {
                [
                    HEX[usize::from(b >> 4)] as char,
                    HEX[usize::from(b & 0xf)] as char,
                ]
            })
            .collect();
        Self(hex)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two levels of fan-out directories: `ab/cd` for `abcd…`.
    pub fn shard(&self) -> (&str, &str) {
        (&self.0[0..2], &self.0[2..4])
    }

    /// A stable 64-bit key (the first 8 bytes) for Postgres advisory locks.
    /// The bits are reinterpreted as signed (`bigint`), so high prefixes are
    /// negative; only stability and spread matter.
    pub fn lock_key(&self) -> i64 {
        u64::from_str_radix(&self.0[..16], 16)
            .expect("validated hex")
            .cast_signed()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn accepts_64_lowercase_hex() {
        assert_eq!(ContentHash::parse(EMPTY).unwrap().as_str(), EMPTY);
    }

    #[test]
    fn rejects_uppercase_short_and_path_like_input() {
        assert!(ContentHash::parse(&EMPTY.to_uppercase()).is_err());
        assert!(ContentHash::parse(&EMPTY[..63]).is_err());
        assert!(ContentHash::parse(&format!("../{}", &EMPTY[3..])).is_err());
        assert!(ContentHash::parse(&format!("{EMPTY}0")).is_err());
    }

    #[test]
    fn formats_a_digest_as_lowercase_hex() {
        let mut digest = [0u8; 32];
        digest[0] = 0xab;
        digest[31] = 0x0f;
        let hash = ContentHash::from_digest(digest);
        assert!(hash.as_str().starts_with("ab00"));
        assert!(hash.as_str().ends_with("0f"));
        assert_eq!(ContentHash::parse(hash.as_str()), Ok(hash));
    }

    #[test]
    fn shards_by_the_first_two_bytes() {
        assert_eq!(ContentHash::parse(EMPTY).unwrap().shard(), ("e3", "b0"));
    }

    #[test]
    fn lock_key_is_stable_and_distinct() {
        let a = ContentHash::parse(EMPTY).unwrap();
        let b = ContentHash::parse(&"0".repeat(64)).unwrap();
        assert_eq!(a.lock_key(), a.clone().lock_key());
        assert_ne!(a.lock_key(), b.lock_key());
        let high = ContentHash::parse(&"f".repeat(64)).unwrap();
        assert_eq!(high.lock_key(), -1);
        assert_eq!(a.lock_key(), 0xe3b0_c442_98fc_1c14_u64.cast_signed());
    }
}
