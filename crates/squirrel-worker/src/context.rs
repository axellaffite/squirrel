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
    /// Model inference, when this worker has models (`None`: no ML kinds).
    pub ml: Option<std::sync::Arc<MlContext>>,
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

/// What the ML handlers need: the supervised `ml-exec` child and which model
/// groups it loads.
pub struct MlContext {
    pub client: crate::ml_client::MlClient,
    pub clip: bool,
    pub faces: bool,
    /// The label bank of the pinned CLIP model, read once from the database
    /// (computed at worker start, `label_bank::ensure`). Empty: no tags.
    label_bank: std::sync::OnceLock<std::sync::Arc<LabelBank>>,
}

/// (slug, text embedding) per label, for the current labels version.
pub type LabelBank = Vec<(String, Vec<f32>)>;

impl MlContext {
    pub fn new(client: crate::ml_client::MlClient, clip: bool, faces: bool) -> Self {
        Self {
            client,
            clip,
            faces,
            label_bank: std::sync::OnceLock::new(),
        }
    }

    /// The job kinds this worker can run with its models.
    pub fn kinds(&self) -> Vec<&'static str> {
        ml_kinds(self.clip, self.faces)
    }

    /// The current label bank, cached once found: an empty one (not computed
    /// yet) is read again next time, so tags start as soon as it exists.
    pub async fn label_bank(
        &self,
        pool: &PgPool,
    ) -> Result<std::sync::Arc<LabelBank>, squirrel_db::DbError> {
        if let Some(bank) = self.label_bank.get() {
            return Ok(bank.clone());
        }
        let bank = std::sync::Arc::new(
            squirrel_db::clip::label_embeddings(
                pool,
                squirrel_ml::CLIP_MODEL_ID,
                squirrel_ml::clip::labels::LABELS_VERSION,
            )
            .await?,
        );
        if !bank.is_empty() {
            let _ = self.label_bank.set(bank.clone());
        }
        Ok(bank)
    }
}

/// `clip_embed` needs the CLIP image tower; the face kinds need the face models.
pub fn ml_kinds(clip: bool, faces: bool) -> Vec<&'static str> {
    use crate::jobs::{CLIP_EMBED, CLUSTER_FACES, FACE_BACKFILL, FACES};
    let clip_kinds = clip.then_some(CLIP_EMBED);
    let face_kinds = faces
        .then_some([FACES, CLUSTER_FACES, FACE_BACKFILL])
        .into_iter()
        .flatten();
    clip_kinds.into_iter().chain(face_kinds).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml_kinds_follow_the_installed_models() {
        assert!(ml_kinds(false, false).is_empty());
        assert_eq!(ml_kinds(true, false), ["clip_embed"]);
        assert_eq!(
            ml_kinds(false, true),
            ["faces", "cluster_faces", "face_backfill"]
        );
        assert_eq!(ml_kinds(true, true).len(), 4);
    }
}
