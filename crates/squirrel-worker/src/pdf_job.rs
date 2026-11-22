//! One sandbox run over a PDF for both of its steps: the `thumbnails` job
//! asks for the first page and, while the text is pending, the text too, so a
//! PDF is opened once. `extract_text` only runs pdfium itself when the
//! thumbnails are already settled.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use squirrel_core::ContentHash;
use squirrel_db::blobs;
use squirrel_db::documents;
use squirrel_db::media::{self, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::{ExecRequest, ExecResponse, MAX_INPUT_BYTES, TEXT_FILE};
use squirrel_media::render::Rendered;
use squirrel_storage::Scratch;

use crate::context::MediaContext;
use crate::exec::ExecError;
use crate::outcome;
use crate::text::read_prefix;
use crate::thumbnails::{self, TOO_LARGE};

/// A PDF that crashed or hung the sandbox this many times is given up on:
/// it would only burn the remaining attempts the same way.
const MAX_SANDBOX_CRASHES: i32 = 2;

/// What to get out of the PDF.
#[derive(Debug, Clone, Copy)]
pub struct PdfSteps {
    pub render: bool,
    pub text: bool,
}

impl PdfSteps {
    fn list(self) -> Vec<Step> {
        let mut steps = Vec::new();
        if self.render {
            steps.push(Step::Thumbs);
        }
        if self.text {
            steps.push(Step::Text);
        }
        steps
    }
}

/// Runs the sandbox once for `steps` and records every result.
pub async fn run(
    ctx: &MediaContext,
    job: &JobContext,
    hash: &ContentHash,
    steps: PdfSteps,
) -> Result<(), JobError> {
    let Some(pdfium_dir) = preflight(ctx, hash, steps).await? else {
        return Ok(());
    };
    let derived = ctx.storage.derived().clone();
    let scratch = tokio::task::spawn_blocking(move || derived.scratch())
        .await
        .map_err(JobError::retry)?
        .map_err(JobError::retry)?;
    let request = ExecRequest::Pdf {
        input: ctx.storage.blob_path(hash),
        out_dir: scratch.path().to_owned(),
        pdfium_dir,
        render: steps.render,
        text: steps.text,
    };
    match ctx.executor.run(request).await {
        Ok(ExecResponse::Pdf {
            rendered,
            page_count,
        }) => record(ctx, job, hash, steps, scratch, rendered, page_count).await,
        other => settle_failure(ctx, job, hash.as_str(), &steps.list(), other).await,
    }
}

/// The pdfium directory to use, or `None` once the steps were settled
/// without decoding (collected blob, too large, no pdfium).
async fn preflight(
    ctx: &MediaContext,
    hash: &ContentHash,
    steps: PdfSteps,
) -> Result<Option<PathBuf>, JobError> {
    let Some(blob) = blobs::find(&ctx.pool, hash)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(None); // collected meanwhile
    };
    let reason = if u64::try_from(blob.size).unwrap_or(u64::MAX) > MAX_INPUT_BYTES {
        TOO_LARGE
    } else {
        match ctx.pdfium() {
            Ok(dir) => return Ok(Some(dir.clone())),
            Err(reason) => reason,
        }
    };
    let state = MediaState::Unsupported;
    outcome::mark_all(ctx, hash.as_str(), &steps.list(), state, reason).await?;
    Ok(None)
}

/// Every sandbox outcome but a PDF result.
async fn settle_failure(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    steps: &[Step],
    outcome: Result<ExecResponse, ExecError>,
) -> Result<(), JobError> {
    let (state, reason) = match outcome {
        Ok(ExecResponse::Unsupported { reason }) => (MediaState::Unsupported, reason),
        Ok(ExecResponse::Failed {
            reason,
            retryable: true,
        }) => return outcome::give_up_or_retry_all(ctx, job, sha256, steps, reason).await,
        Ok(ExecResponse::Failed { reason, .. }) => (MediaState::Failed, reason),
        Ok(
            ExecResponse::Image { .. }
            | ExecResponse::Pdf { .. }
            | ExecResponse::Preview { .. }
            | ExecResponse::Probe { .. },
        ) => (MediaState::Failed, "unexpected sandbox response".to_owned()),
        Err(err @ (ExecError::Crashed { .. } | ExecError::TimedOut(_)))
            if job.attempt >= MAX_SANDBOX_CRASHES =>
        {
            (MediaState::Failed, err.to_string())
        }
        Err(err) => {
            return outcome::give_up_or_retry_all(ctx, job, sha256, steps, err.to_string()).await;
        }
    };
    outcome::mark_all(ctx, sha256, steps, state, &reason).await
}

/// Records what a successful sandbox run produced: the text is read before
/// the thumbnails are moved out of the scratch directory.
async fn record(
    ctx: &MediaContext,
    job: &JobContext,
    hash: &ContentHash,
    steps: PdfSteps,
    scratch: Scratch,
    rendered: Option<Rendered>,
    page_count: u32,
) -> Result<(), JobError> {
    let sha256 = hash.as_str();
    let text = if steps.text {
        let path = scratch.path().join(TEXT_FILE);
        let read = tokio::task::spawn_blocking(move || read_sandbox_text(&path))
            .await
            .map_err(JobError::retry)?;
        match read {
            Ok(text) => Some(text),
            Err(err) => {
                let reason = format!("the sandbox output was refused: {err}");
                outcome::mark(ctx, sha256, Step::Text, MediaState::Failed, &reason).await?;
                None
            }
        }
    } else {
        None
    };
    if steps.render {
        match &rendered {
            Some(rendered) => thumbnails::install(ctx, hash, scratch, rendered).await?,
            None => {
                let reason = "the sandbox rendered no page";
                outcome::mark(ctx, sha256, Step::Thumbs, MediaState::Failed, reason).await?;
            }
        }
    }
    match text {
        Some(text) => store_text(ctx, job, sha256, &text, Some(page_count)).await,
        None => media::set_page_count(&ctx.pool, sha256, page_count)
            .await
            .map_err(JobError::retry),
    }
}

/// Stores the text (with a PDF's page count); a database failure is
/// retried, and ends the step as failed on the last attempt.
pub async fn store_text(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    text: &str,
    page_count: Option<u32>,
) -> Result<(), JobError> {
    // `false`: collected meanwhile, nothing left to record.
    match documents::store_text(&ctx.pool, sha256, text, page_count).await {
        Ok(_) => Ok(()),
        Err(err) => {
            outcome::give_up_or_retry(ctx, job, sha256, Step::Text, format!("{err:#}")).await
        }
    }
}

/// The text file an untrusted child wrote, opened without following a link
/// and checked on the open handle: a regular file with a single name (not a
/// hard link to something else the worker can read). No file: no text.
fn read_sandbox_text(path: &Path) -> io::Result<String> {
    let file = match open_no_follow(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(err) => return Err(refused(&err.to_string())),
    };
    let meta = file.metadata()?;
    if !meta.file_type().is_file() || meta.nlink() != 1 {
        return Err(refused("not a regular file with a single link"));
    }
    read_prefix(file)
}

fn open_no_follow(path: &Path) -> io::Result<File> {
    let no_follow = rustix::fs::OFlags::NOFOLLOW.bits();
    OpenOptions::new()
        .read(true)
        .custom_flags(i32::try_from(no_follow).unwrap_or(i32::MAX))
        .open(path)
}

fn refused(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{TEXT_FILE}: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_single_link_file_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(TEXT_FILE);
        assert_eq!(read_sandbox_text(&path).unwrap(), "", "no file, no text");
        std::fs::write(&path, "hello").unwrap();
        assert_eq!(read_sandbox_text(&path).unwrap(), "hello");
        std::fs::hard_link(&path, dir.path().join("other")).unwrap();
        assert!(read_sandbox_text(&path).is_err(), "hard link");
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(dir.path().join("other"), &path).unwrap();
        assert!(read_sandbox_text(&path).is_err(), "symlink");
    }
}
