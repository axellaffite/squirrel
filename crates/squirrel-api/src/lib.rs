//! HTTP API for Squirrel.

mod albums_dto;
mod blocking;
mod client_ip;
mod content;
mod cursor;
mod devices_dto;
mod downloads;
mod dto;
mod error;
mod explore_dto;
mod extract;
mod files_dto;
mod filter_dto;
mod input;
pub mod maintenance;
pub mod ml;
mod people_dto;
mod photos_dto;
pub mod previews;
mod rate_limit;
mod routes;
mod search_dto;
mod security_headers;
mod session;
mod setup_token;
mod spa;
mod state;
mod sync_dto;
mod upload_runtime;

pub use client_ip::{TrustedProxies, TrustedProxiesError};
pub use downloads::DownloadLimits;
pub use error::{ApiError, ErrorBody};
pub use setup_token::SetupToken;
pub use state::{ApiConfig, AppState, FreeSpace, ModelsInstalled, RateLimit};
pub use upload_runtime::{
    ChunkGuard, CompletionGuard, Slot, Slots, UploadLimits, UploadLocks, UploadRuntime, UserRate,
};

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Method, StatusCode};
use tower_http::timeout::TimeoutLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::SecurityScheme;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Credential bodies (setup, login, join) are tiny; keep them so.
const CREDENTIAL_BODY_LIMIT: usize = 16 * 1024;
const API_BODY_LIMIT: usize = 64 * 1024;
/// A full sync check: 500 items of at most ~350 bytes each.
const SYNC_BODY_LIMIT: usize = 256 * 1024;
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
        (name = "storage"), (name = "jobs"), (name = "photos"), (name = "albums"),
        (name = "search"), (name = "devices"), (name = "sync"), (name = "explore"), (name = "ml"), (name = "people")
    ),
    // Schemas only referenced from query parameters are not collected from
    // the routes: list them here.
    components(schemas(filter_dto::PhotoKindFilter, explore_dto::PlaceLevel, people_dto::FaceAssignedBy)),
    modifiers(&SecuritySchemes)
)]
struct ApiDoc;

/// The credentials an operation may accept; each operation's `security` lists
/// the ones it takes (any one entry suffices):
/// - `session_cookie` (+ `csrf_token` on non-GET requests): a browser session;
/// - `device_bearer`: a phone's access token;
/// - `upload_token`: one upload's chunks and status (iOS background transfers).
struct SecuritySchemes;

impl utoipa::Modify for SecuritySchemes {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder};
        // utoipa fills the licence from Cargo.toml, where none is set yet: an
        // empty `license.name` fails client generators' spec validation.
        openapi.info.license = None;
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                session::SESSION_COOKIE,
                "Browser session, set by `POST /api/auth/login`.",
            ))),
        );
        components.add_security_scheme(
            "csrf_token",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-CSRF-Token",
                "The session's `csrf_token` (from `GET /api/auth/me`), required with the \
                 session cookie on every non-GET request.",
            ))),
        );
        // An apiKey in `Authorization`: OpenAPI's `http` type only knows
        // registered schemes, and this one is `Upload`.
        components.add_security_scheme(
            "upload_token",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "Authorization",
                "Per-upload token `squ_…` from `POST /api/uploads`, sent as \
                 `Authorization: Upload squ_…`, for this upload's chunks and status only.",
            ))),
        );
        components.add_security_scheme(
            "device_bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "Device access token, `sqa_…` (`POST /api/auth/device/login`).",
                    ))
                    .build(),
            ),
        );
    }
}

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
/// - [`sync_routes`]: the sync check (256 KiB body, 30 s timeout, per-device rate limit).
/// - [`credential_routes`]: endpoints that receive passwords or one-time secrets
///   (16 KiB body, 30 s timeout, per-route rate limits).
/// - [`transfer_routes`]: upload chunks, upload completion and file downloads. No
///   30 s timeout (hashing or streaming a large file takes longer); chunks get a
///   body limit of one chunk and their own timeout. Never add them to the groups
///   above, whose limits would cut transfers short.
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let timed = json_routes()
        .merge(credential_routes(config))
        .merge(search_routes(config))
        .merge(sync_routes())
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
        .routes(routes!(routes::devices::device_logout))
        .routes(routes!(
            routes::devices::get_current_device,
            routes::devices::rename_current_device
        ))
        .routes(routes!(routes::devices::list_devices))
        .routes(routes!(routes::devices::revoke_device))
        .routes(routes!(routes::sync::sync_folder))
        .routes(routes!(
            routes::invitations::create_invitation,
            routes::invitations::list_invitations
        ))
        .routes(routes!(routes::invitations::revoke_invitation))
        .routes(routes!(
            routes::admin::get_settings,
            routes::admin::update_settings
        ))
        .routes(routes!(routes::jobs::jobs_overview))
        .routes(routes!(routes::jobs::retry_job))
        .routes(routes!(routes::nodes::list_nodes))
        .routes(routes!(
            routes::nodes::get_node,
            routes::nodes::update_node,
            routes::nodes::trash_node
        ))
        .routes(routes!(routes::nodes::create_folder))
        .routes(routes!(routes::thumbnails::thumbnail))
        .routes(routes!(
            routes::trash::list_trash,
            routes::trash::empty_trash
        ))
        .routes(routes!(routes::trash::restore))
        .routes(routes!(routes::trash::delete_forever))
        .routes(routes!(routes::photos::list_photos))
        .routes(routes!(routes::photos::photo_months))
        .routes(routes!(routes::photos::get_photo))
        .routes(routes!(routes::photos::media_status))
        .routes(routes!(routes::explore::list_tags))
        .routes(routes!(routes::explore::list_places))
        .routes(routes!(routes::explore::list_cameras))
        .routes(routes!(routes::people::list_people))
        .routes(routes!(
            routes::people::get_person,
            routes::people::update_person
        ))
        .routes(routes!(routes::people::merge_people))
        .routes(routes!(routes::people::split_person))
        .routes(routes!(routes::people::person_faces))
        .routes(routes!(routes::faces::photo_faces))
        .routes(routes!(
            routes::faces::assign_face,
            routes::faces::unassign_face
        ))
        .routes(routes!(
            routes::ml_settings::get_ml_settings,
            routes::ml_settings::update_ml_settings
        ))
        .routes(routes!(
            routes::albums::list_albums,
            routes::albums::create_album
        ))
        .routes(routes!(
            routes::albums::get_album,
            routes::albums::update_album,
            routes::albums::delete_album
        ))
        .routes(routes!(
            routes::albums::list_album_items,
            routes::albums::add_album_items
        ))
        .routes(routes!(routes::albums::remove_album_items))
        .routes(routes!(routes::storage::storage_usage))
        .routes(routes!(routes::uploads::create_upload))
        .routes(routes!(
            routes::uploads::upload_status,
            routes::uploads::abort_upload
        ))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT))
}

/// Document and photo search: ordinary JSON endpoints, rate limited per
/// client (every search scans text indexes or embeddings).
fn search_routes(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let routes = OpenApiRouter::new()
        .routes(routes!(routes::search::search_documents))
        .routes(routes!(routes::photo_search::search_photos))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
    let searches = rate_limit::apply(
        routes,
        config.search_rate_limit,
        Method::GET,
        &config.trusted_proxies,
    );
    // Warming up starts a model process: limited like searches.
    let warm_up = OpenApiRouter::new()
        .routes(routes!(routes::photo_search::warm_up_search))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
    searches.merge(rate_limit::apply(
        warm_up,
        config.search_rate_limit,
        Method::POST,
        &config.trusted_proxies,
    ))
}

/// The phone's sync check: larger bodies (a 500-item batch), rate limited per
/// device by the handler (the limit needs the authenticated device).
fn sync_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(routes::sync::sync_check))
        .layer(DefaultBodyLimit::max(SYNC_BODY_LIMIT))
}

/// Endpoints that take credentials or one-time secrets: tiny bodies and rate
/// limits (login/setup/join and device login POSTs share one bucket, invitation
/// checks and device token refreshes have their own).
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
    let refresh_routes = OpenApiRouter::new().routes(routes!(routes::device_auth::device_refresh));
    let refresh_routes = rate_limit::apply(
        refresh_routes,
        config.device_refresh_rate_limit,
        Method::POST,
        trusted,
    );
    let routes = OpenApiRouter::new()
        .routes(routes!(routes::setup::create_first_admin))
        .routes(routes!(routes::auth::login))
        .routes(routes!(routes::device_auth::device_login))
        .merge(join_routes);
    rate_limit::apply(routes, config.login_rate_limit, Method::POST, trusted)
        .merge(refresh_routes)
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
