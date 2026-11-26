//! The caller's blur filter: a rule, written as a dynamic album's, whose
//! matching photos (the caller's own) every client shows blurred until the
//! caller reveals them. Clients read which photos with `GET /api/photos/blurred`.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use squirrel_db::blur_filter::{self, BlurFilter};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::albums_dto::AlbumRulesDto;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::input::ApiJson;
use crate::routes::albums::checked_rules;
use crate::state::AppState;

/// The caller's blur filter; without one, `rules` is null and nothing is blurred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BlurFilterDto {
    /// Whether clients blur the photos matching `rules`. Off keeps the rules.
    pub enabled: bool,
    /// Which photos, as a dynamic album's rules (at least one condition).
    /// `null` on update forgets the filter.
    #[serde(default)]
    pub rules: Option<AlbumRulesDto>,
}

/// The caller's photos their blur filter matches.
#[derive(Debug, Serialize, ToSchema)]
pub struct BlurredPhotosDto {
    /// Node ids of the photos to blur, newest first; empty without an enabled
    /// filter.
    pub node_ids: Vec<Uuid>,
}

async fn current(state: &AppState, user: Uuid) -> Result<BlurFilterDto, ApiError> {
    Ok(match blur_filter::find(&state.pool, user).await? {
        Some(filter) => BlurFilterDto {
            enabled: filter.enabled,
            rules: Some(AlbumRulesDto::from(&filter.rules)),
        },
        None => BlurFilterDto {
            enabled: false,
            rules: None,
        },
    })
}

#[utoipa::path(get, path = "/api/me/blur-filter", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos",
    responses((status = 200, body = BlurFilterDto), (status = 401, body = ErrorBody)))]
pub async fn get_blur_filter(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<BlurFilterDto>, ApiError> {
    Ok(Json(current(&state, caller.user.id).await?))
}

#[utoipa::path(put, path = "/api/me/blur-filter", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "photos",
    request_body = BlurFilterDto,
    responses((status = 200, body = BlurFilterDto), (status = 401, body = ErrorBody), (status = 422, body = ErrorBody),
              (status = 503, body = ErrorBody, description = "Rules with a search text: `ml_unavailable` (no text model on this server) or `busy` (the text model or the label bank is not ready yet: try again)")))]
pub async fn update_blur_filter(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<BlurFilterDto>,
) -> Result<Json<BlurFilterDto>, ApiError> {
    let user = caller.user.id;
    match &request.rules {
        Some(dto) => {
            let rules = checked_rules(&state, user, dto).await?;
            let filter = BlurFilter {
                enabled: request.enabled,
                rules,
            };
            blur_filter::save(&state.pool, user, &filter).await?;
        }
        None => blur_filter::delete(&state.pool, user).await?,
    }
    Ok(Json(current(&state, user).await?))
}

#[utoipa::path(get, path = "/api/photos/blurred", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos",
    responses((status = 200, body = BlurredPhotosDto), (status = 401, body = ErrorBody),
              (status = 503, body = ErrorBody, description = "`busy`: the library is too large to match in time")))]
pub async fn blurred_photos(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<BlurredPhotosDto>, ApiError> {
    let node_ids = blur_filter::blurred_ids(&state.pool, caller.user.id).await?;
    Ok(Json(BlurredPhotosDto { node_ids }))
}
