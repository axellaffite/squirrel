//! Periodic housekeeping, run by the `squirrel` binary (see its `tasks.rs`).

use std::collections::HashSet;

use squirrel_db::uploads;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

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
