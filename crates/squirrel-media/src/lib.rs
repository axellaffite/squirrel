//! Media understanding without I/O policy: EXIF, capture times, decoding,
//! thumbnails, blurhash, PDFs and reverse geocoding. Callers decide where bytes
//! come from and where results go.

pub mod capture;
pub mod exec;
pub mod geo;
pub mod geo_build;
pub mod geo_data;
#[cfg(feature = "heic")]
pub mod heic;
pub mod metadata;
pub mod render;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;
pub mod thumbs;

/// Whether this build decodes HEIC/HEIF/AVIF (cargo feature `heic`).
pub const HEIC_SUPPORTED: bool = cfg!(feature = "heic");
