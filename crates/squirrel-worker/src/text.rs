//! `extract_text`: the searchable text of a document blob. Plain text is read
//! here (bounded, no parsing); PDFs go through pdfium in the sandbox.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::blobs;
use squirrel_db::documents::{self, MAX_BODY_BYTES};
use squirrel_db::media::{self, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::exec::{ExecRequest, ExecResponse, MAX_INPUT_BYTES, TEXT_FILE};

use crate::context::MediaContext;
use crate::jobs::ShaPayload;
use crate::outcome;
use crate::thumbnails::TOO_LARGE;

/// A UTF-8 character is at most this many bytes: reading this much past the
/// cap lets a character cut by it be dropped rather than replaced.
const CHAR_SLACK: usize = 3;

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(()); // the blob was collected meanwhile
    };
    if media.text_state() == MediaState::Ready {
        return Ok(());
    }
    match media.kind() {
        MediaKind::Image => Ok(()),
        MediaKind::Text => plain_text(ctx, &job, &sha256, &hash).await,
        MediaKind::Pdf => pdf_text(ctx, &job, &sha256, &hash).await,
    }
}

async fn plain_text(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    hash: &ContentHash,
) -> Result<(), JobError> {
    let path = ctx.storage.blob_path(hash);
    let read = tokio::task::spawn_blocking(move || read_prefix(&path))
        .await
        .map_err(JobError::retry)?;
    match read {
        Ok(text) => store(ctx, sha256, &text).await,
        Err(err) => outcome::give_up_or_retry(ctx, job, sha256, Step::Text, err.to_string()).await,
    }
}

/// The first [`MAX_BODY_BYTES`] of a file as text: invalid UTF-8 replaced, a
/// character cut at the end dropped.
pub fn read_prefix(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_BODY_BYTES + CHAR_SLACK) as u64)
        .read_to_end(&mut bytes)?;
    Ok(decode_prefix(&bytes))
}

fn decode_prefix(bytes: &[u8]) -> String {
    let cap = bytes.len().min(MAX_BODY_BYTES);
    let mut end = cap;
    if cap < bytes.len() {
        // Back up to the start of a character the cap cuts.
        while end > cap.saturating_sub(CHAR_SLACK) && (bytes[end] & 0xC0) == 0x80 {
            end -= 1;
        }
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

async fn pdf_text(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    hash: &ContentHash,
) -> Result<(), JobError> {
    let Some(blob) = blobs::find(&ctx.pool, hash)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if u64::try_from(blob.size).unwrap_or(u64::MAX) > MAX_INPUT_BYTES {
        return mark(ctx, sha256, MediaState::Unsupported, TOO_LARGE).await;
    }
    let pdfium_dir = match ctx.pdfium() {
        Ok(dir) => dir.clone(),
        Err(reason) => return mark(ctx, sha256, MediaState::Unsupported, reason).await,
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
        render: false,
        text: true,
    };
    match ctx.executor.run(request).await {
        Ok(ExecResponse::Pdf { page_count, .. }) => {
            let path = scratch.path().join(TEXT_FILE);
            let text = tokio::task::spawn_blocking(move || read_sandbox_text(&path))
                .await
                .map_err(JobError::retry)?;
            let text = match text {
                Ok(text) => text,
                Err(err) => {
                    let reason = format!("the sandbox output was refused: {err}");
                    return mark(ctx, sha256, MediaState::Failed, &reason).await;
                }
            };
            media::set_page_count(&ctx.pool, sha256, page_count)
                .await
                .map_err(JobError::retry)?;
            store(ctx, sha256, &text).await
        }
        Ok(ExecResponse::Image { .. }) => {
            mark(
                ctx,
                sha256,
                MediaState::Failed,
                "unexpected sandbox response",
            )
            .await
        }
        Ok(ExecResponse::Unsupported { reason }) => {
            mark(ctx, sha256, MediaState::Unsupported, &reason).await
        }
        Ok(ExecResponse::Failed {
            reason,
            retryable: false,
        }) => mark(ctx, sha256, MediaState::Failed, &reason).await,
        Ok(ExecResponse::Failed {
            reason,
            retryable: true,
        }) => outcome::give_up_or_retry(ctx, job, sha256, Step::Text, reason).await,
        Err(err) => outcome::give_up_or_retry(ctx, job, sha256, Step::Text, err.to_string()).await,
    }
}

/// The text file an untrusted child wrote: a regular file (never a link to
/// something else), read up to the cap. No file: the document has no text.
fn read_sandbox_text(path: &Path) -> io::Result<String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => read_prefix(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{TEXT_FILE} is not a regular file"),
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }
}

async fn store(ctx: &MediaContext, sha256: &str, text: &str) -> Result<(), JobError> {
    // `false`: collected meanwhile, nothing left to record.
    documents::store_text(&ctx.pool, sha256, text, None)
        .await
        .map(|_| ())
        .map_err(JobError::retry)
}

async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    outcome::mark(ctx, sha256, Step::Text, state, reason).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_cut_by_the_cap_is_dropped() {
        let mut bytes = vec![b'a'; MAX_BODY_BYTES - 1];
        bytes.extend_from_slice("é".as_bytes());
        let text = decode_prefix(&bytes);
        assert_eq!(text.len(), MAX_BODY_BYTES - 1);
        assert!(!text.contains('\u{FFFD}'));
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(decode_prefix(b"ok \xFF ok"), "ok \u{FFFD} ok");
        assert_eq!(decode_prefix(b""), "");
    }
}
