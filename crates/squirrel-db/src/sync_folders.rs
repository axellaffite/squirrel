//! A phone's backup folders: `Photos/<device name>/<year>`, created on demand.

use sqlx::PgConnection;
use sqlx::PgPool;
use squirrel_core::NodeName;
use uuid::Uuid;

use crate::DbError;
use crate::nodes::{find_live_folder, free_name};
use crate::quota::lock_owner;

/// Where the device's photos of `year` go: `Photos/<device folder>/<year>`.
/// The device folder is the one recorded on the device while it is live (the
/// user may have renamed or moved it); otherwise the live folder named
/// `device_folder` under `Photos` (one the user made is reused), else a new
/// one, which is recorded. One transaction under the owner lock, so concurrent
/// calls create each folder once. Returns the year folder's id.
pub async fn ensure_year_folder(
    pool: &PgPool,
    owner_id: Uuid,
    device_id: Uuid,
    device_folder: &NodeName,
    year: &NodeName,
) -> Result<Uuid, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner_id).await?;
    let recorded: Option<Uuid> = sqlx::query_scalar(
        "SELECT backup_folder_id FROM devices WHERE id = $1 AND owner_id = $2 FOR UPDATE",
    )
    .bind(device_id)
    .bind(owner_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::NotFound)?;
    let live = match recorded {
        Some(id) => find_live_folder(&mut *tx, owner_id, id)
            .await?
            .map(|f| f.id),
        None => None,
    };
    let device_folder_id = match live {
        Some(id) => id,
        None => {
            let photos = photos_root(&mut tx, owner_id).await?;
            let id = child_folder(&mut tx, owner_id, photos, device_folder).await?;
            crate::devices::set_backup_folder(&mut *tx, device_id, id).await?;
            id
        }
    };
    let year_id = child_folder(&mut tx, owner_id, device_folder_id, year).await?;
    tx.commit().await?;
    Ok(year_id)
}

async fn photos_root(conn: &mut PgConnection, owner_id: Uuid) -> Result<Uuid, DbError> {
    sqlx::query_scalar(
        "SELECT id FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND name = 'Photos' \
           AND kind = 'folder' AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .fetch_optional(conn)
    .await?
    .ok_or(DbError::NotFound)
}

/// The live folder `name` in `parent_id`, created if missing (under a free
/// variant of the name if a file holds it). The caller holds the owner lock.
async fn child_folder(
    conn: &mut PgConnection,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<Uuid, DbError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM nodes WHERE owner_id = $1 AND parent_id = $2 AND name = $3 \
           AND kind = 'folder' AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .bind(parent_id)
    .bind(name.as_str())
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let name = free_name(conn, owner_id, parent_id, name).await?;
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
         VALUES ($1, $2, $3, $4, 'folder') RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(owner_id)
    .bind(parent_id)
    .bind(name.as_str())
    .fetch_one(conn)
    .await
    .map_err(crate::error::name_conflict_or)
}
