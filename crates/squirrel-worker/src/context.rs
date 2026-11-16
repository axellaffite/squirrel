//! What every media handler needs.

use std::path::PathBuf;

use sqlx::PgPool;
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;

use crate::exec::Executor;

pub struct MediaContext {
    pub pool: PgPool,
    pub storage: BlobStore,
    pub executor: Executor,
    /// Directory holding libpdfium; `None`: PDFs are `unsupported` (Milestone E).
    pub pdfium_dir: Option<PathBuf>,
    pub decode_limits: DecodeLimits,
}
