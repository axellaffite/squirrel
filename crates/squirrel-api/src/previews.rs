//! Rendering lightbox previews on request: the sandboxed renderer, its slots
//! (cache hits take none), one render per cache key, and a per-user limit on
//! renders. Visitors of public links only ever hold a part of the slots.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use squirrel_media::preview::PreviewFormat;
use squirrel_media::thumbs::DecodeLimits;
use squirrel_sandbox::exec::Executor;
use squirrel_sandbox::preview::PreviewRenderer;
use squirrel_storage::CacheKey;
use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::upload_runtime::{UserRate, UserRateLimiter};

/// Preview limits (part of [`crate::ApiConfig`]).
#[derive(Debug, Clone)]
pub struct PreviewLimits {
    /// Renders at once, all users together (`SQUIRREL_PREVIEW_CONCURRENCY`).
    pub concurrency: usize,
    /// How many of those slots visitors of public links may hold together.
    /// Always at least one, and one fewer than `concurrency` at most (when
    /// there is more than one slot), so a slot stays free for the users.
    pub public_concurrency: usize,
    /// How long a render waits for a slot before answering 503 `busy`.
    pub slot_wait: Duration,
    /// Per-user limit on renders (cache misses). `None` disables it.
    pub render_rate: Option<UserRate>,
}

impl Default for PreviewLimits {
    fn default() -> Self {
        Self {
            concurrency: 2,
            public_concurrency: 1,
            slot_wait: Duration::from_secs(3),
            render_rate: Some(UserRate {
                per_second: 4.0,
                burst: 40,
            }),
        }
    }
}

impl PreviewLimits {
    /// The render slots visitors of public links may hold together.
    fn public_slots(&self) -> usize {
        let slots = self.concurrency.max(1);
        self.public_concurrency
            .clamp(1, slots.saturating_sub(1).max(1))
    }
}

/// A held render slot, released on drop.
#[must_use]
#[derive(Debug)]
pub struct RenderSlot {
    _slot: OwnedSemaphorePermit,
    _visitor: Option<OwnedSemaphorePermit>,
}

/// Keys with a render in flight: at most this many before stale ones are pruned.
const PRUNE_KEYS_ABOVE: usize = 256;

#[derive(Debug)]
pub struct Previews {
    pub renderer: PreviewRenderer,
    slots: Arc<Semaphore>,
    /// Taken before a slot by renders for public links.
    public_slots: Arc<Semaphore>,
    slot_wait: Duration,
    rate: Option<UserRateLimiter>,
    in_flight: Mutex<HashMap<CacheKey, Weak<KeyTurn>>>,
}

/// What the request that rendered a key ended with, for those that waited on
/// it: they answer the same instead of rendering again one after the other.
#[derive(Debug, Clone)]
pub enum Outcome {
    Preview(Arc<Vec<u8>>, PreviewFormat),
    /// Cannot be rendered now: the stored preview (if any) answers.
    Impossible,
    /// No slot, a timeout or an error: retry later.
    Busy,
}

/// The per-key lock; it holds the outcome once the first request is done.
pub type KeyTurn = tokio::sync::Mutex<Option<Outcome>>;

impl Previews {
    pub fn new(executor: Executor, limits: &PreviewLimits) -> Self {
        Self {
            renderer: PreviewRenderer::new(executor, DecodeLimits::default()),
            slots: Arc::new(Semaphore::new(limits.concurrency.max(1))),
            public_slots: Arc::new(Semaphore::new(limits.public_slots())),
            slot_wait: limits.slot_wait,
            rate: limits.render_rate.map(UserRateLimiter::new),
            in_flight: Mutex::default(),
        }
    }

    /// Takes one render token for `user`; `false` when they render too fast.
    pub fn allow_render(&self, user: Uuid) -> bool {
        self.rate.as_ref().is_none_or(|rate| rate.check(user))
    }

    /// The lock every request for `key` takes before rendering it: the first
    /// renders and records its [`Outcome`], the others answer with it.
    fn key_lock(&self, key: &CacheKey) -> Arc<KeyTurn> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(lock) = in_flight.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        if in_flight.len() >= PRUNE_KEYS_ABOVE {
            in_flight.retain(|_, lock| lock.strong_count() > 0);
        }
        let lock = Arc::new(tokio::sync::Mutex::new(None));
        in_flight.insert(key.clone(), Arc::downgrade(&lock));
        lock
    }

    /// This request's turn on `key`, or `None` when the one rendering it does
    /// not finish within the slot wait (answer busy). The guard holds the
    /// earlier request's outcome, if one finished meanwhile.
    pub async fn turn(&self, key: &CacheKey) -> Option<OwnedMutexGuard<Option<Outcome>>> {
        tokio::time::timeout(self.slot_wait, self.key_lock(key).lock_owned())
            .await
            .ok()
    }

    /// A render slot, or `None` when none frees up within the configured
    /// wait. A render for a public link (`visitor`) first needs one of the
    /// visitors' share of the slots.
    pub async fn slot(&self, visitor: bool) -> Option<RenderSlot> {
        let take = async {
            let visitor = if visitor {
                Some(self.public_slots.clone().acquire_owned().await.ok()?)
            } else {
                None
            };
            let slot = self.slots.clone().acquire_owned().await.ok()?;
            Some(RenderSlot {
                _slot: slot,
                _visitor: visitor,
            })
        };
        tokio::time::timeout(self.slot_wait, take).await.ok()?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_core::ContentHash;

    fn key() -> CacheKey {
        CacheKey {
            hash: ContentHash::parse(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            )
            .unwrap(),
            bucket: 1440,
        }
    }

    #[test]
    fn requests_for_one_key_share_a_lock_while_it_is_held() {
        let previews = Previews::new(Executor::InProcess, &PreviewLimits::default());
        let first = previews.key_lock(&key());
        assert!(Arc::ptr_eq(&first, &previews.key_lock(&key())));
        drop(first);
        let again = previews.key_lock(&key());
        assert_eq!(Arc::strong_count(&again), 1);
    }

    fn limits(wait_ms: u64) -> PreviewLimits {
        PreviewLimits {
            concurrency: 1,
            slot_wait: Duration::from_millis(wait_ms),
            render_rate: None,
            ..PreviewLimits::default()
        }
    }

    /// A failed render is not repeated by every request that waited for it.
    #[tokio::test]
    async fn waiters_get_the_first_requests_outcome() {
        let previews = Arc::new(Previews::new(Executor::InProcess, &limits(5_000)));
        let mut first = previews.turn(&key()).await.unwrap();
        assert!(first.is_none(), "nobody rendered it yet");
        let waiter = {
            let previews = previews.clone();
            tokio::spawn(async move { previews.turn(&key()).await.map(|turn| turn.clone()) })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        *first = Some(Outcome::Impossible);
        drop(first);
        let seen = waiter.await.unwrap().expect("its turn came");
        assert!(matches!(seen, Some(Outcome::Impossible)), "{seen:?}");
        // Once nobody holds the key, a later request starts afresh.
        assert!(previews.turn(&key()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn waiting_for_a_key_is_bounded_by_the_slot_wait() {
        let previews = Previews::new(Executor::InProcess, &limits(20));
        let held = previews.turn(&key()).await.unwrap();
        assert!(previews.turn(&key()).await.is_none());
        drop(held);
        assert!(previews.turn(&key()).await.is_some());
    }

    #[tokio::test]
    async fn slots_run_out_after_the_wait() {
        let previews = Previews::new(Executor::InProcess, &limits(20));
        let held = previews.slot(false).await.unwrap();
        assert!(previews.slot(false).await.is_none());
        drop(held);
        assert!(previews.slot(false).await.is_some());
        assert!(previews.allow_render(Uuid::now_v7()));
    }

    #[tokio::test]
    async fn visitors_leave_a_slot_to_the_users() {
        let limits = PreviewLimits {
            concurrency: 2,
            ..limits(20)
        };
        let previews = Previews::new(Executor::InProcess, &limits);
        let visitor = previews.slot(true).await.unwrap();
        assert!(previews.slot(true).await.is_none(), "one slot for visitors");
        let user = previews.slot(false).await.expect("the other is the users'");
        drop(visitor);
        // Users may take every slot; a visitor then waits like anyone.
        let second = previews.slot(false).await.unwrap();
        assert!(previews.slot(true).await.is_none());
        drop((user, second));
        assert!(previews.slot(true).await.is_some());
    }

    #[test]
    fn the_visitors_share_is_at_least_one_slot_and_never_all_of_several() {
        let share = |concurrency, public_concurrency| {
            PreviewLimits {
                concurrency,
                public_concurrency,
                ..PreviewLimits::default()
            }
            .public_slots()
        };
        assert_eq!(share(2, 1), 1);
        assert_eq!(share(4, 8), 3);
        assert_eq!(share(4, 0), 1);
        assert_eq!(share(1, 1), 1);
    }
}
