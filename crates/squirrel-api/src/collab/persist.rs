//! Loading a co-edited file's document and saving it back into the file.

use std::io::Read;
use std::sync::Arc;
use std::time::Instant;

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;
use squirrel_db::collab::{self, CollabDoc};
use squirrel_db::nodes::{self, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::{DbError, blobs};

use super::room::{self, CloseReason, Room, Snapshot};
use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

/// MIME essences edited as text besides `text/*`.
const TEXT_APPLICATIONS: &[&str] = &[
    "application/json",
    "application/xml",
    "application/yaml",
    "application/x-yaml",
    "application/toml",
    "application/javascript",
    "application/x-sh",
    "application/sql",
];
const FALLBACK_MIME: &str = "text/plain";

#[derive(Debug, thiserror::Error)]
pub enum EditableError {
    #[error("{0}")]
    NotEditable(&'static str),
    #[error(transparent)]
    Api(#[from] ApiError),
}

impl From<DbError> for EditableError {
    fn from(err: DbError) -> Self {
        Self::Api(err.into())
    }
}

/// Whether a file of this MIME type is text the editor can open.
pub fn is_text_mime(mime: &str) -> bool {
    let essence = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence.starts_with("text/") || TEXT_APPLICATIONS.contains(&essence.as_str())
}

/// The stored document of file `node`, rebuilt from the file when there is
/// none or the file changed since (new epoch). The node is read again here:
/// a save may have moved it to a new blob since the caller fetched it.
pub async fn open_doc(state: &AppState, node: &NodeRecord) -> Result<CollabDoc, EditableError> {
    for _ in 0..3 {
        let current = nodes::find_live(&state.pool, node.owner_id, node.id)
            .await?
            .ok_or(EditableError::Api(ApiError::NotFound))?;
        if let Some(doc) = open_current(state, &current).await? {
            return Ok(doc);
        }
    }
    Err(EditableError::Api(ApiError::Busy))
}

/// `None`: the file changed while its document was being built; try again.
async fn open_current(
    state: &AppState,
    node: &NodeRecord,
) -> Result<Option<CollabDoc>, EditableError> {
    let max_bytes = state.config.collab.max_bytes;
    if !node.mime.as_deref().is_some_and(is_text_mime) {
        return Err(EditableError::NotEditable("only text files can be edited"));
    }
    if usize::try_from(node.size).map_or(true, |size| size > max_bytes) {
        return Err(EditableError::NotEditable("this file is too large to edit"));
    }
    let blob = node
        .blob_sha256
        .clone()
        .ok_or(EditableError::NotEditable("only files can be edited"))?;
    if let Some(doc) = collab::find(&state.pool, node.id).await?
        && doc.blob_sha256 == blob
    {
        return Ok(Some(doc));
    }
    let hash = ContentHash::parse(&blob).map_err(|err| ApiError::Internal(err.into()))?;
    let store = state.storage.clone();
    let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let bytes = run_storage(move || {
        let (file, _) = store.open_blob(&hash)?;
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    })
    .await;
    let bytes = match bytes {
        Ok(bytes) => bytes,
        // Collected after a save moved the file to a new blob: read again.
        Err(ApiError::NotFound) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if bytes.len() > max_bytes {
        return Err(EditableError::NotEditable("this file is too large to edit"));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| EditableError::NotEditable("this file is not UTF-8 text"))?;
    Ok(collab::start(&state.pool, node.id, &blob, &room::initial_state(&text)).await?)
}

/// Saves `room` after each burst of changes, and again after a failed save,
/// until the room is retired.
pub async fn saver(state: AppState, room: Arc<Room>) {
    let config = state.config.collab.clone();
    loop {
        room.changed.notified().await;
        if room.is_retired() {
            return;
        }
        let first = Instant::now();
        loop {
            let quiet = tokio::time::sleep(config.save_quiet);
            tokio::select! {
                () = quiet => break,
                () = room.changed.notified() => {
                    if first.elapsed() >= config.save_max_delay || room.is_retired() {
                        break;
                    }
                }
            }
        }
        save(&state, &room).await;
        if room.is_retired() {
            return;
        }
        if room.is_dirty() {
            tokio::time::sleep(config.save_retry).await;
            room.changed.notify_one();
        } else if room.peers() == 0 && super::retire_idle(&state, &room) {
            return;
        }
    }
}

/// Saves unsaved changes of `room`, if any, one save at a time. A file gone
/// or changed outside the editor, or a full quota, closes the room.
pub async fn save(state: &AppState, room: &Arc<Room>) {
    let _one_at_a_time = room.saving.lock().await;
    let Some(snapshot) = room.take_dirty() else {
        return;
    };
    match write(state, room, &snapshot).await {
        Ok(Saved::Content(sha)) => {
            tracing::info!(node_id = %room.node_id, bytes = snapshot.text.len(), "co-edited file saved");
            room.saved(sha);
        }
        Ok(Saved::StateOnly) => {}
        Err(SaveError::Close(reason)) => {
            tracing::info!(node_id = %room.node_id, ?reason, "co-editing room closed while saving");
            room.close(reason);
            super::retire_idle(state, room);
        }
        Err(SaveError::Retry(err)) => {
            tracing::warn!(node_id = %room.node_id, error = %err, "could not save a co-edited file");
            room.save_failed();
        }
    }
}

enum Saved {
    Content(String),
    StateOnly,
}

enum SaveError {
    Close(CloseReason),
    Retry(ApiError),
}

impl<E: Into<ApiError>> From<E> for SaveError {
    fn from(err: E) -> Self {
        Self::Retry(err.into())
    }
}

/// One transaction: owner lock and quota, blob lock, blob file and row, the
/// node pointed at the new blob, the document state, media processing queued.
async fn write(state: &AppState, room: &Room, snapshot: &Snapshot) -> Result<Saved, SaveError> {
    let bytes = snapshot.text.as_bytes();
    if bytes.len() > state.config.collab.max_bytes {
        return Err(SaveError::Close(CloseReason::TooLarge));
    }
    let hash = ContentHash::from_digest(Sha256::digest(bytes).into());
    let mut tx = state.pool.begin().await.map_err(DbError::from)?;
    if hash.as_str() == snapshot.base_sha {
        let kept = collab::save_state(
            &mut tx,
            room.node_id,
            snapshot.epoch,
            hash.as_str(),
            &snapshot.state,
        )
        .await?;
        if !kept {
            return Err(SaveError::Close(CloseReason::Stale));
        }
        tx.commit().await.map_err(DbError::from)?;
        return Ok(Saved::StateOnly);
    }
    let owner_id = room.owner_id;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await?;
    let node = nodes::find_live(&mut *tx, owner_id, room.node_id)
        .await?
        .ok_or(SaveError::Close(CloseReason::Gone))?;
    let size = i64::try_from(bytes.len()).map_err(|err| ApiError::Internal(err.into()))?;
    match quota::ensure_room(
        &mut tx,
        owner_id,
        quota_bytes,
        (size - node.size).max(0),
        Pending::Ignore,
    )
    .await
    {
        Err(DbError::QuotaExceeded) => return Err(SaveError::Close(CloseReason::StorageFull)),
        other => other?,
    }
    blobs::lock(&mut tx, &hash).await?;
    let (store, content, blob) = (state.storage.clone(), bytes.to_vec(), hash.clone());
    run_storage(move || store.put_bytes(&content, &blob)).await?;
    let mime = node.mime.as_deref().unwrap_or(FALLBACK_MIME);
    blobs::insert_if_absent(&mut tx, &hash, size, mime).await?;
    let node = collab::replace_blob(
        &mut tx,
        owner_id,
        room.node_id,
        &snapshot.base_sha,
        &hash,
        size,
    )
    .await?
    .ok_or(SaveError::Close(CloseReason::Stale))?;
    let kept = collab::save_state(
        &mut tx,
        room.node_id,
        snapshot.epoch,
        hash.as_str(),
        &snapshot.state,
    )
    .await?;
    if !kept {
        return Err(SaveError::Close(CloseReason::Stale));
    }
    crate::routes::uploads::enqueue_media(&mut tx, &node).await?;
    tx.commit().await.map_err(DbError::from)?;
    Ok(Saved::Content(hash.as_str().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::is_text_mime;

    #[test]
    fn text_mimes_are_editable() {
        for mime in [
            "text/plain",
            "text/markdown; charset=utf-8",
            "application/json",
            "TEXT/CSV",
        ] {
            assert!(is_text_mime(mime), "{mime}");
        }
        for mime in [
            "image/png",
            "application/pdf",
            "application/octet-stream",
            "",
        ] {
            assert!(!is_text_mime(mime), "{mime}");
        }
    }
}
