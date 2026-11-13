//! Per-user token bucket (session-keyed limits cannot use the IP-keyed governor
//! layer, which runs before authentication).

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use uuid::Uuid;

/// Buckets kept before idle (full) ones are dropped.
const MAX_TRACKED: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UserRate {
    /// Tokens added per second. Must be > 0.
    pub per_second: f64,
    /// Bucket size: requests allowed in a burst. Must be > 0.
    pub burst: u32,
}

#[derive(Debug)]
pub struct UserRateLimiter {
    rate: UserRate,
    buckets: Mutex<HashMap<Uuid, Bucket>>,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    at: Instant,
}

impl UserRateLimiter {
    pub fn new(rate: UserRate) -> Self {
        Self {
            rate,
            buckets: Mutex::default(),
        }
    }

    /// Takes one token for `user`; `false` when their bucket is empty.
    pub fn check(&self, user: Uuid) -> bool {
        self.check_at(user, Instant::now())
    }

    fn check_at(&self, user: Uuid, now: Instant) -> bool {
        let burst = f64::from(self.rate.burst);
        let mut buckets = self.buckets.lock().unwrap_or_else(PoisonError::into_inner);
        if buckets.len() >= MAX_TRACKED {
            buckets.retain(|_, b| self.refilled(*b, now) < burst);
        }
        let bucket = buckets.entry(user).or_insert(Bucket {
            tokens: burst,
            at: now,
        });
        let tokens = self.refilled(*bucket, now);
        let allowed = tokens >= 1.0;
        *bucket = Bucket {
            tokens: if allowed { tokens - 1.0 } else { tokens },
            at: now,
        };
        allowed
    }

    fn refilled(&self, bucket: Bucket, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(bucket.at).as_secs_f64();
        (bucket.tokens + elapsed * self.rate.per_second).min(f64::from(self.rate.burst))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn bursts_then_refills_per_user() {
        let limiter = UserRateLimiter::new(UserRate {
            per_second: 2.0,
            burst: 3,
        });
        let (alice, bob) = (Uuid::now_v7(), Uuid::now_v7());
        let start = Instant::now();
        for _ in 0..3 {
            assert!(limiter.check_at(alice, start));
        }
        assert!(!limiter.check_at(alice, start));
        assert!(limiter.check_at(bob, start));
        assert!(limiter.check_at(alice, start + Duration::from_millis(500)));
        assert!(!limiter.check_at(alice, start + Duration::from_millis(500)));
    }
}
