//! `backfill`: queue processing for files stored before the pipeline existed
//! (or before a new step), one page of nodes at a time.

use serde::Deserialize;
use sqlx::PgConnection;
use squirrel_core::MediaKind;
use squirrel_jobs::{JobContext, JobError, kinds};
use squirrel_media::render::THUMBS_VERSION;
use uuid::Uuid;

use crate::context::MediaContext;
use crate::jobs;

/// Nodes examined per run.
pub const PAGE: i64 = 500;

#[derive(Deserialize, Default)]
struct Payload {
    after: Option<Uuid>,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    run_page(ctx, job, PAGE).await
}

/// [`run`] with another page size (tests).
#[doc(hidden)]
pub async fn run_page(ctx: &MediaContext, job: JobContext, page: i64) -> Result<(), JobError> {
    let Payload { after } = job.payload()?;
    // Every file node, live or trashed (restores then need no hook): images
    // without a timeline entry, documents whose blob has no media row.
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n \
          WHERE n.kind = 'file' AND n.id > $1 AND n.mime = ANY($2) \
            AND NOT EXISTS (SELECT 1 FROM assets a WHERE a.node_id = n.id) \
            AND NOT (n.mime <> ALL($3) \
                     AND EXISTS (SELECT 1 FROM media m WHERE m.sha256 = n.blob_sha256)) \
          ORDER BY n.id LIMIT $4",
    )
    .bind(after.unwrap_or(Uuid::nil()))
    .bind(MediaKind::all_mimes())
    .bind(MediaKind::Image.mimes())
    .bind(page)
    .fetch_all(&ctx.pool)
    .await
    .map_err(JobError::retry)?;
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    for id in &ids {
        squirrel_jobs::enqueue(&mut tx, &kinds::process_media(*id, true))
            .await
            .map_err(JobError::retry)?;
    }
    if after.is_none() {
        requeue_stale_steps(&mut tx).await?;
    }
    if i64::try_from(ids.len()).unwrap_or(0) == page {
        let next = jobs::backfill(ids.last().copied()).delay(jobs::BACKFILL_PAGE_DELAY);
        squirrel_jobs::enqueue(&mut tx, &next)
            .await
            .map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)
}

/// Thumbnails made with older settings are regenerated. (Geocoding with a
/// newer dataset joins in Milestone C.)
async fn requeue_stale_steps(conn: &mut PgConnection) -> Result<(), JobError> {
    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT sha256 FROM media WHERE thumbs_state = 'ready' AND thumbs_version < $1",
    )
    .bind(THUMBS_VERSION)
    .fetch_all(&mut *conn)
    .await
    .map_err(JobError::retry)?;
    for sha in &stale {
        squirrel_jobs::enqueue(conn, &jobs::thumbnails(sha, true))
            .await
            .map_err(JobError::retry)?;
    }
    Ok(())
}
