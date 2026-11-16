//! Media understanding without I/O policy: EXIF, capture times, decoding,
//! thumbnails, blurhash, PDFs and reverse geocoding. Callers decide where bytes
//! come from and where results go.

pub mod capture;
pub mod metadata;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;
