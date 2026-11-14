//! Per-upload in-process locks: chunk writes share an upload's lock, completion
//! takes it exclusively from the claim until the blob is adopted and linked, so
//! no write can land between verification and adoption.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
use uuid::Uuid;

type Entries = Arc<Mutex<HashMap<Uuid, Arc<RwLock<()>>>>>;

/// Lock registry; an entry lives only while someone holds or awaits it.
#[derive(Debug, Clone, Default)]
pub struct UploadLocks {
    entries: Entries,
}

/// Held while writing a chunk of the upload.
#[must_use]
pub struct ChunkGuard {
    _held: Held<OwnedRwLockReadGuard<()>>,
}

/// Held while completing the upload; excludes every chunk write.
#[must_use]
pub struct CompletionGuard {
    _held: Held<OwnedRwLockWriteGuard<()>>,
}

impl UploadLocks {
    fn entry(&self, id: Uuid) -> Arc<RwLock<()>> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.entry(id).or_default().clone()
    }

    /// The entry for `id` and an empty holder that cleans it up when dropped,
    /// created before awaiting so a timed-out wait cleans up too. (A wait
    /// cancelled mid-way may leave an empty entry; the next holder of that id
    /// removes it.)
    fn enter<G>(&self, id: Uuid) -> (Arc<RwLock<()>>, Held<G>) {
        let held = Held {
            guard: None,
            id,
            entries: self.entries.clone(),
        };
        (self.entry(id), held)
    }

    /// `None` if a completion keeps the lock past `wait`.
    pub async fn chunk(&self, id: Uuid, wait: Duration) -> Option<ChunkGuard> {
        let (lock, mut held) = self.enter(id);
        // The timeout future owns (and drops) `lock` before `held` is dropped.
        held.guard = Some(tokio::time::timeout(wait, lock.read_owned()).await.ok()?);
        Some(ChunkGuard { _held: held })
    }

    /// `None` if chunk writes or another completion keep the lock past `wait`.
    pub async fn completion(&self, id: Uuid, wait: Duration) -> Option<CompletionGuard> {
        let (lock, mut held) = self.enter(id);
        // The timeout future owns (and drops) `lock` before `held` is dropped.
        held.guard = Some(tokio::time::timeout(wait, lock.write_owned()).await.ok()?);
        Some(CompletionGuard { _held: held })
    }

    /// Uploads with a lock entry (tests: entries must not leak).
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A guard (or a pending wait) that removes the registry entry once nobody
/// else holds or awaits it.
struct Held<G> {
    guard: Option<G>,
    id: Uuid,
    entries: Entries,
}

impl<G> Drop for Held<G> {
    fn drop(&mut self) {
        // Release the lock (and its `Arc`) first, then check under the map mutex:
        // waiters clone the `Arc` under that mutex, so a count of 1 means nobody.
        drop(self.guard.take());
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries
            .get(&self.id)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            entries.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn completion_excludes_chunk_writes() {
        let locks = UploadLocks::default();
        let id = Uuid::now_v7();
        let chunk = locks.chunk(id, WAIT).await.unwrap();
        assert!(locks.completion(id, WAIT).await.is_none());
        drop(chunk);
        let done = locks.completion(id, WAIT).await.expect("free");
        assert!(
            locks.chunk(id, WAIT).await.is_none(),
            "a chunk write waits for the completion, then gives up"
        );
        drop(done);
        drop(locks.chunk(id, WAIT).await.expect("free again"));
    }

    #[tokio::test]
    async fn chunk_writes_share_the_lock() {
        let locks = UploadLocks::default();
        let id = Uuid::now_v7();
        let _a = locks.chunk(id, WAIT).await.unwrap();
        let _b = locks.chunk(id, WAIT).await.expect("shared");
    }

    #[tokio::test]
    async fn entries_are_removed_when_released() {
        let locks = UploadLocks::default();
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        let first = locks.chunk(a, WAIT).await.unwrap();
        let second = locks.chunk(a, WAIT).await.unwrap();
        let other = locks.completion(b, WAIT).await.unwrap();
        assert_eq!(locks.len(), 2);
        drop(first);
        assert_eq!(locks.len(), 2, "still held by the second writer");
        drop((second, other));
        assert!(locks.is_empty());
        // A timed-out wait leaves nothing behind either.
        let held = locks.chunk(a, WAIT).await.unwrap();
        assert!(locks.completion(a, WAIT).await.is_none());
        drop(held);
        assert!(locks.is_empty());
        let completing = locks.completion(a, WAIT).await.unwrap();
        assert!(locks.chunk(a, WAIT).await.is_none());
        drop(completing);
        assert!(locks.is_empty());
    }
}
