//! Upload sessions: which file is being uploaded where, and which chunks arrived.
//! The bytes themselves live in the blob store's upload area.

use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgPool};
use squirrel_core::{ContentHash, NodeName};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::nodes::find_live_folder;
use crate::quota::{self, Pending};

const UPLOAD_COLUMNS: &str = "id, owner_id, parent_id, name, sha256, size, chunk_size, \
     chunk_count, status, created_at, expires_at";
const STATUS_OPEN: &str = "open";
/// A claim keeps the session at least this long, so a slow completion (hashing
/// a large file) never sees its session expire under it.
const CLAIM_MIN_TTL: &str = "6 hours";
/// A `completing` session whose claim expired this long ago was abandoned
/// (crash mid-completion); only then may cleanup remove it.
const STALE_CLAIM_GRACE: &str = "1 day";
/// A claim older than this was abandoned (crash, panic, cancelled completion):
/// the session may be claimed again. In-process, the API's per-upload lock
/// keeps a live completion from being claimed twice whatever its duration.
const CLAIM_LEASE: &str = "10 minutes";

#[derive(Debug, Clone, FromRow)]
pub struct UploadRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: String,
    pub sha256: String,
    pub size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    status: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

impl UploadRecord {
    /// `false` while a request is completing the upload.
    pub fn is_open(&self) -> bool {
        self.status == STATUS_OPEN
    }
}

#[derive(Debug, Clone)]
pub struct NewUpload<'a> {
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: &'a NodeName,
    pub sha256: &'a ContentHash,
    pub size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    pub expires_at: OffsetDateTime,
    /// The owner may have at most this many unexpired sessions.
    pub max_open: i64,
}

/// Why [`record_chunk`] did or did not record a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkWrite {
    Recorded,
    /// The session is being completed.
    Busy,
    /// The session is gone: completed, aborted or expired.
    Gone,
}

/// Opens a session after checking the destination, the owner's number of open
/// sessions, and that the file fits the quota together with those sessions.
pub async fn create(pool: &PgPool, new: &NewUpload<'_>) -> Result<UploadRecord, DbError> {
    let mut tx = pool.begin().await?;
    let quota_bytes = quota::lock_owner(&mut tx, new.owner_id).await?;
    find_live_folder(&mut *tx, new.owner_id, new.parent_id)
        .await?
        .ok_or(DbError::NotFound)?;
    // Serialized per owner by `lock_owner`.
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM upload_sessions WHERE owner_id = $1 AND expires_at > now()",
    )
    .bind(new.owner_id)
    .fetch_one(&mut *tx)
    .await?;
    if open >= new.max_open {
        return Err(DbError::TooManyUploads);
    }
    quota::ensure_room(
        &mut tx,
        new.owner_id,
        quota_bytes,
        new.size,
        Pending::Reserve,
    )
    .await?;
    let sql = format!(
        "INSERT INTO upload_sessions \
           (id, owner_id, parent_id, name, sha256, size, chunk_size, chunk_count, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {UPLOAD_COLUMNS}"
    );
    let record = sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.owner_id)
        .bind(new.parent_id)
        .bind(new.name.as_str())
        .bind(new.sha256.as_str())
        .bind(new.size)
        .bind(new.chunk_size)
        .bind(new.chunk_count)
        .bind(new.expires_at)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(record)
}

/// The owner's newest unexpired session for exactly this file and destination,
/// open or being completed (a client resuming after a dropped completion gets
/// the session back and may complete it again).
pub async fn find_resumable(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
    sha256: &ContentHash,
    size: i64,
) -> Result<Option<UploadRecord>, DbError> {
    let sql = format!(
        "SELECT {UPLOAD_COLUMNS} FROM upload_sessions \
         WHERE owner_id = $1 AND parent_id = $2 AND name = $3 AND sha256 = $4 AND size = $5 \
           AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1"
    );
    Ok(sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(name.as_str())
        .bind(sha256.as_str())
        .bind(size)
        .fetch_optional(pool)
        .await?)
}

/// An unexpired session of the owner, in any state.
pub async fn find(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<UploadRecord>, DbError> {
    let sql = format!(
        "SELECT {UPLOAD_COLUMNS} FROM upload_sessions \
         WHERE id = $1 AND owner_id = $2 AND expires_at > now()"
    );
    Ok(sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn received_chunks(pool: &PgPool, upload_id: Uuid) -> Result<Vec<i32>, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT chunk_index FROM upload_chunks WHERE upload_id = $1 ORDER BY chunk_index",
    )
    .bind(upload_id)
    .fetch_all(pool)
    .await?)
}

/// Marks a chunk as received and pushes the expiry back. [`ChunkWrite::Busy`]
/// while the session is being completed, [`ChunkWrite::Gone`] when it is
/// missing or expired; `InvalidInput` when `index` is outside `0..chunk_count`.
pub async fn record_chunk(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    index: i32,
    expires_at: OffsetDateTime,
) -> Result<ChunkWrite, DbError> {
    let mut tx = pool.begin().await?;
    let chunk_count: Option<i32> = sqlx::query_scalar(
        "UPDATE upload_sessions SET expires_at = $3 \
         WHERE id = $1 AND owner_id = $2 AND status = 'open' AND expires_at > now() \
         RETURNING chunk_count",
    )
    .bind(id)
    .bind(owner_id)
    .bind(expires_at)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(chunk_count) = chunk_count else {
        drop(tx);
        return Ok(match find(pool, owner_id, id).await? {
            Some(_) => ChunkWrite::Busy,
            None => ChunkWrite::Gone,
        });
    };
    if !(0..chunk_count).contains(&index) {
        return Err(DbError::InvalidInput("chunk index out of range"));
    }
    sqlx::query(
        "INSERT INTO upload_chunks (upload_id, chunk_index) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(index)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ChunkWrite::Recorded)
}

/// Forgets every received chunk (the upload file was recreated empty).
pub async fn clear_chunks(pool: &PgPool, upload_id: Uuid) -> Result<(), DbError> {
    sqlx::query("DELETE FROM upload_chunks WHERE upload_id = $1")
        .bind(upload_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bytes that unexpired sessions (all owners) have declared but not yet
/// written: what their sparse files may still claim on disk. A received chunk
/// counts as `chunk_size` bytes, so the short last chunk is slightly
/// over-counted as written.
pub async fn pending_bytes(pool: &PgPool) -> Result<i64, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(GREATEST(s.size - COALESCE(c.n, 0) * s.chunk_size, 0)), 0)::BIGINT \
           FROM upload_sessions s \
           LEFT JOIN (SELECT upload_id, count(*) AS n FROM upload_chunks GROUP BY upload_id) c \
             ON c.upload_id = s.id \
          WHERE s.expires_at > now()",
    )
    .fetch_one(pool)
    .await?)
}

/// Moves an open session to `completing`, so concurrent completes, chunk writes
/// and aborts are refused. A `completing` session whose claim is older than
/// [`CLAIM_LEASE`] (its completion died) is claimed again. The session then
/// expires at `expires_at` but no sooner than [`CLAIM_MIN_TTL`] from now.
/// `NotFound` or `UploadBusy` otherwise.
pub async fn claim(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    expires_at: OffsetDateTime,
) -> Result<UploadRecord, DbError> {
    let sql = format!(
        "UPDATE upload_sessions SET status = 'completing', claimed_at = now(), \
           expires_at = GREATEST($3, now() + $4::interval) \
         WHERE id = $1 AND owner_id = $2 AND expires_at > now() \
           AND (status = 'open' OR claimed_at IS NULL OR claimed_at <= now() - $5::interval) \
         RETURNING {UPLOAD_COLUMNS}"
    );
    let claimed = sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(expires_at)
        .bind(CLAIM_MIN_TTL)
        .bind(CLAIM_LEASE)
        .fetch_optional(pool)
        .await?;
    match claimed {
        Some(record) => Ok(record),
        None if find(pool, owner_id, id).await?.is_some() => Err(DbError::UploadBusy),
        None => Err(DbError::NotFound),
    }
}

/// Reopens a claimed session of the owner (after a failure the client can fix
/// and retry) and sets its expiry. `false` if there was nothing to release.
pub async fn release(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    expires_at: OffsetDateTime,
) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE upload_sessions SET status = 'open', claimed_at = NULL, expires_at = $3 \
         WHERE id = $1 AND owner_id = $2 AND status = 'completing'",
    )
    .bind(id)
    .bind(owner_id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Deletes a session in any state; accepts a transaction (completion).
pub async fn delete<'e>(executor: impl PgExecutor<'e>, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM upload_sessions WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Aborts an open session of the owner; `UploadBusy` while it is completing.
pub async fn abort(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let deleted: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM upload_sessions WHERE id = $1 AND owner_id = $2 AND status = 'open' RETURNING id",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    match deleted {
        Some(_) => Ok(()),
        None if find(pool, owner_id, id).await?.is_some() => Err(DbError::UploadBusy),
        None => Err(DbError::NotFound),
    }
}

/// Deletes expired sessions and returns their ids. A `completing` session is
/// skipped (a request is still reading its chunks) unless its claim expired
/// more than [`STALE_CLAIM_GRACE`] ago, which only happens if the completing
/// request died.
pub async fn delete_expired(pool: &PgPool) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar(
        "DELETE FROM upload_sessions WHERE expires_at <= now() \
           AND (status = 'open' OR expires_at <= now() - $1::interval) \
         RETURNING id",
    )
    .bind(STALE_CLAIM_GRACE)
    .fetch_all(pool)
    .await?)
}

/// The subset of `ids` that still has a session (to find orphaned files).
pub async fn existing(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Uuid>, DbError> {
    Ok(
        sqlx::query_scalar("SELECT id FROM upload_sessions WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(pool)
            .await?,
    )
}
