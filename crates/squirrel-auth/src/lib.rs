//! Cryptographic primitives: password hashing and opaque tokens.

mod password;

pub use password::{HashError, burn_verification_time, hash_password, verify_password};
