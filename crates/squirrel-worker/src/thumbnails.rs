//! `thumbnails`: render a blob's grid thumbnail and lightbox preview in the
//! sandbox, install them under `derived/`, then record them.

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::blobs::{self, BlobRecord};
use squirrel_db::media::{self, MediaRecord, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::{ExecRequest, ExecResponse, MAX_INPUT_BYTES};
use squirrel_media::render::{Rendered, THUMBS_VERSION};
use squirrel_storage::{Scratch, StorageError};

use crate::context::MediaContext;
use crate::jobs::ShaPayload;
use crate::outcome;
use crate::pdf_job::{self, PdfSteps};

pub const NO_HEIC: &str = "built without HEIC support";
/// Blobs over the sandbox's input cap are never decoded.
pub const TOO_LARGE: &str = "too large to decode";
pub use crate::context::NO_PDFIUM;

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    let current =
        media.thumbs_state() == MediaState::Ready && media.thumbs_version == THUMBS_VERSION;
    if media.kind() == MediaKind::Pdf {
        // One pass for the page and, while it is pending, the text.
        let steps = PdfSteps {
            render: !current,
            text: media.text_state() == MediaState::Pending,
        };
        if !steps.render && !steps.text {
            return Ok(());
        }
        return pdf_job::run(ctx, &job, &hash, steps).await;
    }
    if current {
        return Ok(());
    }
    let Some(blob) = blobs::find(&ctx.pool, &hash)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if let Some(reason) = unsupported_reason(ctx, &media, &blob) {
        return mark(ctx, &sha256, MediaState::Unsupported, reason).await;
    }
    let derived = ctx.storage.derived().clone();
    let scratch = tokio::task::spawn_blocking(move || derived.scratch())
        .await
        .map_err(JobError::retry)?
        .map_err(JobError::retry)?;
    let request = ExecRequest::Image {
        input: ctx.storage.blob_path(&hash),
        out_dir: scratch.path().to_owned(),
        limits: ctx.decode_limits,
    };
    match ctx.executor.run(request).await {
        Ok(ExecResponse::Image { rendered }) => {
            install_for(ctx, &hash, scratch, &rendered, Some(&job)).await
        }
        Ok(ExecResponse::Pdf { .. }) => {
            let reason = "unexpected sandbox response";
            mark(ctx, &sha256, MediaState::Failed, reason).await
        }
        Ok(ExecResponse::Unsupported { reason }) => {
            mark(ctx, &sha256, MediaState::Unsupported, &reason).await
        }
        Ok(ExecResponse::Failed {
            reason,
            retryable: false,
        }) => mark(ctx, &sha256, MediaState::Failed, &reason).await,
        Ok(ExecResponse::Failed {
            reason,
            retryable: true,
        }) => outcome::give_up_or_retry(ctx, &job, &sha256, Step::Thumbs, reason).await,
        Err(err) => {
            outcome::give_up_or_retry(ctx, &job, &sha256, Step::Thumbs, err.to_string()).await
        }
    }
}

/// Why no thumbnail can be made for this blob, decided without decoding.
fn unsupported_reason(
    ctx: &MediaContext,
    media: &MediaRecord,
    blob: &BlobRecord,
) -> Option<&'static str> {
    if u64::try_from(blob.size).unwrap_or(u64::MAX) > MAX_INPUT_BYTES {
        return Some(TOO_LARGE);
    }
    match media.kind() {
        MediaKind::Image
            if MediaKind::is_heif_family(&blob.mime) && !squirrel_media::HEIC_SUPPORTED =>
        {
            Some(NO_HEIC)
        }
        MediaKind::Image => None,
        // Handled by `pdf_job` before this is asked.
        MediaKind::Pdf => ctx.pdfium().err(),
        MediaKind::Text => Some("no preview for text"),
    }
}

/// Moves the variants into place, then records them (row after files: a
/// crash in between only leaves files the next run overwrites).
pub(crate) async fn install(
    ctx: &MediaContext,
    hash: &ContentHash,
    scratch: Scratch,
    rendered: &Rendered,
) -> Result<(), JobError> {
    install_for(ctx, hash, scratch, rendered, None).await
}

/// [`install`], then (for an image job) queues its ML steps, at the priority
/// of the thumbnails job's origin (fresh upload or backfill).
async fn install_for(
    ctx: &MediaContext,
    hash: &ContentHash,
    scratch: Scratch,
    rendered: &Rendered,
    job: Option<&JobContext>,
) -> Result<(), JobError> {
    let (derived, blob) = (ctx.storage.derived().clone(), hash.clone());
    let installed =
        tokio::task::spawn_blocking(move || derived.install(&blob, THUMBS_VERSION, &scratch))
            .await
            .map_err(JobError::retry)?;
    match installed {
        Ok(()) => {}
        // The sandbox wrote something that is not a thumbnail: no retry fixes that.
        Err(StorageError::Io(err)) if err.kind() == std::io::ErrorKind::InvalidData => {
            let reason = format!("the sandbox output was refused: {err}");
            return mark(ctx, hash.as_str(), MediaState::Failed, &reason).await;
        }
        Err(err) => return Err(JobError::retry(err)),
    }
    let recorded = media::set_thumbnails(
        &ctx.pool,
        hash.as_str(),
        THUMBS_VERSION,
        rendered.width,
        rendered.height,
        &rendered.blurhash,
    )
    .await
    .map_err(JobError::retry)?;
    if recorded && let Some(job) = job {
        queue_ml(ctx, hash.as_str(), job).await?;
    }
    if !recorded {
        // Collected while rendering: what was just installed belongs to nothing.
        let (derived, blob) = (ctx.storage.derived().clone(), hash.clone());
        tokio::task::spawn_blocking(move || derived.remove_all(&blob))
            .await
            .map_err(JobError::retry)?
            .map_err(JobError::retry)?;
    }
    Ok(())
}

/// Queues the image's pending ML steps. Queued whether or not this worker has
/// models: a worker that has them (now or later) runs them.
async fn queue_ml(ctx: &MediaContext, sha256: &str, job: &JobContext) -> Result<(), JobError> {
    let Some(media) = media::find(&ctx.pool, sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    let background = job.priority < squirrel_jobs::priority::THUMBNAILS;
    let mut conn = ctx.pool.acquire().await.map_err(JobError::retry)?;
    for next in crate::jobs::ml_follow_ups(&media, background) {
        squirrel_jobs::enqueue(&mut conn, &next)
            .await
            .map_err(JobError::retry)?;
    }
    Ok(())
}

async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    outcome::mark(ctx, sha256, Step::Thumbs, state, reason).await
}
