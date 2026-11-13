//! HTTP API for Squirrel.

mod blocking;
mod client_ip;
mod dto;
mod error;
mod extract;
mod input;
mod rate_limit;
mod routes;
mod security_headers;
mod session;
mod setup_token;
mod spa;
mod state;

pub use client_ip::{TrustedProxies, TrustedProxiesError};
pub use error::{ApiError, ErrorBody};
pub use setup_token::SetupToken;
pub use state::{ApiConfig, AppState, RateLimit};

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Method, StatusCode};
use tower_http::timeout::TimeoutLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Credential bodies (setup, login, join) are tiny; keep them so.
const CREDENTIAL_BODY_LIMIT: usize = 16 * 1024;
const API_BODY_LIMIT: usize = 64 * 1024;
const API_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(OpenApi)]
#[openapi(
    info(title = "Squirrel API", version = "0.1.0"),
    tags((name = "system"), (name = "setup"), (name = "auth"), (name = "invitations"), (name = "admin"))
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, _) = api_router(&state.config).split_for_parts();
    let hsts = state.config.cookie_secure;
    let router = router
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(spa::serve)
        .with_state(state);
    security_headers::apply(router, hsts)
}

async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed
}

/// The OpenAPI document, used to generate the committed `openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api_router(&ApiConfig::default()).split_for_parts().1
}

/// Builds every `/api` route. Routes are grouped by the limits they need; put a
/// new route in the group whose limits fit it:
///
/// - [`json_routes`]: ordinary JSON endpoints (64 KiB body, 30 s timeout). The default.
/// - [`credential_routes`]: endpoints that receive passwords or one-time secrets
///   (16 KiB body, 30 s timeout, per-route rate limits).
/// - `transfer_routes` (none yet): uploads, downloads and streaming. They will get
///   their own large body limit and no request timeout, so they must not be added
///   to the groups above, whose limits would cut transfers short. Merge them here
///   without the timeout layer.
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(json_routes())
        .merge(credential_routes(config))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            API_TIMEOUT,
        ))
}

/// Ordinary JSON endpoints: small bodies. The request timeout is applied by [`api_router`].
fn json_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::setup::setup_status))
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::logout_all))
        .routes(routes!(
            routes::invitations::create_invitation,
            routes::invitations::list_invitations
        ))
        .routes(routes!(routes::invitations::revoke_invitation))
        .routes(routes!(
            routes::admin::get_settings,
            routes::admin::update_settings
        ))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT))
}

/// Endpoints that take credentials or one-time secrets: tiny bodies and rate
/// limits (login/setup/join POSTs share one bucket, invitation checks another).
fn credential_routes(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let trusted = &config.trusted_proxies;
    let join_routes = OpenApiRouter::new().routes(routes!(
        routes::join::check_invitation,
        routes::join::accept_invitation
    ));
    let join_routes = rate_limit::apply(
        join_routes,
        config.join_check_rate_limit,
        Method::GET,
        trusted,
    );
    let routes = OpenApiRouter::new()
        .routes(routes!(routes::setup::create_first_admin))
        .routes(routes!(routes::auth::login))
        .merge(join_routes);
    rate_limit::apply(routes, config.login_rate_limit, Method::POST, trusted)
        .layer(DefaultBodyLimit::max(CREDENTIAL_BODY_LIMIT))
}
