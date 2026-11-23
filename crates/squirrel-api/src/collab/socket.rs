//! One WebSocket connection to a room.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use squirrel_db::nodes::NodeRecord;
use squirrel_db::{access, sessions};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;

use super::room::{CloseReason, Event, Peer, Refused, Room};
use crate::state::AppState;

/// Malformed protocol data (close code).
const BAD_DATA: u16 = 4400;
/// The peer fell behind the room: reconnect to resync (close code).
const TRY_AGAIN: u16 = 1013;

static NEXT_PEER: AtomicU64 = AtomicU64::new(1);

/// Who connected, as checked before the upgrade.
pub struct Connection {
    pub user_id: Uuid,
    /// Hash of the session cookie, re-checked while connected.
    pub session_hash: Vec<u8>,
    pub node: NodeRecord,
    pub can_write: bool,
    pub epoch: Uuid,
}

pub async fn close(mut socket: WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    // The client may already be gone.
    let _ = socket.send(Message::Close(Some(frame))).await;
}

pub async fn run(state: AppState, mut socket: WebSocket, connection: Connection) {
    let (room, greeting) = match super::join(&state, &connection.node, connection.epoch).await {
        Ok(joined) => joined,
        Err(reason) => return close(socket, reason.code(), reason.text()).await,
    };
    let mut peer = Peer::new(
        NEXT_PEER.fetch_add(1, Ordering::Relaxed),
        connection.can_write,
    );
    let mut events = room.subscribe();
    tracing::debug!(node_id = %room.node_id, user_id = %connection.user_id, peer = peer.id,
        can_write = peer.can_write, "co-editing peer joined");
    let mut greeted = true;
    for message in greeting {
        if socket.send(Message::Binary(message.into())).await.is_err() {
            greeted = false;
            break;
        }
    }
    let ended = if greeted {
        serve(
            &state,
            &room,
            &mut socket,
            &mut events,
            &mut peer,
            &connection,
        )
        .await
    } else {
        None
    };
    if let Some((code, reason)) = ended {
        close(socket, code, reason).await;
    }
    let remaining = room.leave(&peer);
    super::left(&state, &room, remaining).await;
}

/// Relays until either side ends. `Some`: the close frame to send.
async fn serve(
    state: &AppState,
    room: &Arc<Room>,
    socket: &mut WebSocket,
    events: &mut tokio::sync::broadcast::Receiver<Event>,
    peer: &mut Peer,
    connection: &Connection,
) -> Option<(u16, &'static str)> {
    let mut recheck = interval(state.config.collab.recheck_every);
    recheck.set_missed_tick_behavior(MissedTickBehavior::Delay);
    recheck.tick().await;
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let data = match incoming {
                    Some(Ok(Message::Binary(data))) => data,
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return None,
                    Some(Ok(_)) => continue,
                };
                match room.receive(peer, &data) {
                    Ok(handled) => {
                        for reply in handled.replies {
                            if socket.send(Message::Binary(reply.into())).await.is_err() {
                                return None;
                            }
                        }
                    }
                    Err(Refused::TooLarge(_)) => {
                        let reason = CloseReason::TooLarge;
                        return Some((reason.code(), reason.text()));
                    }
                    Err(Refused::Malformed(err)) => {
                        tracing::info!(node_id = %room.node_id, error = %err, "malformed co-editing message");
                        return Some((BAD_DATA, "malformed message"));
                    }
                }
            }
            event = events.recv() => match event {
                Ok(Event::Data { from, bytes }) if from != peer.id => {
                    if socket.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                        return None;
                    }
                }
                Ok(Event::Data { .. }) => {}
                Ok(Event::Close(reason)) => return Some((reason.code(), reason.text())),
                Err(RecvError::Lagged(_)) => return Some((TRY_AGAIN, "fell behind, reconnect")),
                Err(RecvError::Closed) => return None,
            },
            _ = recheck.tick() => {
                if let Some(reason) = still_allowed(state, connection, peer).await {
                    return Some((reason.code(), reason.text()));
                }
            }
        }
    }
}

/// Re-checks the session and the access (a share may have been revoked or
/// downgraded, the file trashed). `Some`: why the peer must leave.
async fn still_allowed(
    state: &AppState,
    connection: &Connection,
    peer: &Peer,
) -> Option<CloseReason> {
    match sessions::find_active(&state.pool, &connection.session_hash).await {
        Ok(Some(_)) => {}
        Ok(None) => return Some(CloseReason::SignedOut),
        // A database hiccup is not a reason to drop a connection.
        Err(err) => {
            tracing::warn!(error = %err, "co-editing session re-check failed");
            return None;
        }
    }
    match access::node(&state.pool, connection.user_id, connection.node.id).await {
        // Upgraded or downgraded: the client reopens to learn its new rights.
        Ok(Some(found)) if found.access.can_write() != peer.can_write => Some(CloseReason::Stale),
        Ok(Some(_)) => None,
        Ok(None) => Some(CloseReason::Gone),
        Err(err) => {
            tracing::warn!(error = %err, "co-editing access re-check failed");
            None
        }
    }
}
