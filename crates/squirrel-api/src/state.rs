use std::sync::Arc;
use std::time::Duration as StdDuration;

use sqlx::PgPool;
use squirrel_storage::BlobStore;
use time::Duration;
use tokio::sync::Semaphore;

use crate::client_ip::TrustedProxies;
use crate::setup_token::SetupToken;

#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    /// One request token is replenished every `replenish_every_secs` seconds. Must be > 0.
    pub replenish_every_secs: u64,
    /// Requests allowed in a burst. Must be > 0.
    pub burst: u32,
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub cookie_secure: bool,
    /// Idle lifetime: a session unused for this long expires.
    pub session_ttl: Duration,
    /// Hard lifetime from login, whatever the activity. Also the cookie Max-Age.
    pub session_absolute_ttl: Duration,
    pub session_touch_interval: Duration,
    pub invitation_ttl: Duration,
    /// Per-client limit on credential POSTs (login, setup, join). `None` disables it.
    pub login_rate_limit: Option<RateLimit>,
    /// Separate, more generous per-client limit on `GET /api/join/{token}`.
    pub join_check_rate_limit: Option<RateLimit>,
    /// Proxies whose `X-Forwarded-For` is believed when resolving client addresses.
    pub trusted_proxies: TrustedProxies,
    /// Required to create the first administrator. `None` refuses setup entirely.
    pub setup_token: Option<SetupToken>,
    /// Concurrent Argon2 hash/verify operations (each uses ~19 MiB and a core).
    pub hash_concurrency: usize,
    /// How long a request waits for a hashing slot before answering 503 `busy`.
    pub hash_wait: StdDuration,
    /// Chunk size announced to clients. Every chunk but the last has exactly this size.
    pub upload_chunk_size: u64,
    /// Upload sessions expire after this long without a chunk.
    pub upload_ttl: Duration,
    /// Uploads are refused when they would leave less than this free on the data disk.
    pub min_free_bytes: u64,
    /// Trash entries older than this are deleted for good.
    pub trash_retention: Duration,
    /// Blobs are deleted only after this long without any reference.
    pub blob_gc_grace: Duration,
}

/// Available cores clamped to 2..=4: enough for a few simultaneous logins,
/// bounded so a burst cannot exhaust memory or starve other requests.
fn default_hash_concurrency() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(2, 4)
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            cookie_secure: true,
            session_ttl: Duration::days(30),
            session_absolute_ttl: Duration::days(90),
            session_touch_interval: Duration::hours(1),
            invitation_ttl: Duration::days(7),
            login_rate_limit: Some(RateLimit {
                replenish_every_secs: 2,
                burst: 10,
            }),
            join_check_rate_limit: Some(RateLimit {
                replenish_every_secs: 1,
                burst: 30,
            }),
            trusted_proxies: TrustedProxies::default(),
            setup_token: None,
            hash_concurrency: default_hash_concurrency(),
            hash_wait: StdDuration::from_secs(5),
            upload_chunk_size: 8 * 1024 * 1024,
            upload_ttl: Duration::hours(24),
            min_free_bytes: 1024 * 1024 * 1024,
            trash_retention: Duration::days(30),
            blob_gc_grace: Duration::hours(24),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Blobs and in-progress uploads on disk.
    pub storage: BlobStore,
    pub config: Arc<ApiConfig>,
    /// Bounds concurrent password hashing (see [`ApiConfig::hash_concurrency`]).
    pub hash_permits: Arc<Semaphore>,
}

impl AppState {
    pub fn new(pool: PgPool, storage: BlobStore, config: ApiConfig) -> Self {
        Self {
            pool,
            storage,
            hash_permits: Arc::new(Semaphore::new(config.hash_concurrency)),
            config: Arc::new(config),
        }
    }
}
