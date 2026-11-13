use std::sync::Arc;

use sqlx::PgPool;
use time::Duration;

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
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<ApiConfig>,
}

impl AppState {
    pub fn new(pool: PgPool, config: ApiConfig) -> Self {
        Self {
            pool,
            config: Arc::new(config),
        }
    }
}
