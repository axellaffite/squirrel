//! `clip_embed`: the CLIP image embedding of a blob's preview, then its
//! zero-shot tags against the label bank. Content-derived: one run per blob,
//! whoever owns it.

use std::time::Duration;

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media::{self, MediaRecord, MediaState, Step};
use squirrel_db::tags::{self, TagSource};
use squirrel_jobs::{JobContext, JobError};
use squirrel_ml::CLIP_MODEL_ID;
use squirrel_ml::clip::labels::{self, LABELS_VERSION, TOP_K};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use squirrel_storage::Variant;

use crate::context::{MediaContext, MlContext};
use crate::jobs::ShaPayload;
use crate::outcome;

/// Thumbnails still being made: look again later, without using an attempt.
pub const WAIT_FOR_PREVIEW: Duration = Duration::from_secs(5 * 60);

pub async fn run(ctx: &MediaContext, ml: &MlContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if media.kind() != MediaKind::Image {
        return mark(ctx, &sha256, MediaState::Unsupported, "not an image").await;
    }
    match media.thumbs_state() {
        MediaState::Pending => return Err(JobError::Reschedule(WAIT_FOR_PREVIEW)),
        MediaState::Failed | MediaState::Unsupported => {
            return mark(
                ctx,
                &sha256,
                MediaState::Unsupported,
                "no preview to analyse",
            )
            .await;
        }
        MediaState::Ready => {}
    }
    let stored = squirrel_db::clip::embedding(&ctx.pool, &sha256, CLIP_MODEL_ID)
        .await
        .map_err(JobError::retry)?;
    let embedding = match stored {
        Some(stored)
            if media.clip_state() == MediaState::Ready
                && stored.tagged_labels_version == Some(LABELS_VERSION) =>
        {
            return Ok(());
        }
        // Embedded already (e.g. the label bank changed): only tag again.
        Some(stored) => stored.values,
        None => match embed(ctx, ml, &job, &media, &hash).await? {
            Some(values) => values,
            None => return Ok(()),
        },
    };
    tag(ctx, ml, &sha256, &embedding).await
}

/// Runs the image tower on the preview and stores the result. `None`: the
/// step ended without an embedding (state recorded).
async fn embed(
    ctx: &MediaContext,
    ml: &MlContext,
    job: &JobContext,
    media: &MediaRecord,
    hash: &ContentHash,
) -> Result<Option<Vec<f32>>, JobError> {
    let sha256 = hash.as_str();
    let input = ctx
        .storage
        .derived()
        .variant_path(hash, Variant::Preview, media.thumbs_version);
    let reason = match ml.client.request(MlOp::ClipImage { input }).await {
        Ok(MlBody::Ok(MlOutput::Embedding { embedding })) => {
            squirrel_db::clip::store_embedding(&ctx.pool, sha256, CLIP_MODEL_ID, &embedding)
                .await
                .map_err(JobError::retry)?;
            return Ok(Some(embedding));
        }
        Ok(MlBody::Unsupported { reason }) => {
            mark(ctx, sha256, MediaState::Unsupported, &reason).await?;
            return Ok(None);
        }
        Ok(MlBody::Ok(other)) => format!("unexpected answer {other:?}"),
        Ok(MlBody::Failed { reason }) => reason,
        Err(err) => err.to_string(),
    };
    outcome::give_up_or_retry(ctx, job, sha256, Step::Clip, reason).await?;
    Ok(None)
}

/// Replaces the blob's `clip` tags (no tag at all without a label bank) and
/// marks the step ready, in one transaction.
async fn tag(
    ctx: &MediaContext,
    ml: &MlContext,
    sha256: &str,
    embedding: &[f32],
) -> Result<(), JobError> {
    let bank = ml.label_bank(&ctx.pool).await.map_err(JobError::retry)?;
    let thresholds = labels::thresholds(CLIP_MODEL_ID);
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    if let (false, Some(thresholds)) = (bank.is_empty(), thresholds) {
        let selected = labels::select(&labels::score(embedding, &bank), thresholds, TOP_K);
        tags::replace_tags(&mut tx, sha256, TagSource::Clip, &selected, LABELS_VERSION)
            .await
            .map_err(JobError::retry)?;
        squirrel_db::clip::mark_tagged(&mut tx, sha256, CLIP_MODEL_ID, LABELS_VERSION)
            .await
            .map_err(JobError::retry)?;
    }
    sqlx::query("UPDATE media SET clip_state = 'ready', updated_at = now() WHERE sha256 = $1")
        .bind(sha256)
        .execute(&mut *tx)
        .await
        .map_err(JobError::retry)?;
    tx.commit().await.map_err(JobError::retry)
}

async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    outcome::mark(ctx, sha256, Step::Clip, state, reason).await
}
