//! Periodic housekeeping, run by the `squirrel` binary (see its `tasks.rs`):
//! expired uploads, old trash and unreferenced blobs.

use std::collections::HashSet;

use sqlx::PgConnection;
use squirrel_core::ContentHash;
use squirrel_db::{DbError, blobs, trash, uploads};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

/// Trash entries removed per statement.
const TRASH_BATCH: i64 = 500;
/// Blobs examined per GC run; the rest wait for the next run.
const GC_BATCH: i64 = 1000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UploadCleanup {
    pub expired_sessions: usize,
    /// Upload files without a session (expired, aborted mid-write, crashed).
    pub files_removed: usize,
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
    let files_removed = orphans.len();
    let store = state.storage.clone();
    run_storage(move || orphans.iter().try_for_each(|id| store.discard_upload(*id))).await?;
    Ok(UploadCleanup {
        expired_sessions: expired.len(),
        files_removed,
    })
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
    if let Err(err) = blobs::unlock_session(&mut conn, hash).await {
        // Never hand a connection that still holds the lock back to the pool.
        tracing::error!(error = %err, %hash, "could not release a blob lock; closing the connection");
        conn.close_on_drop();
    }
    result
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
