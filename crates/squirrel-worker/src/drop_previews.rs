//! `drop_previews`: deletes the 1440 px previews stored for photos before
//! they were rendered on request (`preview-v*.webp`), 500 photos per run,
//! each run queueing the next page. PDF previews stay. Idempotent: queued by
//! hand (`squirrel admin media drop-previews`) once the release is checked.

use serde::Deserialize;
use serde_json::json;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_jobs::{JobContext, JobError, NewJob, priority};
use squirrel_storage::Variant;

use crate::context::MediaContext;
use crate::jobs::DROP_PREVIEWS;

/// Photos examined per run.
pub const PAGE: i64 = 500;

#[derive(Deserialize, Default)]
struct Payload {
    after: Option<String>,
}

/// The first page (a no-op to queue while one is queued or running).
pub fn first() -> NewJob {
    page(None)
}

fn page(after: Option<&str>) -> NewJob {
    let key = after.map_or_else(
        || DROP_PREVIEWS.to_owned(),
        |after| format!("{DROP_PREVIEWS}:{after}"),
    );
    NewJob::new(DROP_PREVIEWS, json!({ "after": after }))
        .priority(priority::BACKFILL)
        .dedupe(key)
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    run_page(ctx, job, PAGE).await
}

/// [`run`] with another page size (tests).
#[doc(hidden)]
pub async fn run_page(ctx: &MediaContext, job: JobContext, size: i64) -> Result<(), JobError> {
    let Payload { after } = job.payload()?;
    let hashes: Vec<String> = sqlx::query_scalar(
        "SELECT sha256 FROM media WHERE kind = $1 AND sha256 > $2 ORDER BY sha256 LIMIT $3",
    )
    .bind(MediaKind::Image.as_str())
    .bind(after.as_deref().unwrap_or(""))
    .bind(size)
    .fetch_all(&ctx.pool)
    .await
    .map_err(JobError::retry)?;
    let derived = ctx.storage.derived().clone();
    let batch = hashes.clone();
    let removed = tokio::task::spawn_blocking(move || {
        let mut removed = 0;
        for sha256 in &batch {
            let hash = ContentHash::parse(sha256).map_err(JobError::permanent)?;
            removed += derived
                .remove_variant(&hash, Variant::Preview)
                .map_err(JobError::retry)?;
        }
        Ok::<_, JobError>(removed)
    })
    .await
    .map_err(JobError::retry)??;
    tracing::info!(
        photos = hashes.len(),
        removed,
        "stored photo previews dropped"
    );
    if i64::try_from(hashes.len()).unwrap_or(i64::MAX) >= size
        && let Some(last) = hashes.last()
    {
        let mut conn = ctx.pool.acquire().await.map_err(JobError::retry)?;
        squirrel_jobs::enqueue(&mut conn, &page(Some(last)))
            .await
            .map_err(JobError::retry)?;
    }
    Ok(())
}
