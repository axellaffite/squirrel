//! `thumbnails`: render a blob's grid thumbnail and lightbox preview in the
//! sandbox, install them under `derived/`, then record them.

use std::sync::atomic::{AtomicBool, Ordering};

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::blobs::{self, BlobRecord};
use squirrel_db::media::{self, MediaRecord, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::{ExecRequest, ExecResponse, MAX_INPUT_BYTES};
use squirrel_media::render::{Rendered, THUMBS_VERSION};
use squirrel_storage::{Scratch, StorageError};

use crate::context::MediaContext;
use crate::jobs::ShaPayload;

pub const NO_HEIC: &str = "built without HEIC support";
const NO_PDFIUM: &str = "pdfium is not configured";

/// "pdfium is not configured" is worth one warning per process, not one per PDF.
static WARNED_NO_PDFIUM: AtomicBool = AtomicBool::new(false);

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if media.thumbs_state() == MediaState::Ready && media.thumbs_version == THUMBS_VERSION {
        return Ok(());
    }
    let Some(blob) = blobs::find(&ctx.pool, &hash)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if let Some(reason) = unsupported_reason(&media, &blob, ctx.pdfium_dir.is_some()) {
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
        Ok(ExecResponse::Image { rendered }) => install(ctx, &hash, scratch, &rendered).await,
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
        }) => give_up_or_retry(ctx, &job, &sha256, reason).await,
        Err(err) => give_up_or_retry(ctx, &job, &sha256, err.to_string()).await,
    }
}

/// Why no thumbnail can be made for this blob, decided without decoding.
fn unsupported_reason(
    media: &MediaRecord,
    blob: &BlobRecord,
    has_pdfium: bool,
) -> Option<&'static str> {
    if u64::try_from(blob.size).unwrap_or(u64::MAX) > MAX_INPUT_BYTES {
        return Some("too large to decode");
    }
    match media.kind() {
        MediaKind::Image
            if MediaKind::is_heif_family(&blob.mime) && !squirrel_media::HEIC_SUPPORTED =>
        {
            Some(NO_HEIC)
        }
        MediaKind::Image => None,
        MediaKind::Pdf => {
            if !has_pdfium && !WARNED_NO_PDFIUM.swap(true, Ordering::Relaxed) {
                tracing::warn!("PDF thumbnails are disabled: set SQUIRREL_PDFIUM_DIR");
            }
            // PDF rendering through pdfium arrives with Milestone E.
            Some(NO_PDFIUM)
        }
        MediaKind::Text => Some("no preview for text"),
    }
}

/// Moves the variants into place, then records them (row after files: a
/// crash in between only leaves files the next run overwrites).
async fn install(
    ctx: &MediaContext,
    hash: &ContentHash,
    scratch: Scratch,
    rendered: &Rendered,
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
    media::set_thumbnails(
        &ctx.pool,
        hash.as_str(),
        THUMBS_VERSION,
        rendered.width,
        rendered.height,
        &rendered.blurhash,
    )
    .await
    .map_err(JobError::retry)
}

/// A terminal state for this blob's thumbnails: the job itself succeeded.
async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    tracing::info!(sha256, state = state.as_str(), reason, "no thumbnails");
    media::set_state(&ctx.pool, sha256, Step::Thumbs, state, Some(reason))
        .await
        .map_err(JobError::retry)
}

/// Retries a transient failure; on the last attempt records `failed` first so
/// the UI does not show "Preparing…" forever.
async fn give_up_or_retry(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    reason: String,
) -> Result<(), JobError> {
    if job.is_last_attempt() {
        media::set_state(
            &ctx.pool,
            sha256,
            Step::Thumbs,
            MediaState::Failed,
            Some(&reason),
        )
        .await
        .map_err(JobError::retry)?;
    }
    Err(JobError::retry(anyhow::anyhow!(reason)))
}
