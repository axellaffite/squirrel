use std::sync::OnceLock;

use argon2::password_hash::{self, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use squirrel_core::{Password, normalize_password};

/// OWASP-recommended Argon2id parameters (19 MiB, 2 iterations, 1 lane).
const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;
const PARALLELISM: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("password hashing failed: {0}")]
    Hash(password_hash::Error),
    #[error("stored password hash is malformed")]
    Malformed,
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, PARALLELISM, None)
        .expect("static Argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// CPU-heavy: call from `spawn_blocking`.
pub fn hash_password(password: &Password) -> Result<String, HashError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.expose().as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(HashError::Hash)
}

/// Verifies a raw candidate; it is NFKC-normalized here, like `Password::parse`.
/// CPU-heavy: call from `spawn_blocking`.
pub fn verify_password(candidate: &str, stored_hash: &str) -> Result<bool, HashError> {
    let parsed = PasswordHash::new(stored_hash).map_err(|_| HashError::Malformed)?;
    let normalized = normalize_password(candidate);
    match argon2().verify_password(normalized.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(password_hash::Error::Password) => Ok(false),
        Err(other) => Err(HashError::Hash(other)),
    }
}

fn dummy_hash() -> &'static str {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    DUMMY_HASH.get_or_init(|| {
        let dummy = Password::parse("timing-equalizer-password").expect("valid dummy password");
        hash_password(&dummy).expect("hashing the dummy password succeeds")
    })
}

/// Precompute the dummy hash used by `burn_verification_time`, so the first
/// unknown-user login is not slower than later ones. Call once at startup
/// (CPU-heavy: from `spawn_blocking`).
pub fn warm_up() {
    dummy_hash();
}

/// Spend the same time as a real verification, so unknown usernames are not
/// distinguishable from wrong passwords by timing. CPU-heavy.
pub fn burn_verification_time(candidate: &str) {
    // verify_password normalizes the candidate, matching the real path's cost.
    let _ = verify_password(candidate, dummy_hash());
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_core::Password;

    fn pw(s: &str) -> Password {
        Password::parse(s).unwrap()
    }

    #[test]
    fn uses_argon2id_with_pinned_parameters() {
        let hash = hash_password(&pw("correct horse battery")).unwrap();
        assert!(
            hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{hash}"
        );
    }

    #[test]
    fn verifies_correct_and_rejects_wrong_password() {
        let hash = hash_password(&pw("correct horse battery")).unwrap();
        assert!(verify_password("correct horse battery", &hash).unwrap());
        assert!(!verify_password("wrong horse battery", &hash).unwrap());
    }

    #[test]
    fn same_password_gets_different_salts() {
        let a = hash_password(&pw("correct horse battery")).unwrap();
        let b = hash_password(&pw("correct horse battery")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_hash_is_an_error() {
        assert!(matches!(
            verify_password("x", "not-a-hash"),
            Err(HashError::Malformed)
        ));
    }

    #[test]
    fn verifies_differently_normalized_candidate() {
        let hash = hash_password(&pw("caf\u{e9}-password1")).unwrap();
        assert!(verify_password("cafe\u{301}-password1", &hash).unwrap());
    }

    #[test]
    fn warm_up_does_not_panic() {
        warm_up();
        burn_verification_time("anything");
    }
}
