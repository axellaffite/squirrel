//! HTTP API for Squirrel.

mod blocking;
mod client_ip;
mod dto;
mod error;
mod extract;
mod input;
mod rate_limit;
mod routes;
mod session;
mod setup_token;
mod spa;
mod state;

pub use client_ip::{TrustedProxies, TrustedProxiesError};
pub use error::{ApiError, ErrorBody};
pub use setup_token::SetupToken;
pub use state::{ApiConfig, AppState, RateLimit};

use axum::Router;
use axum::http::Method;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(
    info(title = "Squirrel API", version = "0.1.0"),
    tags((name = "system"), (name = "setup"), (name = "auth"), (name = "invitations"), (name = "admin"))
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, _) = api_router(&state.config).split_for_parts();
    router
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(spa::serve)
        .with_state(state)
}

async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed
}

/// The OpenAPI document, used to generate the committed `openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api_router(&ApiConfig::default()).split_for_parts().1
}

fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
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
    let credential_routes = OpenApiRouter::new()
        .routes(routes!(routes::setup::create_first_admin))
        .routes(routes!(routes::auth::login))
        .merge(join_routes);
    let credential_routes = rate_limit::apply(
        credential_routes,
        config.login_rate_limit,
        Method::POST,
        trusted,
    );

    OpenApiRouter::with_openapi(ApiDoc::openapi())
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
        .merge(credential_routes)
}
