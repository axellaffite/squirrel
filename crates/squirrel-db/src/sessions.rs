use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const SESSION_COLUMNS: &str =
    "id, user_id, csrf_token, created_at, last_seen_at, expires_at, absolute_expires_at";

#[derive(Debug, Clone, FromRow)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub csrf_token: String,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub absolute_expires_at: OffsetDateTime,
}

pub struct NewSession<'a> {
    pub user_id: Uuid,
    pub token_hash: &'a [u8],
    pub csrf_token: &'a str,
    pub user_agent: Option<&'a str>,
    /// Clamped to `absolute_expires_at`.
    pub expires_at: OffsetDateTime,
    /// Hard limit: the session ends then however active it is.
    pub absolute_expires_at: OffsetDateTime,
}

pub async fn create(pool: &PgPool, new: &NewSession<'_>) -> Result<SessionRecord, DbError> {
    let sql = format!(
        "INSERT INTO sessions \
           (id, token_hash, user_id, csrf_token, user_agent, expires_at, absolute_expires_at) \
         VALUES ($1, $2, $3, $4, $5, LEAST($6, $7), $7) RETURNING {SESSION_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, SessionRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.token_hash)
        .bind(new.user_id)
        .bind(new.csrf_token)
        .bind(new.user_agent)
        .bind(new.expires_at)
        .bind(new.absolute_expires_at)
        .fetch_one(pool)
        .await?)
}

pub async fn find_active(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<SessionRecord>, DbError> {
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM sessions \
         WHERE token_hash = $1 AND expires_at > now() AND absolute_expires_at > now()"
    );
    Ok(sqlx::query_as::<_, SessionRecord>(&sql)
        .bind(token_hash)
        .fetch_optional(pool)
        .await?)
}

/// Extends a still-active session, never beyond its absolute expiry. Returns
/// `false` when the session no longer exists or has already expired, so callers
/// must treat it as unauthenticated.
pub async fn touch(pool: &PgPool, id: Uuid, expires_at: OffsetDateTime) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE sessions SET last_seen_at = now(), expires_at = LEAST($2, absolute_expires_at) \
         WHERE id = $1 AND expires_at > now() AND absolute_expires_at > now()",
    )
    .bind(id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn delete_by_id(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_by_token_hash(pool: &PgPool, token_hash: &[u8]) -> Result<(), DbError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn delete_expired(pool: &PgPool) -> Result<u64, DbError> {
    let result = sqlx::query(
        "DELETE FROM sessions WHERE expires_at <= now() OR absolute_expires_at <= now()",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
