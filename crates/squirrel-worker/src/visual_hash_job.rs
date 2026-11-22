//! `visual_hash:<sha256>`: the perceptual hash of a photo whose thumbnail was
//! made before hashes existed, computed in the sandbox from the stored
//! thumbnail (the thumbnails step hashes new photos itself). Then asks for a
//! duplicates grouping run of every owner of the photo.

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::duplicates;
use squirrel_db::media::{self, MediaState};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::{ExecRequest, ExecResponse};
use squirrel_storage::Variant;

use crate::context::MediaContext;
use crate::jobs::ShaPayload;

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if media.kind() != MediaKind::Image || media.thumbs_state() != MediaState::Ready {
        return Ok(());
    }
    let hashed: Option<i64> = sqlx::query_scalar("SELECT phash FROM media WHERE sha256 = $1")
        .bind(&sha256)
        .fetch_one(&ctx.pool)
        .await
        .map_err(JobError::retry)?;
    if hashed.is_some() {
        return Ok(());
    }
    let input = ctx
        .storage
        .derived()
        .variant_path(&hash, Variant::Thumb, media.thumbs_version);
    let request = ExecRequest::VisualHash {
        input,
        limits: ctx.decode_limits,
    };
    match ctx.executor.run(request).await {
        Ok(ExecResponse::VisualHash { phash }) => record(ctx, &sha256, phash).await,
        Ok(ExecResponse::Failed {
            retryable: true,
            reason,
        }) => Err(JobError::retry(anyhow::anyhow!(reason))),
        Ok(ExecResponse::Unsupported { reason } | ExecResponse::Failed { reason, .. }) => {
            // Without a hash the photo is simply never grouped.
            tracing::warn!(sha256 = %sha256, reason = %reason, "no perceptual hash");
            Ok(())
        }
        Ok(other) => {
            tracing::warn!(sha256 = %sha256, response = ?other, "unexpected sandbox response");
            Ok(())
        }
        Err(err) => Err(JobError::retry(err)),
    }
}

async fn record(ctx: &MediaContext, sha256: &str, phash: u64) -> Result<(), JobError> {
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    if duplicates::set_phash(&mut *tx, sha256, phash)
        .await
        .map_err(JobError::retry)?
    {
        crate::jobs::request_grouping_of_blob(&mut tx, sha256).await?;
    }
    tx.commit().await.map_err(JobError::retry)
}
