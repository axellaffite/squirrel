use std::sync::Arc;
use std::time::Duration as StdDuration;

use sqlx::PgPool;
use squirrel_storage::BlobStore;
use time::Duration;
use tokio::sync::Semaphore;

use crate::client_ip::TrustedProxies;
use crate::downloads::DownloadLimits;
use crate::setup_token::SetupToken;
use crate::upload_runtime::{Slots, UploadLimits, UploadRuntime};

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
    /// Uploads are refused when they would leave less than this free on the
    /// data disk, counting what open sessions may still write; chunk writes are
    /// refused below it.
    pub min_free_bytes: u64,
    /// Concurrency, session and rate limits for uploads.
    pub upload_limits: UploadLimits,
    /// Concurrent file content responses (503 `busy` beyond them).
    pub download_limits: DownloadLimits,
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
            upload_limits: UploadLimits::default(),
            download_limits: DownloadLimits::default(),
            trash_retention: Duration::days(30),
            blob_gc_grace: Duration::hours(24),
        }
    }
}

/// Free bytes on the data disk (the blob store's filesystem by default;
/// tests pin it). Blocking: called on the blocking pool.
pub type FreeSpace = Arc<dyn Fn() -> squirrel_storage::Result<u64> + Send + Sync>;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Blobs and in-progress uploads on disk.
    pub storage: BlobStore,
    pub config: Arc<ApiConfig>,
    /// Bounds concurrent password hashing (see [`ApiConfig::hash_concurrency`]).
    pub hash_permits: Arc<Semaphore>,
    /// Per-upload locks and upload concurrency limits.
    pub uploads: Arc<UploadRuntime>,
    /// Streaming file bodies; a slot lives as long as its body.
    pub downloads: Slots,
    pub free_space: FreeSpace,
}

impl AppState {
    pub fn new(pool: PgPool, storage: BlobStore, config: ApiConfig) -> Self {
        let store = storage.clone();
        Self {
            pool,
            storage,
            free_space: Arc::new(move || store.available_space()),
            hash_permits: Arc::new(Semaphore::new(config.hash_concurrency)),
            uploads: Arc::new(UploadRuntime::new(&config.upload_limits)),
            downloads: config.download_limits.slots(),
            config: Arc::new(config),
        }
    }

    /// This state, measuring free disk space with `free_space` instead.
    pub fn with_free_space(
        self,
        free_space: impl Fn() -> squirrel_storage::Result<u64> + Send + Sync + 'static,
    ) -> Self {
        Self {
            free_space: Arc::new(free_space),
            ..self
        }
    }

    /// Free bytes on the data disk now.
    pub async fn available_space(&self) -> Result<u64, crate::ApiError> {
        let free_space = self.free_space.clone();
        crate::blocking::run_storage(move || free_space()).await
    }
}
