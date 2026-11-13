//! HTTP API for Squirrel.

mod blocking;
mod client_ip;
#[allow(
    dead_code,
    unused_imports,
    reason = "used by the content route (Task 19)"
)]
mod content;
mod cursor;
mod dto;
mod error;
mod extract;
mod files_dto;
mod input;
pub mod maintenance;
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
/// Room for framing beyond one chunk in a chunk upload body.
const CHUNK_BODY_MARGIN: usize = 64 * 1024;
/// One chunk (8 MiB by default) must arrive within this time (~28 KB/s at
/// worst): long enough for slow links, short enough to bound slow-loris bodies.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(OpenApi)]
#[openapi(
    info(title = "Squirrel API", version = "0.1.0"),
    tags(
        (name = "system"), (name = "setup"), (name = "auth"), (name = "invitations"),
        (name = "admin"), (name = "files"), (name = "uploads"), (name = "trash"),
        (name = "storage")
    )
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
/// - [`transfer_routes`]: upload chunks, upload completion and file downloads. No
///   30 s timeout (hashing or streaming a large file takes longer); chunks get a
///   body limit of one chunk and their own timeout. Never add them to the groups
///   above, whose limits would cut transfers short.
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let timed =
        json_routes()
            .merge(credential_routes(config))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                API_TIMEOUT,
            ));
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(timed)
        .merge(transfer_routes(config))
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
        .routes(routes!(routes::nodes::list_nodes))
        .routes(routes!(
            routes::nodes::get_node,
            routes::nodes::update_node,
            routes::nodes::trash_node
        ))
        .routes(routes!(routes::nodes::create_folder))
        .routes(routes!(
            routes::trash::list_trash,
            routes::trash::empty_trash
        ))
        .routes(routes!(routes::trash::restore))
        .routes(routes!(routes::trash::delete_forever))
        .routes(routes!(routes::storage::storage_usage))
        .routes(routes!(routes::uploads::create_upload))
        .routes(routes!(
            routes::uploads::upload_status,
            routes::uploads::abort_upload
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

/// Uploads and downloads. Still session- and CSRF-protected by their extractors.
fn transfer_routes(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let chunk_limit = usize::try_from(config.upload_chunk_size)
        .unwrap_or(usize::MAX)
        .saturating_add(CHUNK_BODY_MARGIN);
    let chunks = OpenApiRouter::new()
        .routes(routes!(routes::uploads::chunks::put_chunk))
        .layer(DefaultBodyLimit::max(chunk_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            CHUNK_TIMEOUT,
        ));
    let long_running = OpenApiRouter::new()
        .routes(routes!(routes::uploads::complete::complete_upload))
        .routes(routes!(routes::content::file_content))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
    chunks.merge(long_running)
}
