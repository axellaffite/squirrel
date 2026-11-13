//! In-process upload coordination: per-upload locks, concurrency slots for
//! chunk writes and completions, the per-user creation rate and the disk
//! admission lock.

mod locks;
mod rate;
mod slots;

use std::time::Duration;

pub use locks::{ChunkGuard, CompletionGuard, UploadLocks};
pub use rate::{UserRate, UserRateLimiter};
pub use slots::{Slot, Slots};

/// Upload limits (part of [`crate::ApiConfig`]).
#[derive(Debug, Clone)]
pub struct UploadLimits {
    /// Completions (hash + store) running at once, all users together.
    pub completion_concurrency: usize,
    /// Completions one user may run at once.
    pub completion_per_user: usize,
    /// Chunk writes in flight at once, all users together.
    pub chunk_concurrency: usize,
    /// Chunk writes one user may have in flight.
    pub chunk_per_user: usize,
    /// How long a request waits for a global slot before answering 503 `busy`.
    pub slot_wait: Duration,
    /// Unexpired upload sessions one user may have (429 `too_many_uploads`).
    pub max_open_per_user: i64,
    /// Per-user limit on `POST /api/uploads`. `None` disables it.
    pub create_rate: Option<UserRate>,
}

impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            completion_concurrency: 4,
            completion_per_user: 2,
            chunk_concurrency: 32,
            chunk_per_user: 8,
            slot_wait: Duration::from_secs(5),
            max_open_per_user: 100,
            // Generous: a folder of small files opens one upload per file.
            create_rate: Some(UserRate {
                per_second: 20.0,
                burst: 200,
            }),
        }
    }
}

/// Shared state built from [`UploadLimits`].
#[derive(Debug)]
pub struct UploadRuntime {
    pub locks: UploadLocks,
    pub completions: Slots,
    pub chunks: Slots,
    pub create_rate: Option<UserRateLimiter>,
    /// Serializes "check free disk space, then open a session" so parallel
    /// creations cannot all reserve the same free space.
    pub admission: tokio::sync::Mutex<()>,
}

impl UploadRuntime {
    pub fn new(limits: &UploadLimits) -> Self {
        Self {
            locks: UploadLocks::default(),
            completions: Slots::new(
                limits.completion_concurrency,
                limits.completion_per_user,
                limits.slot_wait,
            ),
            chunks: Slots::new(
                limits.chunk_concurrency,
                limits.chunk_per_user,
                limits.slot_wait,
            ),
            create_rate: limits.create_rate.map(UserRateLimiter::new),
            admission: tokio::sync::Mutex::new(()),
        }
    }
}
