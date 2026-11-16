//! `process_media`: classify a stored file and queue its per-blob work.

use serde::Deserialize;
use squirrel_core::MediaKind;
use squirrel_db::assets;
use squirrel_db::{media, nodes};
use squirrel_jobs::{JobContext, JobError};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::context::MediaContext;
use crate::{facts, jobs};

#[derive(Deserialize)]
struct Payload {
    node_id: Uuid,
    #[serde(default)]
    background: bool,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let payload: Payload = job.payload()?;
    let Some(node) = nodes::find_any(&ctx.pool, payload.node_id)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(()); // deleted for good meanwhile
    };
    let (Some(sha), Some(mime)) = (node.blob_sha256.clone(), node.mime.clone()) else {
        return Ok(()); // a folder
    };
    let Some(kind) = MediaKind::for_mime(&mime) else {
        return Ok(());
    };
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    let media = media::ensure(&mut tx, &sha, kind)
        .await
        .map_err(JobError::retry)?;
    if kind == MediaKind::Image
        && let Some(target) = assets::blob_node(&mut *tx, node.id)
            .await
            .map_err(JobError::retry)?
    {
        let asset = facts::asset_for(&target, &media, OffsetDateTime::now_utc());
        assets::upsert(&mut tx, &asset)
            .await
            .map_err(JobError::retry)?;
    }
    for follow_up in jobs::follow_ups(&media, payload.background) {
        squirrel_jobs::enqueue(&mut tx, &follow_up)
            .await
            .map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)?;
    Ok(())
}
