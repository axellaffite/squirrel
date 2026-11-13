use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use uuid::Uuid;

use super::find_entry;
use crate::DbError;
use crate::nodes::{self, NodeRecord, RESTORE_FALLBACK_ROOT};
use crate::quota::{self, Pending};

/// Used when nothing valid is left of a stored name.
const FALLBACK_NAME: &str = "Restored item";

/// Restores an entry with its whole batch: back into its folder if that is
/// live, else into `Files/`; a clashing name becomes "name (n)". The batch
/// must fit the quota.
pub async fn restore(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await?;
    let entry = find_entry(&mut tx, owner_id, id)
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
    quota::ensure_room(&mut tx, owner_id, quota_bytes, batch_bytes, Pending::Ignore).await?;

    let parent_id = restore_target(&mut tx, owner_id, entry.parent_id).await?;
    let stored = stored_name(entry.id, &entry.name);
    let name = nodes::free_name(&mut tx, owner_id, parent_id, &stored).await?;
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
    tx.commit().await?;
    Ok(node)
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
