//! The trash. Deleting marks a node and its live subtree with
//! `trash_root = <node id>`; a trash entry is a node where `trash_root = id`.

use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::nodes::{self, NODE_COLUMNS, NodeRecord, RESTORE_FALLBACK_ROOT};
use crate::quota::{self, Pending};
use crate::{DbError, Page};

/// Where a trash listing resumes (entries are ordered by `deleted_at DESC, id DESC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashCursor {
    pub deleted_at: OffsetDateTime,
    pub id: Uuid,
}

/// Moves a live, non-root node and its live subtree to the trash as one entry.
pub async fn trash(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    quota::lock_owner(&mut tx, owner_id).await?;
    let node = nodes::find_live(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    if node.is_root() {
        return Err(DbError::ProtectedRoot);
    }
    sqlx::query(
        "WITH RECURSIVE subtree (id) AS ( \
             SELECT $1::uuid \
           UNION \
             SELECT n.id FROM nodes n JOIN subtree s ON n.parent_id = s.id \
             WHERE n.deleted_at IS NULL \
         ) \
         UPDATE nodes SET deleted_at = now(), trash_root = $1 \
         WHERE owner_id = $2 AND deleted_at IS NULL AND id IN (SELECT id FROM subtree)",
    )
    .bind(id)
    .bind(owner_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Trash entries (what the user deleted, not their contents), newest first.
pub async fn list(
    pool: &PgPool,
    owner_id: Uuid,
    after: Option<&TrashCursor>,
    limit: u32,
) -> Result<Page<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND trash_root = id \
           AND ($2::timestamptz IS NULL OR (deleted_at, id) < ($2, $3)) \
         ORDER BY deleted_at DESC, id DESC LIMIT $4"
    );
    let rows = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(after.map(|c| c.deleted_at))
        .bind(after.map(|c| c.id))
        .bind(i64::from(limit) + 1)
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

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
    let stored = NodeName::parse(&entry.name)
        .map_err(|err| DbError::Sqlx(sqlx::Error::Decode(Box::new(err))))?;
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

async fn find_entry(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND owner_id = $2 AND trash_root = id"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(conn)
        .await?)
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

/// Deletes an entry and everything below it for good. Blob references drop
/// through the trigger; blob GC removes the files later.
pub async fn delete_forever(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let result =
        sqlx::query("DELETE FROM nodes WHERE id = $1 AND owner_id = $2 AND trash_root = id")
            .bind(id)
            .bind(owner_id)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}

/// Deletes every trash entry of the owner; returns how many entries went.
pub async fn empty(pool: &PgPool, owner_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM nodes WHERE owner_id = $1 AND trash_root = id")
        .bind(owner_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Deletes entries trashed before `cutoff`, `batch` at a time. The outer
/// `trash_root = id` re-check matters: a row restored while the statement waits
/// on its lock no longer matches and survives.
pub async fn purge_older_than(
    pool: &PgPool,
    cutoff: OffsetDateTime,
    batch: i64,
) -> Result<u64, DbError> {
    let mut total = 0;
    loop {
        let purged = sqlx::query(
            "DELETE FROM nodes WHERE trash_root = id AND deleted_at < $1 AND id IN ( \
                 SELECT id FROM nodes WHERE trash_root = id AND deleted_at < $1 LIMIT $2)",
        )
        .bind(cutoff)
        .bind(batch)
        .execute(pool)
        .await?
        .rows_affected();
        total += purged;
        if purged < u64::try_from(batch).unwrap_or(u64::MAX) {
            return Ok(total);
        }
    }
}
