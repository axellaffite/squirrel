//! Concurrency limits: a global semaphore plus a per-user cap.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

type Counts = Arc<Mutex<HashMap<Uuid, usize>>>;

/// At most `global` holders overall and `per_user` per user.
#[derive(Debug, Clone)]
pub struct Slots {
    global: Arc<Semaphore>,
    per_user: usize,
    users: Counts,
    wait: Duration,
}

/// One held slot, released on drop (also when the work it guards is moved
/// into a spawned task that outlives the request).
#[must_use]
pub struct Slot {
    _permit: OwnedSemaphorePermit,
    _user: UserSlot,
}

impl Slots {
    pub fn new(global: usize, per_user: usize, wait: Duration) -> Self {
        Self {
            global: Arc::new(Semaphore::new(global)),
            per_user,
            users: Arc::default(),
            wait,
        }
    }

    /// `None` when the user already holds `per_user` slots (immediately) or no
    /// global slot frees up within the configured wait.
    pub async fn acquire(&self, user: Uuid) -> Option<Slot> {
        let user_slot = self.take_user(user)?;
        let permit = tokio::time::timeout(self.wait, self.global.clone().acquire_owned())
            .await
            .ok()?
            .ok()?;
        Some(Slot {
            _permit: permit,
            _user: user_slot,
        })
    }

    fn take_user(&self, user: Uuid) -> Option<UserSlot> {
        let mut users = self.users.lock().unwrap_or_else(PoisonError::into_inner);
        let count = users.entry(user).or_default();
        if *count >= self.per_user {
            return None;
        }
        *count += 1;
        Some(UserSlot {
            user,
            users: self.users.clone(),
        })
    }
}

struct UserSlot {
    user: Uuid,
    users: Counts,
}

impl Drop for UserSlot {
    fn drop(&mut self) {
        let mut users = self.users.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = users.get_mut(&self.user) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                users.remove(&self.user);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: Duration = Duration::from_millis(20);

    #[tokio::test]
    async fn users_are_capped_individually() {
        let slots = Slots::new(10, 2, WAIT);
        let (alice, bob) = (Uuid::now_v7(), Uuid::now_v7());
        let first = slots.acquire(alice).await.unwrap();
        let _second = slots.acquire(alice).await.unwrap();
        assert!(slots.acquire(alice).await.is_none());
        let _bob = slots.acquire(bob).await.unwrap();
        drop(first);
        assert!(slots.acquire(alice).await.is_some());
    }

    #[tokio::test]
    async fn the_global_cap_waits_then_gives_up() {
        let slots = Slots::new(1, 2, WAIT);
        let held = slots.acquire(Uuid::now_v7()).await.unwrap();
        let other = Uuid::now_v7();
        assert!(slots.acquire(other).await.is_none());
        // The refused attempt released its per-user count.
        assert_eq!(slots.users.lock().unwrap().len(), 1);
        drop(held);
        assert!(slots.acquire(other).await.is_some());
        assert!(slots.users.lock().unwrap().is_empty());
    }
}
