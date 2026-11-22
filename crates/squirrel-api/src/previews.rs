//! Rendering lightbox previews on request: the sandboxed renderer, its slots
//! (cache hits take none), one render per cache key, and a per-user limit on
//! renders.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use squirrel_media::thumbs::DecodeLimits;
use squirrel_sandbox::exec::Executor;
use squirrel_sandbox::preview::PreviewRenderer;
use squirrel_storage::CacheKey;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::upload_runtime::{UserRate, UserRateLimiter};

/// Preview limits (part of [`crate::ApiConfig`]).
#[derive(Debug, Clone)]
pub struct PreviewLimits {
    /// Renders at once, all users together (`SQUIRREL_PREVIEW_CONCURRENCY`).
    pub concurrency: usize,
    /// How long a render waits for a slot before answering 503 `busy`.
    pub slot_wait: Duration,
    /// Per-user limit on renders (cache misses). `None` disables it.
    pub render_rate: Option<UserRate>,
}

impl Default for PreviewLimits {
    fn default() -> Self {
        Self {
            concurrency: 2,
            slot_wait: Duration::from_secs(3),
            render_rate: Some(UserRate {
                per_second: 4.0,
                burst: 40,
            }),
        }
    }
}

/// Keys with a render in flight: at most this many before stale ones are pruned.
const PRUNE_KEYS_ABOVE: usize = 256;

#[derive(Debug)]
pub struct Previews {
    pub renderer: PreviewRenderer,
    slots: Arc<Semaphore>,
    slot_wait: Duration,
    rate: Option<UserRateLimiter>,
    in_flight: Mutex<HashMap<CacheKey, Weak<tokio::sync::Mutex<()>>>>,
}

impl Previews {
    pub fn new(executor: Executor, limits: &PreviewLimits) -> Self {
        Self {
            renderer: PreviewRenderer::new(executor, DecodeLimits::default()),
            slots: Arc::new(Semaphore::new(limits.concurrency.max(1))),
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
    /// renders, the others then find it cached.
    pub fn key_lock(&self, key: &CacheKey) -> Arc<tokio::sync::Mutex<()>> {
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
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        in_flight.insert(key.clone(), Arc::downgrade(&lock));
        lock
    }

    /// A render slot, or `None` when none frees up within the configured wait.
    pub async fn slot(&self) -> Option<OwnedSemaphorePermit> {
        tokio::time::timeout(self.slot_wait, self.slots.clone().acquire_owned())
            .await
            .ok()?
            .ok()
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

    #[tokio::test]
    async fn slots_run_out_after_the_wait() {
        let limits = PreviewLimits {
            concurrency: 1,
            slot_wait: Duration::from_millis(20),
            render_rate: None,
        };
        let previews = Previews::new(Executor::InProcess, &limits);
        let held = previews.slot().await.unwrap();
        assert!(previews.slot().await.is_none());
        drop(held);
        assert!(previews.slot().await.is_some());
        assert!(previews.allow_render(Uuid::now_v7()));
    }
}
