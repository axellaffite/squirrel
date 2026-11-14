use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgPool};
use squirrel_core::{ContentHash, NodeKind, NodeName};
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::{DbError, Page};

/// Where a folder listing resumes: after this (kind, name) in listing order
/// (folders first, then names in byte order). Callers treat it as opaque and
/// encode it as they like; the query compares `kind == File` as a boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCursor {
    pub kind: NodeKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct PathSegment {
    pub id: Uuid,
    pub name: String,
}

/// Ancestor walks stop here, so a corrupted cycle cannot loop forever.
const MAX_DEPTH: i32 = 1000;

pub async fn find_live<'e>(
    executor: impl PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(executor)
        .await?)
}

pub async fn find_live_folder<'e>(
    executor: impl PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    Ok(find_live(executor, owner_id, id)
        .await?
        .filter(NodeRecord::is_folder))
}

/// The owner's live file named exactly `name` in `parent_id` holding `blob`:
/// the same file at the same place (a repeated or retried upload).
pub async fn find_live_file<'e>(
    executor: impl PgExecutor<'e>,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
    blob: &ContentHash,
) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND parent_id = $2 AND name = $3 AND blob_sha256 = $4 \
           AND kind = 'file' AND deleted_at IS NULL \
         LIMIT 1"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(name.as_str())
        .bind(blob.as_str())
        .fetch_optional(executor)
        .await?)
}

/// Live children of `parent_id` (the roots when `None`), after `after`, in
/// listing order: folders first, then names in byte order.
///
/// Only rows owned by `owner_id` are returned, so a foreign, missing or trashed
/// parent simply yields an empty page: the API must check the parent itself
/// (with [`find_live_folder`]) to answer 404.
pub async fn list_children(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Option<Uuid>,
    after: Option<&ListCursor>,
    limit: u32,
) -> Result<Page<NodeRecord>, DbError> {
    let parent_filter = if parent_id.is_some() {
        "parent_id = $2"
    } else {
        "parent_id IS NULL AND $2::uuid IS NULL"
    };
    // A separate statement per case keeps the row comparison an Index Cond on
    // nodes_children_idx (an `$3 IS NULL OR …` would turn it into a Filter).
    let keyset = if after.is_some() {
        "AND ((kind = 'file'), name COLLATE \"C\") > ($4::bool, $5::text COLLATE \"C\")"
    } else {
        "AND $4::bool IS NULL AND $5::text IS NULL"
    };
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND {parent_filter} AND deleted_at IS NULL {keyset} \
         ORDER BY (kind = 'file'), name COLLATE \"C\" \
         LIMIT $3"
    );
    let rows = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(i64::from(limit) + 1)
        .bind(after.map(|c| c.kind == NodeKind::File))
        .bind(after.map(|c| c.name.as_str()))
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// Ancestors of `id`, top-level folder first, excluding `id` itself.
pub async fn path_to(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<Vec<PathSegment>, DbError> {
    Ok(sqlx::query_as::<_, PathSegment>(
        "WITH RECURSIVE chain (id, parent_id, name, depth) AS ( \
             SELECT id, parent_id, name, 0 FROM nodes WHERE id = $1 AND owner_id = $2 \
           UNION ALL \
             SELECT n.id, n.parent_id, n.name, c.depth + 1 \
             FROM nodes n JOIN chain c ON n.id = c.parent_id \
             WHERE n.owner_id = $2 AND c.depth < $3 \
         ) \
         SELECT id, name FROM chain WHERE id <> $1 ORDER BY depth DESC",
    )
    .bind(id)
    .bind(owner_id)
    .bind(MAX_DEPTH)
    .fetch_all(pool)
    .await?)
}
