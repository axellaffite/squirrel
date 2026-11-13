use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord, find_live, find_live_folder};
use crate::DbError;
use crate::error::name_conflict_or;
use crate::quota::lock_owner;

/// A rename and/or a move; `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct NodeChange {
    pub name: Option<NodeName>,
    pub parent_id: Option<Uuid>,
}

pub async fn create_folder(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner_id).await?;
    find_live_folder(&mut *tx, owner_id, parent_id)
        .await?
        .ok_or(DbError::NotFound)?;
    let sql = format!(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
         VALUES ($1, $2, $3, $4, 'folder') RETURNING {NODE_COLUMNS}"
    );
    let node = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(owner_id)
        .bind(parent_id)
        .bind(name.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(name_conflict_or)?;
    tx.commit().await?;
    Ok(node)
}

/// Renames and/or moves a live, non-root node. A move target must be a live
/// folder of the same owner that is neither the node nor one of its descendants.
pub async fn update(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    change: &NodeChange,
) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner_id).await?;
    let node = find_live(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    let Some(current_parent) = node.parent_id else {
        return Err(DbError::ProtectedRoot);
    };
    let parent_id = match change.parent_id {
        Some(target) if target != current_parent => {
            find_live_folder(&mut *tx, owner_id, target)
                .await?
                .ok_or(DbError::NotFound)?;
            if is_self_or_ancestor(&mut tx, id, target).await? {
                return Err(DbError::MoveIntoSelf);
            }
            target
        }
        _ => current_parent,
    };
    let name = change
        .name
        .as_ref()
        .map_or(node.name.as_str(), NodeName::as_str);
    let sql = format!(
        "UPDATE nodes SET name = $3, parent_id = $4 WHERE id = $1 AND owner_id = $2 \
         RETURNING {NODE_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(name)
        .bind(parent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(name_conflict_or)?;
    tx.commit().await?;
    Ok(updated)
}

/// Whether `node` is `candidate` or one of its ancestors (moving `node` into
/// `candidate` would create a cycle). `UNION` stops on revisits.
async fn is_self_or_ancestor(
    conn: &mut PgConnection,
    node: Uuid,
    candidate: Uuid,
) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up (id, parent_id) AS ( \
             SELECT id, parent_id FROM nodes WHERE id = $2 \
           UNION \
             SELECT n.id, n.parent_id FROM nodes n JOIN up ON n.id = up.parent_id \
         ) \
         SELECT EXISTS (SELECT 1 FROM up WHERE id = $1)",
    )
    .bind(node)
    .bind(candidate)
    .fetch_one(conn)
    .await?)
}
