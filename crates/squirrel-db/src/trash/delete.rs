//! Permanent deletion. Content a phone backed up is tombstoned first
//! (`deleted_hashes`). Every path deletes whole batches in one statement (so
//! the blob reference triggers see all rows at once and lock blobs in sha256
//! order), after moving nested entries out of the doomed batches.
//!
//! Locking: these functions take the owner lock (`quota::lock_owner`, purge
//! takes the lock of every owner in its batch, in id order). Re-checking
//! `trash_root = id` under READ COMMITTED would be enough to keep a restored
//! entry alive, but not to keep the nested-entry re-parenting consistent with a
//! concurrent `trash` or `restore` of the same owner; the lock serializes them.

use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::nodes::RESTORE_FALLBACK_ROOT;
use crate::quota;
use crate::retry::retry_transient;

/// Deletes an entry and everything below it for good. Entries trashed earlier
/// inside it survive and move under `Files/`. Blob references drop through the
/// trigger; blob GC removes the files later.
pub async fn delete_forever(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    retry_transient(|| async move {
        let mut tx = pool.begin().await?;
        quota::lock_owner(&mut tx, owner_id).await?;
        let deleted = delete_entries(&mut tx, &[id], Some(owner_id)).await?;
        if deleted == 0 {
            return Err(DbError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    })
    .await
}

/// Deletes every trash entry of the owner; returns how many entries went.
pub async fn empty(pool: &PgPool, owner_id: Uuid) -> Result<u64, DbError> {
    retry_transient(|| async move {
        let mut tx = pool.begin().await?;
        quota::lock_owner(&mut tx, owner_id).await?;
        let ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM nodes WHERE owner_id = $1 AND trash_root = id")
                .bind(owner_id)
                .fetch_all(&mut *tx)
                .await?;
        let deleted = delete_entries(&mut tx, &ids, Some(owner_id)).await?;
        tx.commit().await?;
        Ok(deleted)
    })
    .await
}

/// Deletes entries of every user trashed before `cutoff`, `batch` entries per
/// transaction, and returns how many went. An entry nested in an older one
/// survives if it is itself recent enough.
pub async fn purge_older_than(
    pool: &PgPool,
    cutoff: OffsetDateTime,
    batch: i64,
) -> Result<u64, DbError> {
    if batch <= 0 {
        return Err(DbError::InvalidInput(
            "the purge batch size must be positive",
        ));
    }
    let mut total = 0;
    loop {
        let (selected, deleted) =
            retry_transient(|| async move { purge_batch(pool, cutoff, batch).await }).await?;
        total += deleted;
        if selected < batch {
            return Ok(total);
        }
    }
}

/// One purge transaction: returns (candidates selected, entries deleted).
async fn purge_batch(
    pool: &PgPool,
    cutoff: OffsetDateTime,
    batch: i64,
) -> Result<(i64, u64), DbError> {
    let mut tx = pool.begin().await?;
    let candidates: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, owner_id FROM nodes WHERE trash_root = id AND deleted_at < $1 \
         ORDER BY deleted_at, id LIMIT $2",
    )
    .bind(cutoff)
    .bind(batch)
    .fetch_all(&mut *tx)
    .await?;
    let selected = i64::try_from(candidates.len()).unwrap_or(i64::MAX);
    if candidates.is_empty() {
        return Ok((0, 0));
    }
    let (ids, owners): (Vec<Uuid>, Vec<Uuid>) = candidates.into_iter().unzip();
    sqlx::query("SELECT 1 FROM users WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE")
        .bind(&owners)
        .execute(&mut *tx)
        .await?;
    // Re-check under the locks: an entry restored meanwhile no longer qualifies.
    let still_old: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM nodes WHERE id = ANY($1) AND trash_root = id AND deleted_at < $2",
    )
    .bind(&ids)
    .bind(cutoff)
    .fetch_all(&mut *tx)
    .await?;
    let deleted = delete_entries(&mut tx, &still_old, None).await?;
    tx.commit().await?;
    Ok((selected, deleted))
}

/// Deletes the batches of the trash entries `ids` (optionally restricted to
/// one owner) and returns how many entries went. Nested entries of other
/// batches first move under their owner's `Files/` root, keeping their own
/// `deleted_at` and `trash_root`. The caller holds the owners' locks.
async fn delete_entries(
    conn: &mut PgConnection,
    ids: &[Uuid],
    owner_id: Option<Uuid>,
) -> Result<u64, DbError> {
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query(
        "WITH doomed AS ( \
             SELECT id, owner_id FROM nodes \
             WHERE id = ANY($1) AND trash_root = id AND ($2::uuid IS NULL OR owner_id = $2) \
         ), batch AS ( \
             SELECT n.id, n.owner_id FROM nodes n \
             JOIN doomed d ON n.owner_id = d.owner_id AND n.trash_root = d.id \
         ) \
         UPDATE nodes e SET parent_id = ( \
             SELECT r.id FROM nodes r WHERE r.owner_id = e.owner_id AND r.parent_id IS NULL \
               AND r.name = $3 AND r.deleted_at IS NULL) \
         FROM batch b \
         WHERE e.parent_id = b.id AND e.owner_id = b.owner_id \
           AND e.trash_root = e.id AND e.id <> ALL($1)",
    )
    .bind(ids)
    .bind(owner_id)
    .bind(RESTORE_FALLBACK_ROOT)
    .execute(&mut *conn)
    .await?;
    record_deleted_hashes(conn, ids, owner_id).await?;
    let deleted: i64 = sqlx::query_scalar(
        "WITH doomed AS ( \
             SELECT id, owner_id FROM nodes \
             WHERE id = ANY($1) AND trash_root = id AND ($2::uuid IS NULL OR owner_id = $2) \
         ), gone AS ( \
             DELETE FROM nodes n USING doomed d \
             WHERE n.owner_id = d.owner_id AND n.trash_root = d.id \
             RETURNING n.id, n.trash_root \
         ) \
         SELECT count(*) FILTER (WHERE id = trash_root) FROM gone",
    )
    .bind(ids)
    .bind(owner_id)
    .fetch_one(conn)
    .await?;
    Ok(u64::try_from(deleted).unwrap_or(0))
}

/// Tombstones the content of the doomed batches that a device of their owner
/// had backed up (`device_assets`), so no phone of that owner uploads it again
/// (see `device_assets::missing`). Runs in the deleting transaction.
async fn record_deleted_hashes(
    conn: &mut PgConnection,
    ids: &[Uuid],
    owner_id: Option<Uuid>,
) -> Result<(), DbError> {
    sqlx::query(
        "WITH doomed AS ( \
             SELECT id, owner_id FROM nodes \
             WHERE id = ANY($1) AND trash_root = id AND ($2::uuid IS NULL OR owner_id = $2) \
         ), content AS ( \
             SELECT DISTINCT n.owner_id, n.blob_sha256 AS sha256 FROM nodes n \
             JOIN doomed d ON n.owner_id = d.owner_id AND n.trash_root = d.id \
             WHERE n.blob_sha256 IS NOT NULL \
         ) \
         INSERT INTO deleted_hashes (owner_id, sha256) \
         SELECT c.owner_id, c.sha256 FROM content c \
          WHERE EXISTS (SELECT 1 FROM device_assets a JOIN devices v ON v.id = a.device_id \
                         WHERE a.sha256 = c.sha256 AND v.owner_id = c.owner_id) \
         ON CONFLICT (owner_id, sha256) DO NOTHING",
    )
    .bind(ids)
    .bind(owner_id)
    .execute(conn)
    .await?;
    Ok(())
}
