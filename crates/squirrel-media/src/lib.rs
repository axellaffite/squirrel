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
pub mod pdf;
pub mod preview;
pub mod render;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;
pub mod thumbs;
#[cfg(feature = "vips")]
pub mod vips;

/// Whether this build renders previews with libvips (cargo feature `vips`).
pub const VIPS_COMPILED: bool = cfg!(feature = "vips");

/// Whether this build decodes HEIC/HEIF/AVIF (cargo feature `heic`).
pub const HEIC_SUPPORTED: bool = cfg!(feature = "heic");

/// Registers the optional decoders compiled into this build (HEIC/HEIF/AVIF with `heic`).
/// Call it before decoding, in whatever process decodes: the sandbox child, or the worker
/// itself in in-process mode. Runs once per process; a no-op in default builds.
pub fn register_decoders() {
    #[cfg(feature = "heic")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(heic::register);
    }
}
