//! The frame the models look at: a 1440 px PNG rendered from the original in
//! the sandbox (previews are no longer stored), in a private directory under
//! the system's temporary directory, so a read-only data directory works too.
//! Removed when the [`Frame`] is dropped.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::blobs;
use squirrel_db::media::{MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::ExecResponse;
use squirrel_media::preview::{DEFAULT_BUCKET, PreviewEncoding};
use squirrel_sandbox::preview::PreviewJob;
use uuid::Uuid;

use crate::context::MediaContext;
use crate::outcome;

/// Mode of the frame directory: private to the worker.
const DIR_MODE: u32 = 0o700;

/// A rendered frame; its directory goes with it.
#[derive(Debug)]
pub struct Frame {
    dir: PathBuf,
    path: PathBuf,
}

impl Frame {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Why there is no frame.
#[derive(Debug)]
pub enum NoFrame {
    /// The blob cannot be decoded (format, size): retrying cannot help.
    Unsupported(String),
    /// Corrupt bytes (`retryable: false`), or an I/O or sandbox problem.
    Failed { reason: String, retryable: bool },
}

/// Renders `hash`'s 1440 px frame. `Err` only for database or local I/O
/// trouble worth a plain retry.
pub async fn render(
    ctx: &MediaContext,
    hash: &ContentHash,
) -> Result<Result<Frame, NoFrame>, JobError> {
    let Some(blob) = blobs::find(&ctx.pool, hash)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(Err(NoFrame::Unsupported("the blob is gone".into())));
    };
    let dir = std::env::temp_dir().join(format!("squirrel-frame-{}", Uuid::now_v7()));
    std::fs::DirBuilder::new()
        .mode(DIR_MODE)
        .create(&dir)
        .map_err(JobError::retry)?;
    let mut frame = Frame {
        dir: dir.clone(),
        path: PathBuf::new(),
    };
    let job = PreviewJob {
        input: ctx.storage.blob_path(hash),
        out_dir: dir,
        bucket: DEFAULT_BUCKET,
        encoding: PreviewEncoding::Png,
        heif: MediaKind::is_heif_family(&blob.mime),
    };
    let preview = match ctx.previews.render(job).await {
        Ok(ExecResponse::Preview { preview }) => preview,
        Ok(ExecResponse::Unsupported { reason }) => return Ok(Err(NoFrame::Unsupported(reason))),
        Ok(ExecResponse::Failed { reason, retryable }) => {
            return Ok(Err(NoFrame::Failed { reason, retryable }));
        }
        Ok(other) => {
            return Ok(Err(NoFrame::Failed {
                reason: format!("unexpected sandbox response {other:?}"),
                retryable: false,
            }));
        }
        Err(err) => {
            return Ok(Err(NoFrame::Failed {
                reason: err.to_string(),
                retryable: true,
            }));
        }
    };
    frame.path = frame.dir.join(preview.format.file_name());
    // The sandbox wrote it: hand the models a regular file, never a symlink.
    match squirrel_storage::preview_cache::check_rendered(&frame.path) {
        Ok(()) => Ok(Ok(frame)),
        Err(err) => Ok(Err(NoFrame::Failed {
            reason: format!("the sandbox output was refused: {err}"),
            retryable: false,
        })),
    }
}

/// The frame for `step`, or `None` once the step's outcome is recorded
/// (unsupported, failed, or a retry scheduled).
pub async fn for_step(
    ctx: &MediaContext,
    job: &JobContext,
    hash: &ContentHash,
    step: Step,
) -> Result<Option<Frame>, JobError> {
    let sha256 = hash.as_str();
    match render(ctx, hash).await? {
        Ok(frame) => Ok(Some(frame)),
        Err(NoFrame::Unsupported(reason)) => {
            outcome::mark(ctx, sha256, step, MediaState::Unsupported, &reason).await?;
            Ok(None)
        }
        Err(NoFrame::Failed {
            reason,
            retryable: false,
        }) => {
            outcome::mark(ctx, sha256, step, MediaState::Failed, &reason).await?;
            Ok(None)
        }
        Err(NoFrame::Failed { reason, .. }) => {
            outcome::give_up_or_retry(ctx, job, sha256, step, reason).await?;
            Ok(None)
        }
    }
}
