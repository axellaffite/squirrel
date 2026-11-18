//! `face_backfill`: after a user turns faces on, queues `faces` for their
//! photos without a scan, one page of nodes at a time.

use serde::Deserialize;
use squirrel_db::{faces, ml_settings};
use squirrel_jobs::{JobContext, JobError, kinds};
use uuid::Uuid;

use crate::context::MediaContext;
use crate::jobs;

/// Photos examined per run.
pub const PAGE: i64 = 500;

#[derive(Deserialize)]
struct Payload {
    owner_id: Uuid,
    after: Option<Uuid>,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    run_page(ctx, job, PAGE).await
}

/// [`run`] with another page size (tests).
#[doc(hidden)]
pub async fn run_page(ctx: &MediaContext, job: JobContext, page: i64) -> Result<(), JobError> {
    let Payload { owner_id, after } = job.payload()?;
    if !ml_settings::faces_enabled(&ctx.pool, owner_id)
        .await
        .map_err(JobError::retry)?
    {
        return Ok(());
    }
    let photos =
        faces::unscanned_page(&ctx.pool, owner_id, squirrel_ml::FACE_MODEL_ID, after, page)
            .await
            .map_err(JobError::retry)?;
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    for (_, sha256) in &photos {
        squirrel_jobs::enqueue(&mut tx, &jobs::faces(sha256, true))
            .await
            .map_err(JobError::retry)?;
    }
    if i64::try_from(photos.len()).unwrap_or(0) == page {
        let next = kinds::face_backfill(owner_id, photos.last().map(|(node, _)| *node))
            .delay(jobs::BACKFILL_PAGE_DELAY);
        squirrel_jobs::enqueue(&mut tx, &next)
            .await
            .map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)
}
