//! Periodic housekeeping, run by the `squirrel` binary (see its `tasks.rs`):
//! expired uploads, device tokens and old devices, old trash, unreferenced blobs (with their derived files),
//! and blob files or derived directories without a row.

use std::collections::HashSet;
use std::time::SystemTime;

use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, Postgres};
use squirrel_core::ContentHash;
use squirrel_db::{DbError, blobs, devices, trash, uploads};
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
/// Crashes leave a handful of orphan files at most. More orphans than this,
/// making up more than [`SWEEP_BRAKE_MAX_PERCENT`] of the files examined, means
/// the catalogue does not describe this data directory (wrong or restored
/// database): the sweep then deletes nothing.
pub const SWEEP_BRAKE_MIN_ORPHANS: usize = 20;
const SWEEP_BRAKE_MAX_PERCENT: usize = 10;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceCleanup {
    pub expired_access_tokens: u64,
    /// Devices revoked, or idle past their refresh lifetime, for longer than the retention.
    pub stale_devices: u64,
    pub retired_refresh_tokens: u64,
}

impl DeviceCleanup {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Deletes expired access tokens, devices revoked or expired for longer than
/// [`crate::ApiConfig::device_retention`] (with their sync records), and
/// retired refresh tokens older than the refresh lifetime.
pub async fn purge_devices(state: &AppState) -> Result<DeviceCleanup, ApiError> {
    let now = OffsetDateTime::now_utc();
    Ok(DeviceCleanup {
        expired_access_tokens: devices::delete_expired_tokens(&state.pool).await?,
        stale_devices: devices::delete_stale(&state.pool, now - state.config.device_retention)
            .await?,
        retired_refresh_tokens: devices::prune_retired(
            &state.pool,
            now - state.config.device_refresh_ttl,
        )
        .await?,
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
    let previews = state.preview_cache.clone();
    run_storage(move || {
        store.remove_blob(&blob)?;
        store.derived().remove_all(&blob)?;
        // A cached preview of content nobody owns any more must not outlive it.
        previews.map_or(Ok(0), |cache| cache.remove_hash(&blob))?;
        Ok(())
    })
    .await?;
    tracing::debug!(%hash, "blob deleted");
    Ok(true)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrphanSweep {
    /// Blob files removed because no catalogue row knows them.
    pub removed: usize,
    /// Orphans that could not be removed (logged; retried next run).
    pub failed: usize,
    /// Orphans left alone because there were implausibly many (logged).
    pub held_back: usize,
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
/// rename until its row commits. Deletes nothing when the orphans look like a
/// catalogue/data-directory mismatch rather than crash leftovers.
pub async fn sweep_orphan_blobs(state: &AppState) -> Result<OrphanSweep, ApiError> {
    let cutoff = sweep_cutoff(state);
    let store = state.storage.clone();
    let on_disk = run_storage(move || store.list_blobs_older_than(cutoff)).await?;
    sweep_orphans(state, &on_disk, Leftover::BlobFile).await
}

/// Removes derived directories (thumbnails) whose blob has no catalogue row
/// (a crash between deleting the blob and its derived files, or a worker
/// finishing after the blob was collected), last modified before the grace
/// period. Same lock, re-check and brake as [`sweep_orphan_blobs`].
pub async fn sweep_orphan_derived(state: &AppState) -> Result<OrphanSweep, ApiError> {
    let cutoff = sweep_cutoff(state);
    let store = state.storage.clone();
    let on_disk = run_storage(move || store.derived().list_older_than(cutoff)).await?;
    sweep_orphans(state, &on_disk, Leftover::DerivedFiles).await
}

fn sweep_cutoff(state: &AppState) -> SystemTime {
    SystemTime::now()
        .checked_sub(state.config.blob_gc_grace.unsigned_abs())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

async fn sweep_orphans(
    state: &AppState,
    on_disk: &[ContentHash],
    leftover: Leftover,
) -> Result<OrphanSweep, ApiError> {
    let orphans = uncatalogued(state, on_disk).await?;
    if orphans.is_empty() {
        return Ok(OrphanSweep::default());
    }
    let catalogue_empty = !blobs::any_exist(&state.pool).await?;
    if sweep_looks_wrong(catalogue_empty, on_disk.len(), orphans.len()) {
        tracing::error!(
            listed = on_disk.len(),
            orphans = orphans.len(),
            catalogue_empty,
            ?leftover,
            "not sweeping orphan files: far too many have no catalogue row; \
             is the server using the right database for this data directory?"
        );
        return Ok(OrphanSweep {
            held_back: orphans.len(),
            ..OrphanSweep::default()
        });
    }
    let mut sweep = OrphanSweep::default();
    for hash in &orphans {
        match remove_if_orphan(state, hash, leftover).await {
            Ok(true) => sweep.removed += 1,
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(%hash, ?leftover, error = %err, "could not remove orphan files");
                sweep.failed += 1;
            }
        }
    }
    Ok(sweep)
}

/// The files of `on_disk` without a catalogue row.
async fn uncatalogued(
    state: &AppState,
    on_disk: &[ContentHash],
) -> Result<Vec<ContentHash>, ApiError> {
    let mut orphans = Vec::new();
    for batch in on_disk.chunks(SWEEP_BATCH) {
        let known: HashSet<String> = blobs::existing(&state.pool, batch)
            .await?
            .into_iter()
            .collect();
        orphans.extend(
            batch
                .iter()
                .filter(|hash| !known.contains(hash.as_str()))
                .cloned(),
        );
    }
    Ok(orphans)
}

/// Whether `orphans` uncatalogued files among `listed` look like a mismatch
/// between the catalogue and the data directory rather than crash leftovers.
fn sweep_looks_wrong(catalogue_empty: bool, listed: usize, orphans: usize) -> bool {
    if orphans == 0 {
        return false;
    }
    catalogue_empty
        || (orphans > SWEEP_BRAKE_MIN_ORPHANS
            && orphans.saturating_mul(100) > listed.saturating_mul(SWEEP_BRAKE_MAX_PERCENT))
}

/// What an orphan sweep removes for a hash without a catalogue row.
#[derive(Debug, Clone, Copy)]
enum Leftover {
    BlobFile,
    DerivedFiles,
}

async fn remove_if_orphan(
    state: &AppState,
    hash: &ContentHash,
    leftover: Leftover,
) -> Result<bool, ApiError> {
    let mut conn = state.pool.acquire().await.map_err(DbError::from)?;
    blobs::lock_session(&mut conn, hash).await?;
    let result = remove_unless_catalogued(state, &mut conn, hash, leftover).await;
    unlock(conn, hash).await;
    result
}

async fn remove_unless_catalogued(
    state: &AppState,
    conn: &mut PgConnection,
    hash: &ContentHash,
    leftover: Leftover,
) -> Result<bool, ApiError> {
    if blobs::exists(conn, hash).await? {
        return Ok(false);
    }
    let (store, blob) = (state.storage.clone(), hash.clone());
    run_storage(move || match leftover {
        Leftover::BlobFile => store.remove_blob(&blob),
        Leftover::DerivedFiles => store.derived().remove_all(&blob),
    })
    .await?;
    tracing::debug!(%hash, ?leftover, "orphan files deleted");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_crash_leftovers_are_swept() {
        assert!(!sweep_looks_wrong(false, 10, 0));
        assert!(!sweep_looks_wrong(false, 2, 1));
        assert!(!sweep_looks_wrong(false, 10, SWEEP_BRAKE_MIN_ORPHANS));
        assert!(!sweep_looks_wrong(false, 10_000, 1000));
    }

    #[test]
    fn an_empty_catalogue_never_sweeps() {
        assert!(sweep_looks_wrong(true, 1, 1));
        assert!(!sweep_looks_wrong(true, 0, 0));
    }

    #[test]
    fn many_orphans_making_up_a_large_share_are_held_back() {
        let many = SWEEP_BRAKE_MIN_ORPHANS + 1;
        assert!(sweep_looks_wrong(false, many, many));
        assert!(sweep_looks_wrong(false, many * 9, many));
        assert!(!sweep_looks_wrong(false, many * 10, many));
    }
}
