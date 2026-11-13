//! The blob catalogue. `ref_count` is maintained by a trigger on `nodes`;
//! this module only inserts rows, looks them up and deletes unreferenced ones.

use sqlx::{FromRow, PgConnection, PgPool};
use squirrel_core::ContentHash;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const BLOB_COLUMNS: &str = "sha256, size, mime, ref_count, zero_since, created_at";

#[derive(Debug, Clone, FromRow)]
pub struct BlobRecord {
    pub sha256: String,
    pub size: i64,
    pub mime: String,
    pub ref_count: i64,
    pub zero_since: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

/// Serializes storing and linking a blob with deleting it, until the
/// transaction ends. Take it after `quota::lock_owner`.
pub async fn lock(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(hash.lock_key())
        .execute(conn)
        .await?;
    Ok(())
}

/// Session-level form for GC, which must hold the lock across a commit and the
/// file deletion. Always pair with [`unlock_session`] on the same connection.
pub async fn lock_session(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(hash.lock_key())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn unlock_session(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(hash.lock_key())
        .fetch_one(conn)
        .await?;
    if !released {
        tracing::warn!(%hash, "blob advisory lock was not held");
    }
    Ok(())
}

pub async fn find(pool: &PgPool, hash: &ContentHash) -> Result<Option<BlobRecord>, DbError> {
    let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE sha256 = $1");
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(hash.as_str())
        .fetch_optional(pool)
        .await?)
}

/// The blob if a node of `owner_id` (live or in their trash) references it.
/// This is the only dedup lookup: other users' content is never revealed.
pub async fn find_owned(
    conn: &mut PgConnection,
    owner_id: Uuid,
    hash: &ContentHash,
) -> Result<Option<BlobRecord>, DbError> {
    let sql = format!(
        "SELECT {BLOB_COLUMNS} FROM blobs b WHERE b.sha256 = $2 \
         AND EXISTS (SELECT 1 FROM nodes n WHERE n.owner_id = $1 AND n.blob_sha256 = b.sha256)"
    );
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(owner_id)
        .bind(hash.as_str())
        .fetch_optional(conn)
        .await?)
}

/// Records a stored blob; an existing row is returned unchanged.
pub async fn insert_if_absent(
    conn: &mut PgConnection,
    hash: &ContentHash,
    size: i64,
    mime: &str,
) -> Result<BlobRecord, DbError> {
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, $3) ON CONFLICT (sha256) DO NOTHING")
        .bind(hash.as_str())
        .bind(size)
        .bind(mime)
        .execute(&mut *conn)
        .await?;
    let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE sha256 = $1");
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(hash.as_str())
        .fetch_one(conn)
        .await?)
}

/// Blobs with no reference since before `cutoff`, oldest first.
pub async fn unreferenced_since(
    pool: &PgPool,
    cutoff: OffsetDateTime,
    limit: i64,
) -> Result<Vec<ContentHash>, DbError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT sha256 FROM blobs WHERE ref_count = 0 AND zero_since < $1 ORDER BY zero_since LIMIT $2",
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .filter_map(|sha| ContentHash::parse(sha).ok())
        .collect())
}

/// Deletes the row if it is still unreferenced since before `cutoff`. Run in
/// autocommit while holding [`lock_session`].
pub async fn delete_if_unreferenced(
    conn: &mut PgConnection,
    hash: &ContentHash,
    cutoff: OffsetDateTime,
) -> Result<bool, DbError> {
    let result =
        sqlx::query("DELETE FROM blobs WHERE sha256 = $1 AND ref_count = 0 AND zero_since < $2")
            .bind(hash.as_str())
            .bind(cutoff)
            .execute(conn)
            .await?;
    Ok(result.rows_affected() == 1)
}
