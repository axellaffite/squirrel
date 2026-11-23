//! One co-edited file in memory: its Yjs document and awareness, the people
//! connected to it, and the y-sync protocol (the same wire format as the
//! `y-websocket` client). Everything here is synchronous: no lock is held
//! across an `.await`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::{Notify, broadcast};
use uuid::Uuid;
use yrs::encoding::read::Cursor;
use yrs::sync::AwarenessUpdate;
use yrs::sync::{Awareness, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, GetString, ReadTxn, StateVector, Text, Transact, Update};

/// The shared text inside each document (the client binds the same name).
pub const TEXT_NAME: &str = "content";
/// Buffered events per room; a peer lagging further behind is disconnected.
const EVENT_BUFFER: usize = 1024;
/// Awareness clients one connection may announce (one per tab is normal).
const MAX_CLIENTS_PER_PEER: usize = 4;

/// Why a room or a peer is closed, as a WebSocket close code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The session ended.
    SignedOut,
    /// No access any more, or the file was trashed or deleted.
    Gone,
    /// The file changed outside the editor: reopen with a fresh document.
    Stale,
    /// An update would make the text larger than the limit.
    TooLarge,
    /// Too many rooms or people.
    Busy,
    /// The server is restarting: reconnect.
    Restarting,
    /// The owner's storage is full: the latest edits could not be saved.
    StorageFull,
}

impl CloseReason {
    pub fn code(self) -> u16 {
        match self {
            Self::SignedOut => 4401,
            Self::Gone => 4404,
            Self::Stale => 4409,
            Self::TooLarge => 4413,
            Self::Busy => 4429,
            Self::Restarting => 1012,
            Self::StorageFull => 4507,
        }
    }

    /// Whether unsaved changes may still be saved after closing for this reason
    /// (not when the file is gone or was replaced).
    fn keeps_changes(self) -> bool {
        matches!(self, Self::Restarting | Self::Busy | Self::SignedOut)
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::SignedOut => "signed out",
            Self::Gone => "no longer available",
            Self::Stale => "the file changed: reopen it",
            Self::TooLarge => "the document is too large",
            Self::Busy => "too many people editing",
            Self::Restarting => "the server is restarting",
            Self::StorageFull => "the owner's storage is full",
        }
    }
}

/// What a room tells its peers.
#[derive(Debug, Clone)]
pub enum Event {
    /// Protocol bytes from peer `from`, for everyone else.
    Data { from: u64, bytes: Arc<[u8]> },
    /// Everyone leaves.
    Close(CloseReason),
}

/// One connection's view of the room.
#[derive(Debug)]
pub struct Peer {
    pub id: u64,
    pub can_write: bool,
    /// Awareness clients this connection announced, cleared when it leaves.
    clients: HashSet<ClientID>,
}

impl Peer {
    pub fn new(id: u64, can_write: bool) -> Self {
        Self {
            id,
            can_write,
            clients: HashSet::new(),
        }
    }
}

/// The result of one incoming message batch.
#[derive(Debug, Default)]
pub struct Handled {
    /// Messages for the sender only.
    pub replies: Vec<Vec<u8>>,
    /// The document changed (it needs saving).
    pub changed: bool,
}

/// Why [`Room::join`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    /// The room was dropped from the registry meanwhile: look it up again.
    Retired,
    Refused(CloseReason),
}

#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("malformed message: {0}")]
    Malformed(String),
    #[error("the text would exceed {0} bytes")]
    TooLarge(usize),
}

/// A snapshot to save.
#[derive(Debug)]
pub struct Snapshot {
    pub text: String,
    pub state: Vec<u8>,
    pub epoch: Uuid,
    pub base_sha: String,
}

pub struct Room {
    pub node_id: Uuid,
    pub owner_id: Uuid,
    max_bytes: usize,
    state: Mutex<RoomState>,
    events: broadcast::Sender<Event>,
    /// Wakes the saver after a change.
    pub changed: Notify,
    /// One save at a time: a save snapshots, writes and records the new blob
    /// as one step, so two saves never race on the same base blob.
    pub saving: tokio::sync::Mutex<()>,
}

struct RoomState {
    awareness: Awareness,
    epoch: Uuid,
    /// The blob the file references, which the document was last saved to.
    base_sha: String,
    /// Changes not saved yet.
    dirty: bool,
    peers: usize,
    closed: Option<CloseReason>,
    /// Removed from the registry: joins must look the room up again.
    retired: bool,
    /// Which peer announced each awareness client.
    owners: HashMap<ClientID, u64>,
}

impl Room {
    /// A room for the stored Yjs `state` (built from blob `base_sha`).
    pub fn load(
        node_id: Uuid,
        owner_id: Uuid,
        epoch: Uuid,
        base_sha: String,
        state: &[u8],
        max_bytes: usize,
    ) -> Result<Self, Refused> {
        let doc = Doc::new();
        let update = Update::decode_v1(state).map_err(|err| Refused::Malformed(err.to_string()))?;
        doc.transact_mut()
            .apply_update(update)
            .map_err(|err| Refused::Malformed(err.to_string()))?;
        Ok(Self {
            node_id,
            owner_id,
            max_bytes,
            state: Mutex::new(RoomState {
                awareness: Awareness::new(doc),
                epoch,
                base_sha,
                dirty: false,
                peers: 0,
                closed: None,
                retired: false,
                owners: HashMap::new(),
            }),
            events: broadcast::channel(EVENT_BUFFER).0,
            changed: Notify::new(),
            saving: tokio::sync::Mutex::new(()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, RoomState> {
        // A panic while holding the lock leaves a consistent CRDT (yrs
        // transactions are applied whole), so a poisoned lock is still usable.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn epoch(&self) -> Uuid {
        self.lock().epoch
    }

    /// Adds a peer unless the room is closed or full. Returns its greeting
    /// (sync step 1 and the current awareness), one message per WebSocket
    /// frame: the y-websocket client reads a single message from each frame.
    pub fn join(&self, max_peers: usize) -> Result<Vec<Vec<u8>>, JoinError> {
        let mut state = self.lock();
        if state.retired {
            return Err(JoinError::Retired);
        }
        if let Some(reason) = state.closed {
            return Err(JoinError::Refused(reason));
        }
        if state.peers >= max_peers {
            return Err(JoinError::Refused(CloseReason::Busy));
        }
        state.peers += 1;
        let sv = state.awareness.doc().transact().state_vector();
        let mut greeting = vec![Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1()];
        if let Ok(update) = state.awareness.update() {
            greeting.push(Message::Awareness(update).encode_v1());
        }
        Ok(greeting)
    }

    /// Removes a peer and its awareness states (announced to the others).
    /// Returns how many peers remain.
    pub fn leave(&self, peer: &Peer) -> usize {
        let mut state = self.lock();
        state.peers = state.peers.saturating_sub(1);
        let clients: Vec<ClientID> = peer.clients.iter().copied().collect();
        for client in &clients {
            state.owners.remove(client);
        }
        if !clients.is_empty() {
            for client in &clients {
                state.awareness.remove_state(*client);
            }
            if let Ok(update) = state.awareness.update_with_clients(clients) {
                self.send(Event::Data {
                    from: peer.id,
                    bytes: Message::Awareness(update).encode_v1().into(),
                });
            }
        }
        state.peers
    }

    pub fn peers(&self) -> usize {
        self.lock().peers
    }

    /// Marks the room retired if nobody is in it and nothing is unsaved (or
    /// it was closed). Call with the registry locked, then unregister it.
    pub fn retire_if_idle(&self) -> bool {
        let mut state = self.lock();
        let idle = state.peers == 0 && !state.dirty;
        if idle || state.closed.is_some() {
            state.retired = true;
        }
        state.retired
    }

    pub fn is_retired(&self) -> bool {
        self.lock().retired
    }

    /// Closes the room for everyone; later joins are refused with `reason`.
    pub fn close(&self, reason: CloseReason) {
        let mut state = self.lock();
        if state.closed.is_none() {
            state.closed = Some(reason);
            self.send(Event::Close(reason));
        }
    }

    pub fn closed(&self) -> Option<CloseReason> {
        self.lock().closed
    }

    fn send(&self, event: Event) {
        // No receiver is fine: nobody is connected.
        let _ = self.events.send(event);
    }

    /// Handles one binary WebSocket message from `peer`.
    pub fn receive(&self, peer: &mut Peer, data: &[u8]) -> Result<Handled, Refused> {
        let mut decoder = DecoderV1::new(Cursor::new(data));
        let messages = MessageReader::new(&mut decoder)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| Refused::Malformed(err.to_string()))?;
        let mut state = self.lock();
        let mut handled = Handled::default();
        for message in messages {
            match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let update = state
                        .awareness
                        .doc()
                        .transact()
                        .encode_state_as_update_v1(&sv);
                    handled
                        .replies
                        .push(Message::Sync(SyncMessage::SyncStep2(update)).encode_v1());
                }
                Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                    // A reader's document changes are ignored: the server is the
                    // only path between peers, so they never reach anyone.
                    let open = state.closed.is_none() && !state.retired;
                    if peer.can_write && open && self.apply(&mut state, &update)? {
                        handled.changed = true;
                        self.send(Event::Data {
                            from: peer.id,
                            bytes: Message::Sync(SyncMessage::Update(update))
                                .encode_v1()
                                .into(),
                        });
                    }
                }
                Message::Awareness(update) => {
                    let update = claim(&mut state, peer, update);
                    if update.clients.is_empty() {
                        continue;
                    }
                    let bytes = Message::Awareness(update.clone()).encode_v1();
                    state
                        .awareness
                        .apply_update(update)
                        .map_err(|err| Refused::Malformed(err.to_string()))?;
                    self.send(Event::Data {
                        from: peer.id,
                        bytes: bytes.into(),
                    });
                }
                Message::AwarenessQuery => {
                    if let Ok(update) = state.awareness.update() {
                        handled.replies.push(Message::Awareness(update).encode_v1());
                    }
                }
                Message::Auth(_) | Message::Custom(..) => {}
            }
        }
        if handled.changed {
            state.dirty = true;
            drop(state);
            self.changed.notify_one();
        }
        Ok(handled)
    }

    /// Applies a document update if the text stays within the limit.
    /// `false`: it changed nothing.
    fn apply(&self, state: &mut RoomState, update: &[u8]) -> Result<bool, Refused> {
        if update_is_empty(update) {
            return Ok(false);
        }
        let decoded =
            Update::decode_v1(update).map_err(|err| Refused::Malformed(err.to_string()))?;
        let doc = state.awareness.doc();
        // Updates arrive in causal order through this server: one starting
        // past what the document has cannot be integrated (yrs would keep it
        // pending, outside the size check).
        let known = doc.transact().state_vector();
        if decoded
            .state_vector_lower()
            .iter()
            .any(|(client, clock)| *clock > known.get(client))
        {
            return Err(Refused::Malformed(
                "an update depends on missing changes".into(),
            ));
        }
        let current = text_len(doc);
        // The inserted text is inside the update, so a small update cannot
        // cross the limit: only then is it tried on a copy first.
        if current + update.len() > self.max_bytes {
            let trial = Doc::new();
            let full = doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            {
                let mut txn = trial.transact_mut();
                for bytes in [full.as_slice(), update] {
                    let part = Update::decode_v1(bytes)
                        .map_err(|err| Refused::Malformed(err.to_string()))?;
                    txn.apply_update(part)
                        .map_err(|err| Refused::Malformed(err.to_string()))?;
                }
            }
            if text_len(&trial) > self.max_bytes {
                return Err(Refused::TooLarge(self.max_bytes));
            }
        }
        let mut txn = doc.transact_mut();
        txn.apply_update(decoded)
            .map_err(|err| Refused::Malformed(err.to_string()))?;
        if txn.has_missing_updates() {
            // Left pending (it references unknown items): the sender is
            // disconnected; saving still checks the size of the text.
            return Err(Refused::Malformed(
                "an update depends on missing changes".into(),
            ));
        }
        Ok(true)
    }

    /// The text and state to save, if anything changed since the last save
    /// (which then counts as saved; call [`Room::save_failed`] if it fails).
    pub fn take_dirty(&self) -> Option<Snapshot> {
        let mut state = self.lock();
        let keeps = state.closed.is_none_or(CloseReason::keeps_changes);
        if !state.dirty || !keeps {
            return None;
        }
        state.dirty = false;
        let doc = state.awareness.doc();
        let txn = doc.transact();
        let text = txn
            .get_text(TEXT_NAME)
            .map(|text| text.get_string(&txn))
            .unwrap_or_default();
        Some(Snapshot {
            text,
            state: txn.encode_state_as_update_v1(&StateVector::default()),
            epoch: state.epoch,
            base_sha: state.base_sha.clone(),
        })
    }

    /// The save failed: the changes stay unsaved (the saver tries again).
    pub fn save_failed(&self) {
        self.lock().dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.lock().dirty
    }

    /// The file now references blob `sha`.
    pub fn saved(&self, sha: String) {
        self.lock().base_sha = sha;
    }
}

/// The part of `update` that `peer` may announce: its own clients, which no
/// other connection claimed, at most [`MAX_CLIENTS_PER_PEER`] of them.
fn claim(state: &mut RoomState, peer: &mut Peer, update: AwarenessUpdate) -> AwarenessUpdate {
    let clients = update
        .clients
        .into_iter()
        .filter(|(client, _)| match state.owners.get(client) {
            Some(owner) => *owner == peer.id,
            None if peer.clients.len() < MAX_CLIENTS_PER_PEER => {
                state.owners.insert(*client, peer.id);
                peer.clients.insert(*client);
                true
            }
            None => false,
        })
        .collect();
    AwarenessUpdate { clients }
}

/// The Yjs state of a new document holding `text`.
pub fn initial_state(text: &str) -> Vec<u8> {
    let doc = Doc::new();
    let shared = doc.get_or_insert_text(TEXT_NAME);
    let mut txn = doc.transact_mut();
    shared.insert(&mut txn, 0, text);
    txn.encode_state_as_update_v1(&StateVector::default())
}

fn text_len(doc: &Doc) -> usize {
    let txn = doc.transact();
    txn.get_text(TEXT_NAME)
        .map_or(0, |text| text.len(&txn) as usize)
}

/// An encoded update with no structs and no deletions (what an empty
/// client sends in sync step 2).
fn update_is_empty(update: &[u8]) -> bool {
    update == [0, 0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::updates::encoder::Encode;

    const MAX: usize = 64;

    fn room(text: &str) -> Room {
        Room::load(
            Uuid::now_v7(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            "a".repeat(64),
            &initial_state(text),
            MAX,
        )
        .unwrap()
    }

    /// A client document synced with `room`, as the browser would hold it.
    fn client_of(room: &Room) -> Doc {
        let client = Doc::new();
        let state = room
            .lock()
            .awareness
            .doc()
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        client
            .transact_mut()
            .apply_update(Update::decode_v1(&state).unwrap())
            .unwrap();
        client
    }

    fn edit(client: &Doc, index: u32, insert: &str) -> Vec<u8> {
        let text = client.get_or_insert_text(TEXT_NAME);
        let before = client.transact().state_vector();
        {
            let mut txn = client.transact_mut();
            text.insert(&mut txn, index, insert);
        }
        let update = client.transact().encode_state_as_update_v1(&before);
        Message::Sync(SyncMessage::Update(update)).encode_v1()
    }

    fn text_of(room: &Room) -> String {
        let state = room.lock();
        let txn = state.awareness.doc().transact();
        txn.get_text(TEXT_NAME).unwrap().get_string(&txn)
    }

    #[test]
    fn a_writer_edit_is_applied_broadcast_and_marked_dirty() {
        let room = room("hello");
        let mut events = room.subscribe();
        let client = client_of(&room);
        let mut writer = Peer::new(1, true);
        let handled = room
            .receive(&mut writer, &edit(&client, 5, " world"))
            .unwrap();
        assert!(handled.changed);
        assert_eq!(text_of(&room), "hello world");
        assert!(matches!(events.try_recv(), Ok(Event::Data { from: 1, .. })));
        let snapshot = room.take_dirty().unwrap();
        assert_eq!(snapshot.text, "hello world");
        assert!(room.take_dirty().is_none());
    }

    #[test]
    fn a_reader_edit_is_ignored() {
        let room = room("hello");
        let client = client_of(&room);
        let mut reader = Peer::new(2, false);
        let handled = room.receive(&mut reader, &edit(&client, 0, "x")).unwrap();
        assert!(!handled.changed);
        assert_eq!(text_of(&room), "hello");
        assert!(room.take_dirty().is_none());
    }

    #[test]
    fn sync_step1_is_answered_with_the_missing_state() {
        let room = room("hello");
        let mut peer = Peer::new(1, false);
        let step1 = Message::Sync(SyncMessage::SyncStep1(StateVector::default())).encode_v1();
        let handled = room.receive(&mut peer, &step1).unwrap();
        let reply = Message::decode_v1(&handled.replies[0]).unwrap();
        let Message::Sync(SyncMessage::SyncStep2(update)) = reply else {
            panic!("expected sync step 2, got {reply:?}");
        };
        let client = Doc::new();
        client
            .transact_mut()
            .apply_update(Update::decode_v1(&update).unwrap())
            .unwrap();
        let txn = client.transact();
        assert_eq!(txn.get_text(TEXT_NAME).unwrap().get_string(&txn), "hello");
    }

    #[test]
    fn an_update_past_the_limit_is_refused_and_not_applied() {
        let room = room("hello");
        let client = client_of(&room);
        let mut writer = Peer::new(1, true);
        let big = "x".repeat(MAX);
        let refused = room.receive(&mut writer, &edit(&client, 0, &big));
        assert!(matches!(refused, Err(Refused::TooLarge(MAX))));
        assert_eq!(text_of(&room), "hello");
    }

    #[test]
    fn garbage_is_malformed() {
        let room = room("");
        let mut peer = Peer::new(1, true);
        assert!(matches!(
            room.receive(&mut peer, &[0, 9, 9]),
            Err(Refused::Malformed(_))
        ));
    }

    #[test]
    fn leaving_clears_the_peer_awareness_for_the_others() {
        let room = room("");
        let mut peer = Peer::new(1, true);
        let remote = Awareness::new(Doc::new());
        remote.set_local_state_raw(r#"{"user":{"name":"bob"}}"#);
        let announce = Message::Awareness(remote.update().unwrap()).encode_v1();
        room.join(8).unwrap();
        room.receive(&mut peer, &announce).unwrap();
        let mut events = room.subscribe();
        assert_eq!(room.leave(&peer), 0);
        let Ok(Event::Data { bytes, .. }) = events.try_recv() else {
            panic!("expected an awareness removal");
        };
        let Message::Awareness(update) = Message::decode_v1(&bytes).unwrap() else {
            panic!("expected awareness");
        };
        assert!(update.clients.values().all(|entry| &*entry.json == "null"));
    }

    #[test]
    fn the_greeting_sends_one_message_per_frame_with_present_people() {
        let room = room("hi");
        room.join(8).unwrap();
        let mut first = Peer::new(1, true);
        let remote = Awareness::new(Doc::new());
        remote.set_local_state_raw(r#"{"user":{"name":"alice"}}"#);
        let announce = Message::Awareness(remote.update().unwrap()).encode_v1();
        room.receive(&mut first, &announce).unwrap();

        let greeting = room.join(8).unwrap();
        // y-websocket decodes a single message from each frame.
        let decoded: Vec<Message> = greeting
            .iter()
            .map(|frame| {
                let mut decoder = DecoderV1::new(Cursor::new(frame));
                let messages: Vec<Message> = MessageReader::new(&mut decoder)
                    .collect::<Result<_, _>>()
                    .unwrap();
                assert_eq!(messages.len(), 1, "one message per frame");
                messages.into_iter().next().unwrap()
            })
            .collect();
        assert!(matches!(
            decoded[0],
            Message::Sync(SyncMessage::SyncStep1(_))
        ));
        let Message::Awareness(update) = &decoded[1] else {
            panic!("expected awareness, got {:?}", decoded[1]);
        };
        assert!(
            update
                .clients
                .values()
                .any(|entry| entry.json.contains("alice"))
        );
    }

    #[test]
    fn a_closed_room_refuses_joins_and_a_full_one_is_busy() {
        let room = room("");
        room.join(1).unwrap();
        assert_eq!(room.join(1), Err(JoinError::Refused(CloseReason::Busy)));
        room.close(CloseReason::Stale);
        assert_eq!(room.join(8), Err(JoinError::Refused(CloseReason::Stale)));
    }

    #[test]
    fn a_retired_room_sends_joiners_back_to_the_registry() {
        let room = room("");
        let peer = Peer::new(1, true);
        room.join(8).unwrap();
        assert!(!room.retire_if_idle(), "someone is in it");
        room.leave(&peer);
        assert!(room.retire_if_idle());
        assert_eq!(room.join(8), Err(JoinError::Retired));
    }

    #[test]
    fn an_unsaved_room_is_not_retired() {
        let room = room("hello");
        let client = client_of(&room);
        let mut writer = Peer::new(1, true);
        room.join(8).unwrap();
        room.receive(&mut writer, &edit(&client, 0, "x")).unwrap();
        room.leave(&writer);
        assert!(!room.retire_if_idle());
        room.take_dirty().unwrap();
        assert!(room.retire_if_idle());
    }

    #[test]
    fn a_closed_room_still_saves_when_restarting_but_not_when_stale() {
        let restarting = room("hello");
        let client = client_of(&restarting);
        let mut writer = Peer::new(1, true);
        restarting
            .receive(&mut writer, &edit(&client, 0, "x"))
            .unwrap();
        restarting.close(CloseReason::Restarting);
        assert!(restarting.take_dirty().is_some());
        // Edits after closing are not taken.
        let handled = restarting
            .receive(&mut writer, &edit(&client, 0, "y"))
            .unwrap();
        assert!(!handled.changed);

        let stale = room("hello");
        let client = client_of(&stale);
        stale.receive(&mut writer, &edit(&client, 0, "x")).unwrap();
        stale.close(CloseReason::Stale);
        assert!(stale.take_dirty().is_none());
    }

    #[test]
    fn a_peer_cannot_announce_someone_else_awareness() {
        let room = room("");
        let (mut alice, mut mallory) = (Peer::new(1, true), Peer::new(2, false));
        let tab = Awareness::new(Doc::new());
        tab.set_local_state_raw(r#"{"user":{"name":"alice"}}"#);
        room.receive(
            &mut alice,
            &Message::Awareness(tab.update().unwrap()).encode_v1(),
        )
        .unwrap();
        let mut events = room.subscribe();
        tab.set_local_state_raw(r#"{"user":{"name":"spoofed"}}"#);
        room.receive(
            &mut mallory,
            &Message::Awareness(tab.update().unwrap()).encode_v1(),
        )
        .unwrap();
        assert!(events.try_recv().is_err(), "nothing relayed");
        let state = room.lock();
        let json = state.awareness.update().unwrap();
        assert!(
            json.clients
                .values()
                .all(|entry| entry.json.contains("alice"))
        );
    }

    #[test]
    fn an_update_with_missing_dependencies_is_refused() {
        let room = room("hello");
        let client = client_of(&room);
        let _skipped = edit(&client, 5, " one");
        let gap = edit(&client, 9, " two");
        let mut writer = Peer::new(1, true);
        assert!(matches!(
            room.receive(&mut writer, &gap),
            Err(Refused::Malformed(_))
        ));
        assert_eq!(text_of(&room), "hello");
    }
}
