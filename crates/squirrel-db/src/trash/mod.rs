//! The trash: each `trash` call makes one entry (`trash_root = id`) whose live
//! subtree joins its batch (`trash_root = entry`). An entry trashed earlier
//! inside that subtree stays a separate entry ("nested entry").

mod delete;
mod restore;

pub use delete::{delete_forever, empty, purge_older_than};
pub use restore::{Restored, restore, restore_in};

use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::nodes::{self, NODE_COLUMNS, NodeRecord};
use crate::quota;
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
