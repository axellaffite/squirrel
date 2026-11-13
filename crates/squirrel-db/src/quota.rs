//! Storage usage and the per-user lock that serializes tree changes.

use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::DbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct Usage {
    /// Logical size of live files (what the quota limits).
    pub used_bytes: i64,
    /// Logical size of files in the trash (not counted against the quota).
    pub trash_bytes: i64,
    /// `None`: unlimited.
    pub quota_bytes: Option<i64>,
}

/// Whether the owner's open upload sessions count as already used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Ignore,
    /// Used when opening a session, so parallel sessions cannot overcommit.
    Reserve,
}

pub async fn usage(pool: &PgPool, owner_id: Uuid) -> Result<Usage, DbError> {
    sqlx::query_as::<_, Usage>(
        "SELECT u.quota_bytes, \
           COALESCE(SUM(n.size) FILTER (WHERE n.deleted_at IS NULL), 0)::bigint AS used_bytes, \
           COALESCE(SUM(n.size) FILTER (WHERE n.deleted_at IS NOT NULL), 0)::bigint AS trash_bytes \
         FROM users u LEFT JOIN nodes n ON n.owner_id = u.id AND n.kind = 'file' \
         WHERE u.id = $1 GROUP BY u.id",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    .ok_or(DbError::NotFound)
}

/// Locks the owner's row until the transaction ends and returns their quota.
/// Every tree change and quota check takes this lock first.
pub async fn lock_owner(conn: &mut PgConnection, owner_id: Uuid) -> Result<Option<i64>, DbError> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT quota_bytes FROM users WHERE id = $1 FOR UPDATE")
            .bind(owner_id)
            .fetch_optional(&mut *conn)
            .await?;
    row.map(|(quota,)| quota).ok_or(DbError::NotFound)
}

/// Fails with `QuotaExceeded` unless `extra` more bytes fit. Call after
/// [`lock_owner`] in the same transaction, passing the quota it returned.
pub async fn ensure_room(
    conn: &mut PgConnection,
    owner_id: Uuid,
    quota_bytes: Option<i64>,
    extra: i64,
    pending: Pending,
) -> Result<(), DbError> {
    let Some(quota_bytes) = quota_bytes else {
        return Ok(());
    };
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::bigint FROM nodes \
         WHERE owner_id = $1 AND kind = 'file' AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .fetch_one(&mut *conn)
    .await?;
    let reserved: i64 = match pending {
        Pending::Ignore => 0,
        Pending::Reserve => {
            sqlx::query_scalar(
                "SELECT COALESCE(SUM(size), 0)::bigint FROM upload_sessions \
                 WHERE owner_id = $1 AND expires_at > now()",
            )
            .bind(owner_id)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if used.saturating_add(reserved).saturating_add(extra) > quota_bytes {
        return Err(DbError::QuotaExceeded);
    }
    Ok(())
}
