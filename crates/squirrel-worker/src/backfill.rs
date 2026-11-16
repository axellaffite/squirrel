//! `backfill`: queue processing for files stored before the pipeline existed
//! (or before a new step), one page of nodes at a time.

use serde::Deserialize;
use sqlx::PgConnection;
use squirrel_core::MediaKind;
use squirrel_jobs::{JobContext, JobError, kinds, priority};
use squirrel_media::render::THUMBS_VERSION;
use uuid::Uuid;

use crate::context::{MediaContext, NO_PDFIUM};
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
    // Loaded (once per process) before the transaction opens.
    let dataset = match after {
        None => ctx.geo.installed_dataset_id().await,
        Some(_) => None,
    };
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    for id in &ids {
        squirrel_jobs::enqueue(&mut tx, &kinds::process_media(*id, true))
            .await
            .map_err(JobError::retry)?;
    }
    if after.is_none() {
        requeue_stale_steps(&mut tx, dataset.as_deref()).await?;
        if ctx.pdfium_dir.is_some() {
            requeue_pdfs_awaiting_pdfium(&mut tx).await?;
        }
    }
    if i64::try_from(ids.len()).unwrap_or(0) == page {
        let next = jobs::backfill(ids.last().copied()).delay(jobs::BACKFILL_PAGE_DELAY);
        squirrel_jobs::enqueue(&mut tx, &next)
            .await
            .map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)
}

/// `INSERT … SELECT` of one per-blob job (same payload, dedupe key and
/// priority as [`jobs::thumbnails`] and its siblings) for every `sha256`
/// that `blobs` (a fixed SQL fragment reading `$3`) selects: one statement,
/// however many blobs.
fn per_blob_insert(blobs: &str) -> String {
    format!(
        "INSERT INTO jobs (kind, payload, priority, dedupe_key, run_at) \
         SELECT $1, jsonb_build_object('sha256', b.sha256), $2, $1 || ':' || b.sha256, now() \
           FROM ({blobs}) AS b \
         ON CONFLICT (dedupe_key) \
            WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') DO NOTHING"
    )
}

/// Thumbnails made with older settings are regenerated, and geotagged blobs
/// not yet geocoded with the installed dataset are geocoded (again).
async fn requeue_stale_steps(
    conn: &mut PgConnection,
    dataset: Option<&str>,
) -> Result<(), JobError> {
    let stale = per_blob_insert(
        "SELECT sha256 FROM media WHERE thumbs_state = 'ready' AND thumbs_version < $3",
    );
    sqlx::query(&stale)
        .bind(jobs::THUMBNAILS)
        .bind(priority::background(priority::THUMBNAILS))
        .bind(THUMBS_VERSION)
        .execute(&mut *conn)
        .await
        .map_err(JobError::retry)?;
    let Some(dataset) = dataset else {
        return Ok(());
    };
    let ungeocoded = per_blob_insert(
        "SELECT sha256 FROM media \
          WHERE latitude IS NOT NULL AND geocoded_with IS DISTINCT FROM $3",
    );
    sqlx::query(&ungeocoded)
        .bind(jobs::GEOCODE)
        .bind(priority::background(priority::GEOCODE))
        .bind(dataset)
        .execute(&mut *conn)
        .await
        .map_err(JobError::retry)?;
    Ok(())
}

/// PDF steps skipped because no pdfium was configured go back to `pending`
/// and are queued again, now that one is.
///
/// The only record of *why* a step is unsupported is `media.last_error`, so
/// this matches its exact text ([`NO_PDFIUM`]): changing that message strands
/// PDFs skipped under the old one. Workers with different pdfium settings on
/// one database can flip a PDF back and forth (one marks it unsupported, the
/// next one's start queues it again); run them alike.
///
/// It runs once per worker start, as one statement; a sequential scan of
/// `media` is cheap at a home library's size, so no partial index was added.
async fn requeue_pdfs_awaiting_pdfium(conn: &mut PgConnection) -> Result<(), JobError> {
    let reset = "WITH reset AS ( \
             UPDATE media \
                SET thumbs_state = CASE WHEN thumbs_state = 'unsupported' THEN 'pending' \
                                        ELSE thumbs_state END, \
                    text_state = CASE WHEN text_state = 'unsupported' THEN 'pending' \
                                      ELSE text_state END, \
                    last_error = NULL, updated_at = now() \
              WHERE kind = 'pdf' AND last_error = $1 \
                AND 'unsupported' IN (thumbs_state, text_state) \
             RETURNING sha256, thumbs_state, text_state) \
         INSERT INTO jobs (kind, payload, priority, dedupe_key, run_at) \
         SELECT step.kind, jsonb_build_object('sha256', reset.sha256), step.priority, \
                step.kind || ':' || reset.sha256, now() \
           FROM reset, LATERAL (VALUES ($2, $3, reset.thumbs_state), \
                                       ($4, $5, reset.text_state)) \
                AS step (kind, priority, state) \
          WHERE step.state = 'pending' \
         ON CONFLICT (dedupe_key) \
            WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') DO NOTHING";
    let queued = sqlx::query(reset)
        .bind(NO_PDFIUM)
        .bind(jobs::THUMBNAILS)
        .bind(priority::background(priority::THUMBNAILS))
        .bind(jobs::EXTRACT_TEXT)
        .bind(priority::background(priority::TEXT))
        .execute(&mut *conn)
        .await
        .map_err(JobError::retry)?;
    if queued.rows_affected() > 0 {
        tracing::info!(
            jobs = queued.rows_affected(),
            "queued PDF steps skipped while pdfium was not configured"
        );
    }
    Ok(())
}
