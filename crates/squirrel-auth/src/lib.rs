//! Cryptographic primitives: password hashing and opaque tokens.

mod password;
mod token;

pub use password::{HashError, burn_verification_time, hash_password, verify_password};
pub use token::{generate_token, hash_token};
