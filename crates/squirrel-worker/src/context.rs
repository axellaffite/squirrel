//! What every media handler needs.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use sqlx::PgPool;
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;

use crate::exec::Executor;
use crate::geocode::GeoCache;

/// Why a PDF step was skipped when no pdfium is configured. Backfill looks
/// for it (`media.last_error`) to queue those steps again once one is.
pub const NO_PDFIUM: &str = "pdfium is not configured";

/// "pdfium is not configured" is worth one warning per process, not one per PDF.
static WARNED_NO_PDFIUM: AtomicBool = AtomicBool::new(false);

pub struct MediaContext {
    pub pool: PgPool,
    pub storage: BlobStore,
    pub executor: Executor,
    /// Directory holding libpdfium; `None`: PDFs are `unsupported`.
    pub pdfium_dir: Option<PathBuf>,
    pub decode_limits: DecodeLimits,
    /// The reverse geocoder's dataset.
    pub geo: GeoCache,
}

impl MediaContext {
    /// The pdfium directory, or [`NO_PDFIUM`] (warned about once).
    pub fn pdfium(&self) -> Result<&PathBuf, &'static str> {
        self.pdfium_dir.as_ref().ok_or_else(|| {
            if !WARNED_NO_PDFIUM.swap(true, Ordering::Relaxed) {
                tracing::warn!("PDF thumbnails and text are disabled: set SQUIRREL_PDFIUM_DIR");
            }
            NO_PDFIUM
        })
    }
}
