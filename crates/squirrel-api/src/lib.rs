//! HTTP API for Squirrel.

mod blocking;
mod dto;
mod error;
mod routes;
mod session;
mod spa;
mod state;

pub use error::{ApiError, ErrorBody};
pub use state::{ApiConfig, AppState, RateLimit};

use axum::Router;
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
    router.fallback(spa::serve).with_state(state)
}

/// The OpenAPI document, used to generate the committed `openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api_router(&ApiConfig::default()).split_for_parts().1
}

fn api_router(_config: &ApiConfig) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::setup::setup_status))
        .routes(routes!(routes::setup::create_first_admin))
}
