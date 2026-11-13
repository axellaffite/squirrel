use sqlx::PgConnection;
use squirrel_core::{ContentHash, NodeName};
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::DbError;

/// Give up looking for a free "name (n).ext" after this many tries.
const MAX_NAME_VARIANTS: u32 = 1000;

#[derive(Debug, Clone)]
pub struct NewFile<'a> {
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: &'a NodeName,
    pub blob: &'a ContentHash,
    pub size: i64,
    pub mime: &'a str,
}

/// The first free variant of `name` among the live children of `parent_id`
/// (`name`, `name (1)`, …). The caller holds the owner lock, so the answer
/// stays true until it writes.
pub async fn free_name(
    conn: &mut PgConnection,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<NodeName, DbError> {
    for n in 0..MAX_NAME_VARIANTS {
        let candidate = name.variant(n);
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM nodes \
             WHERE owner_id = $1 AND parent_id = $2 AND name = $3 AND deleted_at IS NULL)",
        )
        .bind(owner_id)
        .bind(parent_id)
        .bind(candidate.as_str())
        .fetch_one(&mut *conn)
        .await?;
        if !taken {
            return Ok(candidate);
        }
    }
    Err(DbError::NameConflict)
}

/// Inserts a file node under a free variant of its name. Preconditions: the
/// caller holds the owner lock, checked that the parent is a live folder of the
/// owner, and the blob row exists.
pub async fn insert_file(
    conn: &mut PgConnection,
    file: &NewFile<'_>,
) -> Result<NodeRecord, DbError> {
    let name = free_name(conn, file.owner_id, file.parent_id, file.name).await?;
    let sql = format!(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES ($1, $2, $3, $4, 'file', $5, $6, $7) RETURNING {NODE_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(file.owner_id)
        .bind(file.parent_id)
        .bind(name.as_str())
        .bind(file.blob.as_str())
        .bind(file.size)
        .bind(file.mime)
        .fetch_one(conn)
        .await?)
}
