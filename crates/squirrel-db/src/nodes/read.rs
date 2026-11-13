use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgPool};
use squirrel_core::NodeKind;
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::{DbError, Page};

/// Where a folder listing resumes: after this (kind, name) in listing order
/// (folders first, then names in byte order).
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

/// Live children of `parent_id` (the roots when `None`), after `after`.
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
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND {parent_filter} AND deleted_at IS NULL \
           AND ($3::text IS NULL OR kind < $3 OR (kind = $3 AND name COLLATE \"C\" > $4)) \
         ORDER BY kind DESC, name COLLATE \"C\" \
         LIMIT $5"
    );
    let rows = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(after.map(|c| c.kind.as_str()))
        .bind(after.map(|c| c.name.as_str()))
        .bind(i64::from(limit) + 1)
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
