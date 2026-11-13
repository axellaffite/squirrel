use std::sync::Arc;

use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use utoipa_axum::router::OpenApiRouter;

use crate::state::{AppState, RateLimit};

/// Per-peer-IP limit. Behind a reverse proxy every request shares the proxy's IP,
/// making this a global limit — acceptable for a home server; per-account lockout
/// still protects individual accounts.
pub fn apply(router: OpenApiRouter<AppState>, limit: RateLimit) -> OpenApiRouter<AppState> {
    let config = GovernorConfigBuilder::default()
        .per_second(limit.replenish_every_secs)
        .burst_size(limit.burst)
        .finish()
        .expect("rate limit values are non-zero");
    router.layer(GovernorLayer {
        config: Arc::new(config),
    })
}
