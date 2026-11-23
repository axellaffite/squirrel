//! Live co-editing of text files (plan: docs/plans/2026-11-23-text-editor.md).
//!
//! One [`Room`] per open file lives in the [`Registry`] while anyone is
//! connected. Edits are saved into the file itself (a new blob, same node) by a
//! per-room saver, and the Yjs state is kept in `collab_docs` so a reconnecting
//! browser merges into the same document.

mod persist;
pub mod room;
pub mod socket;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use squirrel_db::collab::CollabDoc;
use squirrel_db::nodes::NodeRecord;
use uuid::Uuid;

pub use persist::{EditableError, open_doc};
pub use room::{CloseReason, JoinError, Room};

use crate::error::ApiError;
use crate::state::AppState;

/// Limits and timings of co-editing.
#[derive(Debug, Clone)]
pub struct CollabConfig {
    /// Largest text that can be edited, in bytes of UTF-8.
    pub max_bytes: usize,
    pub max_rooms: usize,
    pub max_peers: usize,
    /// Save this long after the last change...
    pub save_quiet: Duration,
    /// ...or at the latest this long after the first unsaved one.
    pub save_max_delay: Duration,
    /// How often a connection re-checks its session and access.
    pub recheck_every: Duration,
    /// Wait before trying a failed save again.
    pub save_retry: Duration,
}

impl Default for CollabConfig {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024,
            max_rooms: 256,
            max_peers: 32,
            save_quiet: Duration::from_secs(2),
            save_max_delay: Duration::from_secs(10),
            recheck_every: Duration::from_secs(10),
            save_retry: Duration::from_secs(5),
        }
    }
}

/// The open rooms, by node id.
#[derive(Default)]
pub struct Registry {
    rooms: Mutex<HashMap<Uuid, Arc<Room>>>,
}

impl Registry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, Arc<Room>>> {
        self.rooms
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Number of open rooms (tests and logs).
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn get(&self, node_id: Uuid) -> Option<Arc<Room>> {
        self.lock().get(&node_id).cloned()
    }

    /// Unregisters `room` if it is idle (nobody in it, nothing unsaved) or
    /// closed. Marking it retired under the registry lock makes a concurrent
    /// join look it up again instead of joining a room nobody saves.
    fn retire(&self, room: &Arc<Room>) -> bool {
        let mut rooms = self.lock();
        if !room.retire_if_idle() {
            return false;
        }
        if rooms
            .get(&room.node_id)
            .is_some_and(|current| Arc::ptr_eq(current, room))
        {
            rooms.remove(&room.node_id);
        }
        true
    }
}

/// The room for `node`, loaded if needed, and a greeting for one more peer.
/// `epoch` must match the stored document's: a client holding an older one
/// reopens, and a room left on an older epoch (the file changed outside the
/// editor) is closed for everyone.
pub async fn join(
    state: &AppState,
    node: &NodeRecord,
    epoch: Uuid,
) -> Result<(Arc<Room>, Vec<Vec<u8>>), CloseReason> {
    let config = &state.config.collab;
    let doc = open_doc(state, node).await.map_err(|err| {
        tracing::info!(node_id = %node.id, error = %err, "co-edited file cannot be opened");
        CloseReason::Gone
    })?;
    if epoch != doc.epoch {
        // Checked before loading anything: a wrong epoch leaves no room behind.
        close_outdated(state, node.id, doc.epoch);
        return Err(CloseReason::Stale);
    }
    for _ in 0..3 {
        let room = match state.collab.get(node.id) {
            Some(room) => room,
            None => load(state, node, &doc)?,
        };
        if room.epoch() != doc.epoch {
            close_outdated(state, node.id, doc.epoch);
            continue;
        }
        match room.join(config.max_peers) {
            Ok(greeting) => return Ok((room, greeting)),
            Err(JoinError::Retired) => {}
            Err(JoinError::Refused(reason)) if room.closed().is_some() => {
                state.collab.retire(&room);
                if reason == CloseReason::Gone {
                    return Err(reason);
                }
            }
            Err(JoinError::Refused(reason)) => return Err(reason),
        }
    }
    Err(CloseReason::Busy)
}

/// Closes the open room of `node_id` if it holds another epoch than the
/// stored document's (the file changed outside the editor).
fn close_outdated(state: &AppState, node_id: Uuid, epoch: Uuid) {
    if let Some(room) = state.collab.get(node_id)
        && room.epoch() != epoch
    {
        tracing::info!(%node_id, "co-edited file changed outside the editor");
        room.close(CloseReason::Stale);
        retire_idle(state, &room);
    }
}

/// Registers a room for the stored document, unless another connection
/// registered one meanwhile (that one is returned).
fn load(state: &AppState, node: &NodeRecord, doc: &CollabDoc) -> Result<Arc<Room>, CloseReason> {
    let config = &state.config.collab;
    let room = Room::load(
        node.id,
        node.owner_id,
        doc.epoch,
        doc.blob_sha256.clone(),
        &doc.state,
        config.max_bytes,
    )
    .map_err(|err| {
        tracing::error!(node_id = %node.id, error = %err, "stored co-editing state is unreadable");
        CloseReason::Gone
    })?;
    let room = {
        let mut rooms = state.collab.lock();
        if let Some(existing) = rooms.get(&node.id) {
            return Ok(existing.clone());
        }
        if rooms.len() >= config.max_rooms {
            return Err(CloseReason::Busy);
        }
        let room = Arc::new(room);
        rooms.insert(node.id, room.clone());
        room
    };
    tokio::spawn(persist::saver(state.clone(), room.clone()));
    tracing::info!(node_id = %node.id, "co-editing room opened");
    Ok(room)
}

/// A peer left: once the room is empty it is saved and dropped. If the save
/// failed the room stays, and its saver tries again and drops it later.
pub async fn left(state: &AppState, room: &Arc<Room>, remaining: usize) {
    if remaining > 0 {
        return;
    }
    persist::save(state, room).await;
    retire_idle(state, room);
}

fn retire_idle(state: &AppState, room: &Arc<Room>) -> bool {
    let retired = state.collab.retire(room);
    if retired {
        // Ends the saver.
        room.changed.notify_one();
        tracing::info!(node_id = %room.node_id, "co-editing room closed");
    }
    retired
}

/// Server shutdown: stops every room (no more edits), saves it, then lets
/// its peers go (they reconnect to the next server).
pub async fn shutdown(state: &AppState) {
    let rooms: Vec<Arc<Room>> = state.collab.lock().values().cloned().collect();
    for room in rooms {
        room.close(CloseReason::Restarting);
        persist::save(state, &room).await;
        retire_idle(state, &room);
    }
}

impl From<EditableError> for ApiError {
    fn from(err: EditableError) -> Self {
        match err {
            EditableError::NotEditable(reason) => ApiError::NotEditable(reason),
            EditableError::Api(err) => err,
        }
    }
}
