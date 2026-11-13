use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, Response};
use axum::response::IntoResponse;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};
use utoipa_axum::router::OpenApiRouter;

use crate::client_ip::{TrustedProxies, rate_limit_key, resolve};
use crate::error::ApiError;
use crate::state::{AppState, RateLimit};

/// How often idle buckets are dropped from the limiter's memory.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Keys requests by client address (see [`resolve`]), IPv6 masked to /64.
#[derive(Debug, Clone)]
pub struct ClientIpKeyExtractor {
    trusted: TrustedProxies,
}

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        let ConnectInfo(peer) = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .ok_or(GovernorError::UnableToExtractKey)?;
        Ok(rate_limit_key(resolve(
            peer.ip(),
            req.headers(),
            &self.trusted,
        )))
    }
}

/// Limits requests with the given `method` on every route of `router`.
/// `None` leaves the router unlimited.
pub fn apply(
    router: OpenApiRouter<AppState>,
    limit: Option<RateLimit>,
    method: Method,
    trusted: &TrustedProxies,
) -> OpenApiRouter<AppState> {
    let Some(limit) = limit else {
        return router;
    };
    let config = GovernorConfigBuilder::default()
        .per_second(limit.replenish_every_secs)
        .burst_size(limit.burst)
        .methods(vec![method])
        .key_extractor(ClientIpKeyExtractor {
            trusted: trusted.clone(),
        })
        .error_handler(error_response)
        .finish()
        .expect("rate limit values are non-zero");
    let config = Arc::new(config);
    // Periodically forget idle buckets (per tower_governor's docs). A plain thread,
    // so building a router needs no runtime; it stops once the limiter is dropped.
    let limiter = Arc::downgrade(config.limiter());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(CLEANUP_INTERVAL);
            let Some(limiter) = limiter.upgrade() else {
                break;
            };
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    });
    router.layer(GovernorLayer { config })
}

fn error_response(error: GovernorError) -> Response<Body> {
    match error {
        GovernorError::TooManyRequests { headers, .. } => {
            let mut response = ApiError::RateLimited.into_response();
            if let Some(headers) = headers {
                response.headers_mut().extend(headers);
            }
            response
        }
        GovernorError::UnableToExtractKey => ApiError::Internal(anyhow::anyhow!(
            "rate limiter could not determine the client address"
        ))
        .into_response(),
        GovernorError::Other { msg, .. } => {
            ApiError::Internal(anyhow::anyhow!("rate limiter error: {msg:?}")).into_response()
        }
    }
}
