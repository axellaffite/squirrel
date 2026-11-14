//! Periodic housekeeping, run by the `squirrel` binary (see its `tasks.rs`):
//! expired uploads, old trash, unreferenced blobs and blob files without a row.

use std::collections::HashSet;
use std::time::SystemTime;

use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, Postgres};
use squirrel_core::ContentHash;
use squirrel_db::{DbError, blobs, trash, uploads};
use squirrel_storage::BlobStore;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

/// Trash entries removed per statement.
const TRASH_BATCH: i64 = 500;
/// Blobs examined per GC run; the rest wait for the next run.
const GC_BATCH: i64 = 1000;
/// Blob files checked against the catalogue per query.
const SWEEP_BATCH: usize = 1000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UploadCleanup {
    pub expired_sessions: usize,
    /// Upload files without a session (expired, aborted mid-write, crashed).
    pub files_removed: usize,
    /// Orphan files that could not be removed (logged; retried next run).
    pub files_failed: usize,
}

impl UploadCleanup {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Deletes expired upload sessions, then every upload file that no session owns.
/// Sessions are created before their file, so a file listed here whose id has
/// no row really is an orphan.
pub async fn purge_expired_uploads(state: &AppState) -> Result<UploadCleanup, ApiError> {
    let expired = uploads::delete_expired(&state.pool).await?;
    let store = state.storage.clone();
    let on_disk = run_storage(move || store.list_uploads()).await?;
    let live: HashSet<Uuid> = uploads::existing(&state.pool, &on_disk)
        .await?
        .into_iter()
        .collect();
    let orphans: Vec<Uuid> = on_disk
        .into_iter()
        .filter(|id| !live.contains(id))
        .collect();
    let store = state.storage.clone();
    let discarded = run_storage(move || Ok(discard_each(&store, &orphans))).await?;
    Ok(UploadCleanup {
        expired_sessions: expired.len(),
        files_removed: discarded.removed,
        files_failed: discarded.failed,
    })
}

struct Discarded {
    removed: usize,
    failed: usize,
}

/// Removes every orphan, logging (not stopping at) the ones that fail.
fn discard_each(store: &BlobStore, orphans: &[Uuid]) -> Discarded {
    let failed = orphans
        .iter()
        .filter(|id| match store.discard_upload(**id) {
            Ok(()) => false,
            Err(err) => {
                tracing::warn!(upload_id = %id, error = %err, "could not remove an orphan upload file");
                true
            }
        })
        .count();
    Discarded {
        removed: orphans.len() - failed,
        failed,
    }
}

/// Deletes trash entries older than the retention period (30 days).
pub async fn purge_old_trash(state: &AppState) -> Result<u64, ApiError> {
    let cutoff = OffsetDateTime::now_utc() - state.config.trash_retention;
    Ok(trash::purge_older_than(&state.pool, cutoff, TRASH_BATCH).await?)
}

/// Deletes blobs unreferenced for longer than the grace period: the row first,
/// then the file, under the blob's session-level advisory lock so a concurrent
/// upload of the same content cannot link to a file being removed.
pub async fn collect_garbage(state: &AppState) -> Result<usize, ApiError> {
    let cutoff = OffsetDateTime::now_utc() - state.config.blob_gc_grace;
    let candidates = blobs::unreferenced_since(&state.pool, cutoff, GC_BATCH).await?;
    let mut removed = 0;
    for hash in &candidates {
        if remove_if_unreferenced(state, hash, cutoff).await? {
            removed += 1;
        }
    }
    Ok(removed)
}

async fn remove_if_unreferenced(
    state: &AppState,
    hash: &ContentHash,
    cutoff: OffsetDateTime,
) -> Result<bool, ApiError> {
    let mut conn = state.pool.acquire().await.map_err(DbError::from)?;
    blobs::lock_session(&mut conn, hash).await?;
    let result = delete_row_then_file(state, &mut conn, hash, cutoff).await;
    unlock(conn, hash).await;
    result
}

async fn unlock(mut conn: PoolConnection<Postgres>, hash: &ContentHash) {
    if let Err(err) = blobs::unlock_session(&mut conn, hash).await {
        // Never hand a connection that still holds the lock back to the pool.
        tracing::error!(error = %err, %hash, "could not release a blob lock; closing the connection");
        conn.close_on_drop();
    }
}

async fn delete_row_then_file(
    state: &AppState,
    conn: &mut PgConnection,
    hash: &ContentHash,
    cutoff: OffsetDateTime,
) -> Result<bool, ApiError> {
    if !blobs::delete_if_unreferenced(conn, hash, cutoff).await? {
        return Ok(false);
    }
    // Row first: a crash between the two leaves an orphan file, never a row without its file.
    let (store, blob) = (state.storage.clone(), hash.clone());
    run_storage(move || store.remove_blob(&blob)).await?;
    tracing::debug!(%hash, "blob deleted");
    Ok(true)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrphanSweep {
    /// Blob files removed because no catalogue row knows them.
    pub removed: usize,
    /// Orphans that could not be removed (logged; retried next run).
    pub failed: usize,
}

impl OrphanSweep {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Removes blob files that have no catalogue row (a crash between storing a
/// file and committing its row, or between deleting a row and its file) and
/// were last modified before the grace period. Each removal re-checks the row
/// under the blob's advisory lock, which completion holds from before the
/// rename until its row commits.
pub async fn sweep_orphan_blobs(state: &AppState) -> Result<OrphanSweep, ApiError> {
    let cutoff = SystemTime::now()
        .checked_sub(state.config.blob_gc_grace.unsigned_abs())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let store = state.storage.clone();
    let on_disk = run_storage(move || store.list_blobs_older_than(cutoff)).await?;
    let mut sweep = OrphanSweep::default();
    for batch in on_disk.chunks(SWEEP_BATCH) {
        let known: HashSet<String> = blobs::existing(&state.pool, batch)
            .await?
            .into_iter()
            .collect();
        for hash in batch.iter().filter(|hash| !known.contains(hash.as_str())) {
            match remove_if_orphan(state, hash).await {
                Ok(true) => sweep.removed += 1,
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(%hash, error = %err, "could not remove an orphan blob file");
                    sweep.failed += 1;
                }
            }
        }
    }
    Ok(sweep)
}

async fn remove_if_orphan(state: &AppState, hash: &ContentHash) -> Result<bool, ApiError> {
    let mut conn = state.pool.acquire().await.map_err(DbError::from)?;
    blobs::lock_session(&mut conn, hash).await?;
    let result = remove_unless_catalogued(state, &mut conn, hash).await;
    unlock(conn, hash).await;
    result
}

async fn remove_unless_catalogued(
    state: &AppState,
    conn: &mut PgConnection,
    hash: &ContentHash,
) -> Result<bool, ApiError> {
    if blobs::exists(conn, hash).await? {
        return Ok(false);
    }
    let (store, blob) = (state.storage.clone(), hash.clone());
    run_storage(move || store.remove_blob(&blob)).await?;
    tracing::debug!(%hash, "orphan blob file deleted");
    Ok(true)
}
