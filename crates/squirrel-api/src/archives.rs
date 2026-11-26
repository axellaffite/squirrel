//! Zip download tickets: what `POST /api/files/archive` checked, kept in
//! memory until `GET /api/files/archive/{ticket}` streams it.
//!
//! A ticket is a random 256-bit token, usable only by the user who created
//! it, any number of times until it expires (a download may be resumed).
//! Tickets do not survive a restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::zip;

/// How long a ticket stays usable after its creation.
pub const TICKET_TTL: Duration = Duration::from_secs(10 * 60);
/// Live tickets one user may hold; creating one more drops their oldest.
const MAX_TICKETS_PER_USER: usize = 32;

/// One file of an archive.
#[derive(Debug, Clone)]
pub struct ArchiveFile {
    pub node_id: Uuid,
    pub hash: ContentHash,
    /// Its name, size and date inside the zip.
    pub entry: zip::Entry,
}

/// A zip ready to be streamed.
#[derive(Debug)]
pub struct Archive {
    pub owner_id: Uuid,
    /// Suggested file name (`… .zip`).
    pub name: String,
    /// Exact byte length of the zip.
    pub size: u64,
    pub files: Vec<ArchiveFile>,
}

struct Ticket {
    archive: Arc<Archive>,
    created: Instant,
}

/// The live tickets, by token.
pub struct ArchiveTickets {
    ttl: Duration,
    tickets: Mutex<HashMap<String, Ticket>>,
}

impl Default for ArchiveTickets {
    fn default() -> Self {
        Self::new(TICKET_TTL)
    }
}

impl ArchiveTickets {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            tickets: Mutex::default(),
        }
    }

    /// Keeps `archive` and returns its new ticket.
    pub fn insert(&self, archive: Archive) -> String {
        self.insert_at(archive, Instant::now())
    }

    /// The archive of `token` if it is live and `user` created it.
    pub fn get(&self, token: &str, user: Uuid) -> Option<Arc<Archive>> {
        self.get_at(token, user, Instant::now())
    }

    fn insert_at(&self, archive: Archive, now: Instant) -> String {
        let token = squirrel_auth::generate_token();
        let mut tickets = self.tickets.lock().unwrap_or_else(PoisonError::into_inner);
        tickets.retain(|_, ticket| now.duration_since(ticket.created) < self.ttl);
        let owner = archive.owner_id;
        let mut own: Vec<(Instant, String)> = tickets
            .iter()
            .filter(|(_, ticket)| ticket.archive.owner_id == owner)
            .map(|(token, ticket)| (ticket.created, token.clone()))
            .collect();
        if own.len() >= MAX_TICKETS_PER_USER {
            own.sort();
            let excess = own.len() + 1 - MAX_TICKETS_PER_USER;
            for (_, oldest) in own.into_iter().take(excess) {
                tickets.remove(&oldest);
            }
        }
        tickets.insert(
            token.clone(),
            Ticket {
                archive: Arc::new(archive),
                created: now,
            },
        );
        token
    }

    fn get_at(&self, token: &str, user: Uuid, now: Instant) -> Option<Arc<Archive>> {
        let tickets = self.tickets.lock().unwrap_or_else(PoisonError::into_inner);
        tickets
            .get(token)
            .filter(|ticket| now.duration_since(ticket.created) < self.ttl)
            .filter(|ticket| ticket.archive.owner_id == user)
            .map(|ticket| ticket.archive.clone())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.tickets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(owner_id: Uuid) -> Archive {
        Archive {
            owner_id,
            name: "Squirrel 0 files 2026-01-01.zip".into(),
            size: 22,
            files: Vec::new(),
        }
    }

    #[test]
    fn a_ticket_serves_its_creator_until_it_expires() {
        let tickets = ArchiveTickets::new(Duration::from_secs(600));
        let (alice, bob) = (Uuid::now_v7(), Uuid::now_v7());
        let start = Instant::now();
        let token = tickets.insert_at(archive(alice), start);
        assert_eq!(token.len(), 43, "32 random bytes, base64url");

        let later = start + Duration::from_secs(599);
        assert!(tickets.get_at(&token, alice, later).is_some());
        assert!(tickets.get_at(&token, alice, later).is_some(), "reusable");
        assert!(tickets.get_at(&token, bob, later).is_none(), "another user");
        assert!(tickets.get_at("unknown", alice, later).is_none());
        let expired = start + Duration::from_secs(600);
        assert!(tickets.get_at(&token, alice, expired).is_none());
    }

    #[test]
    fn expired_tickets_are_purged_on_insert() {
        let tickets = ArchiveTickets::new(Duration::from_secs(10));
        let user = Uuid::now_v7();
        let start = Instant::now();
        tickets.insert_at(archive(user), start);
        tickets.insert_at(archive(user), start + Duration::from_secs(5));
        assert_eq!(tickets.len(), 2);
        tickets.insert_at(archive(user), start + Duration::from_secs(12));
        assert_eq!(tickets.len(), 2, "the first one expired");
    }

    #[test]
    fn a_user_holds_a_bounded_number_of_tickets() {
        let tickets = ArchiveTickets::default();
        let (alice, bob) = (Uuid::now_v7(), Uuid::now_v7());
        let start = Instant::now();
        let bobs = tickets.insert_at(archive(bob), start);
        let first = tickets.insert_at(archive(alice), start);
        let mut last = String::new();
        for i in 1..=MAX_TICKETS_PER_USER {
            let at = start + Duration::from_millis(u64::try_from(i).unwrap());
            last = tickets.insert_at(archive(alice), at);
        }
        let now = start + Duration::from_secs(1);
        assert!(
            tickets.get_at(&first, alice, now).is_none(),
            "oldest dropped"
        );
        assert!(tickets.get_at(&last, alice, now).is_some());
        assert!(
            tickets.get_at(&bobs, bob, now).is_some(),
            "others untouched"
        );
        assert_eq!(tickets.len(), MAX_TICKETS_PER_USER + 1);
    }
}
