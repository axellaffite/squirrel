use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use uuid::Uuid;

use super::find_entry;
use crate::DbError;
use crate::faces;
use crate::nodes::{self, NodeRecord, RESTORE_FALLBACK_ROOT};
use crate::quota::{self, Pending};

/// Used when nothing valid is left of a stored name.
const FALLBACK_NAME: &str = "Restored item";

/// Restores an entry with its whole batch: back into its folder if that is
/// live, else into `Files/`; a clashing name becomes "name (n)". The batch
/// must fit the quota.
/// Its own transaction, ignoring `faces_due` (the API uses [`restore_in`]).
pub async fn restore(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    let restored = restore_in(&mut tx, owner_id, id, "").await?;
    tx.commit().await?;
    Ok(restored.node)
}

/// A restored trash entry.
#[derive(Debug)]
pub struct Restored {
    pub node: NodeRecord,
    /// Rendered photos of the batch that the owner (faces on) has no scan of
    /// by the face model: the caller queues `faces` for them in the same
    /// transaction, since the thumbnails step will not run again.
    pub faces_due: Vec<String>,
}

/// [`restore`] in the caller's transaction. `face_model`: the face model
/// whose scans count (`faces_due` lists the photos without one).
pub async fn restore_in(
    tx: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    face_model: &str,
) -> Result<Restored, DbError> {
    let quota_bytes = quota::lock_owner(&mut *tx, owner_id).await?;
    let entry = find_entry(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    let batch_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::bigint FROM nodes \
         WHERE owner_id = $1 AND trash_root = $2 AND kind = 'file'",
    )
    .bind(owner_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    quota::ensure_room(
        &mut *tx,
        owner_id,
        quota_bytes,
        batch_bytes,
        Pending::Ignore,
    )
    .await?;
    let faces_due = faces::unscanned_in_batch(&mut *tx, owner_id, id, face_model).await?;

    let parent_id = restore_target(&mut *tx, owner_id, entry.parent_id).await?;
    let stored = stored_name(entry.id, &entry.name);
    let name = nodes::free_name(&mut *tx, owner_id, parent_id, &stored).await?;
    sqlx::query(
        "UPDATE nodes SET deleted_at = NULL, trash_root = NULL, \
           parent_id = CASE WHEN id = $2 THEN $3 ELSE parent_id END, \
           name = CASE WHEN id = $2 THEN $4 ELSE name END \
         WHERE owner_id = $1 AND trash_root = $2",
    )
    .bind(owner_id)
    .bind(id)
    .bind(parent_id)
    .bind(name.as_str())
    .execute(&mut *tx)
    .await?;
    let node = nodes::find_live(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    Ok(Restored { node, faces_due })
}

/// The original folder if it is still live, else the owner's `Files/` root.
async fn restore_target(
    conn: &mut PgConnection,
    owner_id: Uuid,
    original: Option<Uuid>,
) -> Result<Uuid, DbError> {
    if let Some(parent) = original
        && nodes::find_live_folder(&mut *conn, owner_id, parent)
            .await?
            .is_some()
    {
        return Ok(parent);
    }
    sqlx::query_scalar(
        "SELECT id FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND name = $2 AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .bind(RESTORE_FALLBACK_ROOT)
    .fetch_optional(conn)
    .await?
    .ok_or(DbError::NotFound)
}

/// The stored name, or a sanitized form of it when it no longer passes
/// [`NodeName::parse`] (rows written before a validation rule existed).
fn stored_name(id: Uuid, raw: &str) -> NodeName {
    NodeName::parse(raw).unwrap_or_else(|err| {
        tracing::warn!(node_id = %id, error = %err, "restoring a node with an invalid name");
        sanitize(raw)
    })
}

/// Turns `/` into `_`, drops every character a name may not contain and
/// shortens to the byte limit; falls back to [`FALLBACK_NAME`].
fn sanitize(raw: &str) -> NodeName {
    let kept: String = raw
        .chars()
        .map(|c| if c == '/' { '_' } else { c })
        .filter(|c| NodeName::parse(&format!("a{c}a")).is_ok())
        .collect();
    let mut cut = kept.len().min(NodeName::MAX_BYTES);
    while !kept.is_char_boundary(cut) {
        cut -= 1;
    }
    NodeName::parse(&kept[..cut])
        .unwrap_or_else(|_| NodeName::parse(FALLBACK_NAME).expect("the fallback name is valid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_what_is_allowed() {
        assert_eq!(sanitize("a/b\nc\u{202e}.txt").as_str(), "a_bc.txt");
        assert_eq!(sanitize("..").as_str(), FALLBACK_NAME);
        assert_eq!(sanitize("\u{200b}").as_str(), FALLBACK_NAME);
        assert_eq!(sanitize(&"\u{e9}".repeat(200)).as_str().len(), 254);
    }
}
