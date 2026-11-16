//! `extract_text`: the searchable text of a document blob. Plain text is read
//! here (bounded, no parsing); PDFs go through pdfium in the sandbox, usually
//! within their `thumbnails` job (see [`crate::pdf_job`]).

use std::fs::File;
use std::io::{self, Read};

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::documents::MAX_BODY_BYTES;
use squirrel_db::media::{self, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};

use crate::context::MediaContext;
use crate::jobs::ShaPayload;
use crate::outcome;
use crate::pdf_job::{self, PdfSteps};

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
    match media.kind() {
        MediaKind::Image => Ok(()),
        MediaKind::Text if media.text_state() == MediaState::Ready => Ok(()),
        MediaKind::Text => plain_text(ctx, &job, &sha256, &hash).await,
        // Already stored, or settled otherwise.
        MediaKind::Pdf if media.text_state() != MediaState::Pending => Ok(()),
        // The pending thumbnails job reads the text in the same pass.
        MediaKind::Pdf if media.thumbs_state() == MediaState::Pending => Ok(()),
        MediaKind::Pdf => {
            let steps = PdfSteps {
                render: false,
                text: true,
            };
            pdf_job::run(ctx, &job, &hash, steps).await
        }
    }
}

async fn plain_text(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    hash: &ContentHash,
) -> Result<(), JobError> {
    let path = ctx.storage.blob_path(hash);
    let read = tokio::task::spawn_blocking(move || read_prefix(File::open(path)?))
        .await
        .map_err(JobError::retry)?;
    match read {
        Ok(text) => pdf_job::store_text(ctx, job, sha256, &text, None).await,
        Err(err) => outcome::give_up_or_retry(ctx, job, sha256, Step::Text, err.to_string()).await,
    }
}

/// The first [`MAX_BODY_BYTES`] of `file` as text: invalid UTF-8 replaced, a
/// character cut at the end dropped.
pub fn read_prefix(file: impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    file.take((MAX_BODY_BYTES + CHAR_SLACK) as u64)
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
