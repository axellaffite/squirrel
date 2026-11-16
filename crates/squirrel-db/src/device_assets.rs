//! Which phone asset became which content (`device_assets`), and the sync
//! check: which of a phone's assets the server is missing. Owner-scoped: the
//! library lookup only ever reads the caller's own nodes, so it cannot reveal
//! another user's content.

use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::DbError;

/// One asset of a sync check: the phone's id and the content's SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncItem<'a> {
    pub device_asset_id: &'a str,
    pub sha256: &'a str,
}

/// The check, over the batch unnested from `$3` (ids) and `$4` (hashes): is
/// the id recorded for this device (`$1`) with this hash, which node of the
/// owner (`$2`), live or trashed, holds this content, and did the owner delete
/// this content for good after a phone backed it up (`deleted_hashes`).
#[doc(hidden)]
pub const CHECK_SQL: &str = "\
    SELECT i.device_asset_id, i.sha256, \
           EXISTS (SELECT 1 FROM device_assets d \
                    WHERE d.device_id = $1 AND d.device_asset_id = i.device_asset_id \
                      AND d.sha256 = i.sha256) AS recorded, \
           (SELECT n.id FROM nodes n \
             WHERE n.owner_id = $2 AND n.blob_sha256 = i.sha256 LIMIT 1) AS library_node, \
           EXISTS (SELECT 1 FROM deleted_hashes t \
                    WHERE t.owner_id = $2 AND t.sha256 = i.sha256) AS deleted \
      FROM unnest($3::text[], $4::text[]) AS i (device_asset_id, sha256)";

#[derive(Debug, FromRow)]
struct Checked {
    device_asset_id: String,
    sha256: String,
    recorded: bool,
    library_node: Option<Uuid>,
    deleted: bool,
}

/// The ids of `items` the server does not have: neither recorded for this
/// device with this hash, nor content the owner already holds (live or in
/// the trash), nor content the owner deleted for good after a phone backed it
/// up (never uploaded again, whichever device asks). Items found in the
/// library or among the deleted are recorded for the device in the same
/// transaction, so the next check answers from `device_assets`. An id
/// recorded with another hash (an edited photo) is missing again.
pub async fn missing(
    pool: &PgPool,
    device_id: Uuid,
    owner_id: Uuid,
    items: &[SyncItem<'_>],
) -> Result<Vec<String>, DbError> {
    let ids: Vec<&str> = items.iter().map(|item| item.device_asset_id).collect();
    let hashes: Vec<&str> = items.iter().map(|item| item.sha256).collect();
    let mut tx = pool.begin().await?;
    let checked: Vec<Checked> = sqlx::query_as(CHECK_SQL)
        .bind(device_id)
        .bind(owner_id)
        .bind(&ids)
        .bind(&hashes)
        .fetch_all(&mut *tx)
        .await?;
    let mut missing = Vec::new();
    let mut found = Vec::new();
    for item in checked {
        match (item.recorded, item.library_node, item.deleted) {
            (true, _, _) => {}
            (false, Some(node_id), _) => {
                found.push((item.device_asset_id, item.sha256, Some(node_id)))
            }
            (false, None, true) => found.push((item.device_asset_id, item.sha256, None)),
            (false, None, false) => missing.push(item.device_asset_id),
        }
    }
    record_many(&mut tx, device_id, &found).await?;
    tx.commit().await?;
    Ok(missing)
}

/// Records `(device_asset_id, sha256, node_id)` rows in one statement, in
/// `device_asset_id` order: overlapping checks of one device lock the same
/// rows in the same order, so they cannot deadlock.
async fn record_many(
    conn: &mut PgConnection,
    device_id: Uuid,
    rows: &[(String, String, Option<Uuid>)],
) -> Result<(), DbError> {
    if rows.is_empty() {
        return Ok(());
    }
    let ids: Vec<&str> = rows.iter().map(|(id, _, _)| id.as_str()).collect();
    let hashes: Vec<&str> = rows.iter().map(|(_, sha, _)| sha.as_str()).collect();
    let nodes: Vec<Option<Uuid>> = rows.iter().map(|(_, _, node)| *node).collect();
    sqlx::query(
        "INSERT INTO device_assets (device_id, device_asset_id, sha256, node_id) \
         SELECT d.id, r.device_asset_id, r.sha256, r.node_id \
           FROM devices d, \
                unnest($2::text[], $3::text[], $4::uuid[]) AS r (device_asset_id, sha256, node_id) \
          WHERE d.id = $1 \
          ORDER BY r.device_asset_id COLLATE \"C\" \
            FOR KEY SHARE OF d \
         ON CONFLICT (device_id, device_asset_id) DO UPDATE \
            SET sha256 = EXCLUDED.sha256, node_id = EXCLUDED.node_id, synced_at = now()",
    )
    .bind(device_id)
    .bind(&ids)
    .bind(&hashes)
    .bind(&nodes)
    .execute(conn)
    .await?;
    Ok(())
}

/// Records (or updates) that the device's asset is this content, held by
/// `node_id`. A device deleted meanwhile records nothing: the device row is
/// locked (`FOR KEY SHARE`) instead of relying on the foreign key, whose
/// violation would abort the caller's whole transaction.
pub async fn record(
    conn: &mut PgConnection,
    device_id: Uuid,
    device_asset_id: &str,
    sha256: &str,
    node_id: Option<Uuid>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO device_assets (device_id, device_asset_id, sha256, node_id) \
         SELECT d.id, $2, $3, $4 FROM devices d WHERE d.id = $1 FOR KEY SHARE \
         ON CONFLICT (device_id, device_asset_id) DO UPDATE \
            SET sha256 = EXCLUDED.sha256, node_id = EXCLUDED.node_id, synced_at = now()",
    )
    .bind(device_id)
    .bind(device_asset_id)
    .bind(sha256)
    .bind(node_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// A recorded asset (tests and support).
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DeviceAsset {
    pub device_asset_id: String,
    pub sha256: String,
    pub node_id: Option<Uuid>,
}

/// Internal (no owner check): what a device has recorded, by id.
#[doc(hidden)]
pub async fn list_for_device(pool: &PgPool, device_id: Uuid) -> Result<Vec<DeviceAsset>, DbError> {
    Ok(sqlx::query_as::<_, DeviceAsset>(
        "SELECT device_asset_id, sha256, node_id FROM device_assets \
          WHERE device_id = $1 ORDER BY device_asset_id",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?)
}
