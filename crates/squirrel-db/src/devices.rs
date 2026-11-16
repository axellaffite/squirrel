//! Phones signed in with bearer tokens (Phase 6). Access and refresh tokens
//! are stored as SHA-256 only. Every owner-facing function is scoped by
//! `owner_id`: another user's device behaves exactly like a missing one.

use sqlx::{FromRow, PgConnection, PgPool};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::DbError;

const DEVICE_COLUMNS: &str = "id, owner_id, name, platform, model, app_version, \
     refresh_expires_at, backup_folder_id, created_at, last_seen_at, revoked_at";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DeviceRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    /// `android` or `ios` (CHECK constraint).
    pub platform: String,
    pub model: Option<String>,
    pub app_version: String,
    pub refresh_expires_at: OffsetDateTime,
    /// `Photos/<name>`, once `POST /api/sync/folder` created it.
    pub backup_folder_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

impl DeviceRecord {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// An access token to store (by hash).
#[derive(Debug, Clone, Copy)]
pub struct NewAccess<'a> {
    pub hash: &'a [u8],
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy)]
pub struct NewDevice<'a> {
    pub owner_id: Uuid,
    pub name: &'a str,
    pub platform: &'a str,
    pub model: Option<&'a str>,
    pub app_version: &'a str,
    pub refresh_hash: &'a [u8],
    pub refresh_expires_at: OffsetDateTime,
    pub access: NewAccess<'a>,
}

/// How long after a rotation the previous refresh token is still answered
/// (with the same new token): two refreshes racing, or a lost response.
pub const REFRESH_GRACE: Duration = Duration::seconds(30);

/// A refresh request: the presented token's hash and the replacements. The
/// new refresh token must be derived from the presented one and
/// `rotation_salt`, so a replay within [`REFRESH_GRACE`] can be answered with
/// the same token.
#[derive(Debug, Clone, Copy)]
pub struct Refresh<'a> {
    pub presented: &'a [u8],
    pub new_refresh: &'a [u8],
    pub rotation_salt: &'a [u8],
    pub refresh_expires_at: OffsetDateTime,
    pub access: NewAccess<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rotation {
    /// New tokens were stored for this device.
    Rotated(DeviceRecord),
    /// The previous token, within [`REFRESH_GRACE`] of its rotation: a new
    /// access token was stored, the refresh token was not rotated again. The
    /// current refresh token is the one derived from the presented token and
    /// this salt.
    Replayed {
        device: DeviceRecord,
        rotation_salt: Vec<u8>,
    },
    /// A retired refresh token was presented: the device has been revoked.
    Reused { device_id: Uuid, owner_id: Uuid },
    /// The device's refresh lifetime is over. Nothing changed.
    Expired,
    /// No live device has this refresh token.
    Unknown,
}

/// Creates the device with its first refresh and access tokens.
pub async fn create(pool: &PgPool, new: &NewDevice<'_>) -> Result<DeviceRecord, DbError> {
    let mut tx = pool.begin().await?;
    let sql = format!(
        "INSERT INTO devices (id, owner_id, name, platform, model, app_version, refresh_hash, \
                              refresh_expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {DEVICE_COLUMNS}"
    );
    let device = sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.owner_id)
        .bind(new.name)
        .bind(new.platform)
        .bind(new.model)
        .bind(new.app_version)
        .bind(new.refresh_hash)
        .bind(new.refresh_expires_at)
        .fetch_one(&mut *tx)
        .await?;
    insert_access(&mut tx, device.id, &new.access).await?;
    tx.commit().await?;
    Ok(device)
}

async fn insert_access(
    conn: &mut PgConnection,
    device_id: Uuid,
    access: &NewAccess<'_>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO device_access_tokens (token_hash, device_id, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(access.hash)
    .bind(device_id)
    .bind(access.expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// The live device holding this unexpired access token.
pub async fn find_by_access_hash(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<DeviceRecord>, DbError> {
    let sql = format!(
        "SELECT {} FROM device_access_tokens t JOIN devices d ON d.id = t.device_id \
          WHERE t.token_hash = $1 AND t.expires_at > now() AND d.revoked_at IS NULL",
        prefixed("d")
    );
    Ok(sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(token_hash)
        .fetch_optional(pool)
        .await?)
}

fn prefixed(alias: &str) -> String {
    DEVICE_COLUMNS
        .split(", ")
        .map(|column| format!("{alias}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Rotates a refresh token. Presenting the current token makes it `previous`,
/// retires the one it replaces, and stores the new one. Presenting the
/// previous token within [`REFRESH_GRACE`] of the rotation (a concurrent
/// refresh, or a retry after a lost response) only adds an access token: the
/// caller answers with the same current token ([`Rotation::Replayed`]).
/// Presenting the previous token later, or a retired one, is reuse: the
/// device is revoked. One transaction with the device row locked, so two
/// refreshes with the same token serialise: one rotates, the other replays.
pub async fn rotate_refresh(pool: &PgPool, refresh: &Refresh<'_>) -> Result<Rotation, DbError> {
    let mut tx = pool.begin().await?;
    let sql = format!(
        "SELECT {DEVICE_COLUMNS}, refresh_hash, previous_refresh_hash, rotated_at, rotation_salt \
           FROM devices \
          WHERE refresh_hash = $1 OR previous_refresh_hash = $1 FOR UPDATE"
    );
    let row: Option<RefreshRow> = sqlx::query_as(&sql)
        .bind(refresh.presented)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(row) = row else {
        let reused = reuse(&mut tx, refresh.presented).await?;
        tx.commit().await?;
        return Ok(reused);
    };
    if row.device.is_revoked() {
        return Ok(Rotation::Unknown);
    }
    if row.device.refresh_expires_at <= OffsetDateTime::now_utc() {
        return Ok(Rotation::Expired);
    }
    if row.refresh_hash != refresh.presented {
        // The previous token.
        let rotation = replay(&mut tx, row, refresh).await?;
        tx.commit().await?;
        return Ok(rotation);
    }
    if let Some(retired) = row.previous_refresh_hash {
        sqlx::query(
            "INSERT INTO device_retired_refresh (token_hash, device_id) VALUES ($1, $2) \
             ON CONFLICT (token_hash) DO NOTHING",
        )
        .bind(retired)
        .bind(row.device.id)
        .execute(&mut *tx)
        .await?;
    }
    let sql = format!(
        "UPDATE devices SET refresh_hash = $2, previous_refresh_hash = $3, \
                refresh_expires_at = $4, rotated_at = now(), rotation_salt = $5, \
                last_seen_at = now() \
          WHERE id = $1 RETURNING {DEVICE_COLUMNS}"
    );
    let device = sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(row.device.id)
        .bind(refresh.new_refresh)
        .bind(row.refresh_hash)
        .bind(refresh.refresh_expires_at)
        .bind(refresh.rotation_salt)
        .fetch_one(&mut *tx)
        .await?;
    insert_access(&mut tx, device.id, &refresh.access).await?;
    tx.commit().await?;
    Ok(Rotation::Rotated(device))
}

#[derive(FromRow)]
struct RefreshRow {
    #[sqlx(flatten)]
    device: DeviceRecord,
    refresh_hash: Vec<u8>,
    previous_refresh_hash: Option<Vec<u8>>,
    rotated_at: Option<OffsetDateTime>,
    rotation_salt: Option<Vec<u8>>,
}

/// The previous token was presented: within the grace window, a new access
/// token and the same current refresh token; after it, reuse.
async fn replay(
    conn: &mut PgConnection,
    row: RefreshRow,
    refresh: &Refresh<'_>,
) -> Result<Rotation, DbError> {
    let now = OffsetDateTime::now_utc();
    match (row.rotated_at, row.rotation_salt) {
        (Some(rotated_at), Some(rotation_salt)) if now - rotated_at <= REFRESH_GRACE => {
            insert_access(conn, row.device.id, &refresh.access).await?;
            Ok(Rotation::Replayed {
                device: row.device,
                rotation_salt,
            })
        }
        _ => {
            revoke_ids(conn, &[row.device.id]).await?;
            Ok(Rotation::Reused {
                device_id: row.device.id,
                owner_id: row.device.owner_id,
            })
        }
    }
}

/// A presented token that is no current or previous one: reuse if retired.
async fn reuse(conn: &mut PgConnection, presented: &[u8]) -> Result<Rotation, DbError> {
    let found: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT d.id, d.owner_id FROM device_retired_refresh r \
           JOIN devices d ON d.id = r.device_id WHERE r.token_hash = $1 FOR UPDATE OF d",
    )
    .bind(presented)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((device_id, owner_id)) = found else {
        return Ok(Rotation::Unknown);
    };
    revoke_ids(conn, &[device_id]).await?;
    Ok(Rotation::Reused {
        device_id,
        owner_id,
    })
}

/// Marks devices revoked (keeping an earlier revocation time) and deletes
/// their access tokens. Returns how many were live.
async fn revoke_ids(conn: &mut PgConnection, ids: &[Uuid]) -> Result<u64, DbError> {
    let revoked = sqlx::query(
        "UPDATE devices SET revoked_at = now() WHERE id = ANY($1) AND revoked_at IS NULL",
    )
    .bind(ids)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM device_access_tokens WHERE device_id = ANY($1)")
        .bind(ids)
        .execute(&mut *conn)
        .await?;
    Ok(revoked)
}

/// The owner's devices: live ones first (most recently seen first), then revoked.
pub async fn list_for_owner(pool: &PgPool, owner_id: Uuid) -> Result<Vec<DeviceRecord>, DbError> {
    let sql = format!(
        "SELECT {DEVICE_COLUMNS} FROM devices WHERE owner_id = $1 \
          ORDER BY revoked_at NULLS FIRST, last_seen_at DESC, id DESC"
    );
    Ok(sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(owner_id)
        .fetch_all(pool)
        .await?)
}

pub async fn find_for_owner(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<DeviceRecord>, DbError> {
    let sql = format!("SELECT {DEVICE_COLUMNS} FROM devices WHERE id = $1 AND owner_id = $2");
    Ok(sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?)
}

/// Revokes one of the owner's devices at once (its access tokens are
/// deleted). Idempotent. `false` when the owner has no such device.
pub async fn revoke(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<bool, DbError> {
    let mut tx = pool.begin().await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM devices WHERE id = $1 AND owner_id = $2)")
            .bind(id)
            .bind(owner_id)
            .fetch_one(&mut *tx)
            .await?;
    if exists {
        revoke_ids(&mut tx, &[id]).await?;
    }
    tx.commit().await?;
    Ok(exists)
}

/// Revokes every live device of the user. Returns how many were live.
pub async fn revoke_all_for_user(pool: &PgPool, owner_id: Uuid) -> Result<u64, DbError> {
    let mut tx = pool.begin().await?;
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM devices WHERE owner_id = $1")
        .bind(owner_id)
        .fetch_all(&mut *tx)
        .await?;
    let revoked = revoke_ids(&mut tx, &ids).await?;
    tx.commit().await?;
    Ok(revoked)
}

/// Records activity (callers throttle it).
pub async fn touch(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE devices SET last_seen_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Renames one of the owner's devices; `None` when the owner has no such device.
pub async fn rename(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    name: &str,
) -> Result<Option<DeviceRecord>, DbError> {
    let sql = format!(
        "UPDATE devices SET name = $3 WHERE id = $1 AND owner_id = $2 RETURNING {DEVICE_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, DeviceRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(name)
        .fetch_optional(pool)
        .await?)
}

/// Records the device's `Photos/<name>` folder. The foreign key refuses a
/// folder of another owner.
pub async fn set_backup_folder<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    id: Uuid,
    folder_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("UPDATE devices SET backup_folder_id = $2 WHERE id = $1")
        .bind(id)
        .bind(folder_id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Deletes expired access tokens.
pub async fn delete_expired_tokens(pool: &PgPool) -> Result<u64, DbError> {
    Ok(
        sqlx::query("DELETE FROM device_access_tokens WHERE expires_at <= now()")
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

/// Deletes devices revoked, or whose refresh lifetime ended, before `cutoff`
/// (their tokens and synced-asset records go with them).
pub async fn delete_stale(pool: &PgPool, cutoff: OffsetDateTime) -> Result<u64, DbError> {
    Ok(
        sqlx::query("DELETE FROM devices WHERE revoked_at < $1 OR refresh_expires_at < $1")
            .bind(cutoff)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

/// Forgets refresh tokens retired before `cutoff` (older than any refresh
/// lifetime: they could not be used anyway).
pub async fn prune_retired(pool: &PgPool, cutoff: OffsetDateTime) -> Result<u64, DbError> {
    Ok(
        sqlx::query("DELETE FROM device_retired_refresh WHERE retired_at < $1")
            .bind(cutoff)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}
