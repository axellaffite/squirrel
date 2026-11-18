//! Job kinds enqueued across crates. Light on purpose: `squirrel-api` enqueues
//! media work without depending on the media crates.

use serde_json::json;
use uuid::Uuid;

use crate::{NewJob, priority};

pub const PROCESS_MEDIA: &str = "process_media";
/// Queues `faces` for a user's photos that have no scan (turning faces on).
pub const FACE_BACKFILL: &str = "face_backfill";
/// Clusters a user's new faces (Phase 5).
pub const CLUSTER_FACES: &str = "cluster_faces";

/// One page of a user's face backfill, after node `after` (`None`: the first).
pub fn face_backfill(owner: Uuid, after: Option<Uuid>) -> NewJob {
    let key = after.map_or_else(
        || format!("{FACE_BACKFILL}:{owner}"),
        |id| format!("{FACE_BACKFILL}:{owner}:{id}"),
    );
    NewJob::new(FACE_BACKFILL, json!({ "owner_id": owner, "after": after }))
        .priority(priority::background(priority::FACES))
        .dedupe(key)
}

/// Classify a stored file and queue its media work. `background`: from backfill.
pub fn process_media(node_id: Uuid, background: bool) -> NewJob {
    let base = if background {
        priority::background(priority::INTERACTIVE)
    } else {
        priority::INTERACTIVE
    };
    NewJob::new(
        PROCESS_MEDIA,
        json!({ "node_id": node_id, "background": background }),
    )
    .priority(base)
    .dedupe(format!("{PROCESS_MEDIA}:{node_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_uploads_come_before_backfill() {
        let id = Uuid::now_v7();
        let fresh = process_media(id, false);
        let old = process_media(id, true);
        assert_eq!(fresh.priority, priority::INTERACTIVE);
        assert!(old.priority < fresh.priority);
        assert_eq!(fresh.dedupe_key, Some(format!("process_media:{id}")));
        assert_eq!(fresh.payload["node_id"], json!(id));
    }
}
