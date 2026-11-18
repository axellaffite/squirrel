//! The per-blob jobs a stored file leads to, and the backfill job.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;
use squirrel_db::media::{MediaRecord, MediaState};
use squirrel_jobs::{NewJob, priority};
use uuid::Uuid;

pub const EXTRACT_METADATA: &str = "extract_metadata";
pub const THUMBNAILS: &str = "thumbnails";
pub const GEOCODE: &str = "geocode";
pub const EXTRACT_TEXT: &str = "extract_text";
pub const BACKFILL: &str = "backfill";
/// Phase 5 (machine learning): registered only when their models are installed.
pub const CLIP_EMBED: &str = "clip_embed";
pub use squirrel_jobs::kinds::{CLUSTER_FACES, FACE_BACKFILL, FACES, faces};
/// Encodes the label bank's prompts with the CLIP text tower.
pub const LABEL_BANK: &str = "label_bank";

/// Kinds that run model inference: they share the worker's ML slots
/// (`--ml-concurrency`). Clustering and face backfill are SQL work.
pub const ML_KINDS: [&str; 3] = [CLIP_EMBED, FACES, LABEL_BANK];

/// Every kind this crate can register, whether or not it is registered now.
pub const KNOWN_KINDS: [&str; 11] = [
    squirrel_jobs::kinds::PROCESS_MEDIA,
    EXTRACT_METADATA,
    THUMBNAILS,
    EXTRACT_TEXT,
    GEOCODE,
    BACKFILL,
    CLIP_EMBED,
    FACES,
    CLUSTER_FACES,
    FACE_BACKFILL,
    LABEL_BANK,
];

/// Payload of every per-blob job.
#[derive(Debug, Deserialize)]
pub struct ShaPayload {
    pub sha256: String,
}

fn per_blob(kind: &'static str, sha256: &str, base: i16, background: bool) -> NewJob {
    let priority = if background {
        priority::background(base)
    } else {
        base
    };
    NewJob::new(kind, json!({ "sha256": sha256 }))
        .priority(priority)
        .dedupe(format!("{kind}:{sha256}"))
}

pub fn extract_metadata(sha256: &str, background: bool) -> NewJob {
    per_blob(EXTRACT_METADATA, sha256, priority::METADATA, background)
}

pub fn thumbnails(sha256: &str, background: bool) -> NewJob {
    per_blob(THUMBNAILS, sha256, priority::THUMBNAILS, background)
}

pub fn geocode(sha256: &str, background: bool) -> NewJob {
    per_blob(GEOCODE, sha256, priority::GEOCODE, background)
}

/// CLIP embedding and tags of an image blob (shared by every owner).
pub fn clip_embed(sha256: &str, background: bool) -> NewJob {
    per_blob(CLIP_EMBED, sha256, priority::CLIP, background)
}

/// Computes the label bank of `model` for the current labels version (one
/// queued at a time per model and version).
pub fn label_bank(model: &str) -> NewJob {
    let version = squirrel_ml::clip::labels::LABELS_VERSION;
    NewJob::new(
        LABEL_BANK,
        json!({ "model": model, "labels_version": version }),
    )
    .priority(priority::LABEL_BANK)
    .dedupe(format!("{LABEL_BANK}:{model}:{version}"))
}

/// Clusters `owner`'s new faces, at most once per [`CLUSTER_DELAY`] while
/// photos keep arriving (the dedupe key collapses the requests).
pub fn cluster_faces(owner: Uuid) -> NewJob {
    NewJob::new(CLUSTER_FACES, json!({ "owner_id": owner }))
        .priority(priority::CLUSTER)
        .dedupe(format!("{CLUSTER_FACES}:{owner}"))
        .delay(CLUSTER_DELAY)
}

/// How long new faces wait before they are clustered.
pub const CLUSTER_DELAY: Duration = Duration::from_secs(10 * 60);

/// The ML steps of an image whose preview was just made. `faces_wanted`:
/// some owner of a live copy has faces on.
pub fn ml_follow_ups(media: &MediaRecord, faces_wanted: bool, background: bool) -> Vec<NewJob> {
    let sha = media.sha256.as_str();
    let mut jobs = Vec::new();
    if media.clip_state() == MediaState::Pending {
        jobs.push(clip_embed(sha, background));
    }
    if faces_wanted && media.faces_state() == MediaState::Pending {
        jobs.push(faces(sha, background));
    }
    jobs
}

pub fn extract_text(sha256: &str, background: bool) -> NewJob {
    per_blob(EXTRACT_TEXT, sha256, priority::TEXT, background)
}

/// One page of the backfill, after node `after` (`None`: the first page).
/// The first page's dedupe key is `backfill`; a follow-up page's is
/// `backfill:<after>`, since the running page still holds `backfill`.
pub fn backfill(after: Option<Uuid>) -> NewJob {
    let key = after.map_or_else(|| BACKFILL.to_owned(), |id| format!("{BACKFILL}:{id}"));
    NewJob::new(BACKFILL, json!({ "after": after }))
        .priority(priority::BACKFILL)
        .dedupe(key)
}

/// Delay before the next backfill page, so a large library does not starve
/// fresh uploads of database time.
pub const BACKFILL_PAGE_DELAY: Duration = Duration::from_secs(2);

/// The steps of `media` still pending, as jobs.
pub fn follow_ups(media: &MediaRecord, background: bool) -> Vec<NewJob> {
    let sha = media.sha256.as_str();
    let pending = |state: MediaState| state == MediaState::Pending;
    let mut jobs = Vec::new();
    if pending(media.metadata_state()) {
        jobs.push(extract_metadata(sha, background));
    }
    if pending(media.thumbs_state()) {
        jobs.push(thumbnails(sha, background));
    }
    if pending(media.text_state()) {
        jobs.push(extract_text(sha, background));
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_work_is_lowered() {
        assert_eq!(thumbnails("ab", false).priority, 80);
        assert_eq!(thumbnails("ab", true).priority, 30);
        assert_eq!(extract_metadata("ab", true).priority, 20);
        assert_eq!(
            geocode("ab", false).dedupe_key.as_deref(),
            Some("geocode:ab")
        );
    }

    #[test]
    fn ml_jobs_come_after_every_media_step() {
        assert_eq!(clip_embed("ab", false).priority, priority::CLIP);
        assert_eq!(
            clip_embed("ab", true).priority,
            priority::background(priority::CLIP)
        );
        assert_eq!(
            clip_embed("ab", false).dedupe_key.as_deref(),
            Some("clip_embed:ab")
        );
        assert_eq!(faces("ab", false).dedupe_key.as_deref(), Some("faces:ab"));
        assert!(clip_embed("ab", false).priority < geocode("ab", true).priority);
    }

    #[test]
    fn backfill_pages_have_distinct_keys() {
        let id = Uuid::now_v7();
        assert_eq!(backfill(None).dedupe_key.as_deref(), Some("backfill"));
        assert_eq!(
            backfill(Some(id)).dedupe_key,
            Some(format!("backfill:{id}"))
        );
        assert_eq!(backfill(Some(id)).payload, json!({ "after": id }));
    }
}
