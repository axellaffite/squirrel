use axum::Json;
use axum::extract::State;
use squirrel_db::settings;

use crate::dto::InstanceSettingsDto;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiJson;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/admin/settings", tag = "admin",
    responses((status = 200, body = InstanceSettingsDto), (status = 403, body = ErrorBody)))]
pub async fn get_settings(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<InstanceSettingsDto>, ApiError> {
    current.require_admin()?;
    Ok(Json(settings::get(&state.pool).await?.into()))
}

#[utoipa::path(patch, path = "/api/admin/settings", tag = "admin",
    request_body = InstanceSettingsDto,
    responses((status = 200, body = InstanceSettingsDto), (status = 403, body = ErrorBody)))]
pub async fn update_settings(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(request): ApiJson<InstanceSettingsDto>,
) -> Result<Json<InstanceSettingsDto>, ApiError> {
    current.require_admin()?;
    Ok(Json(
        settings::set_invites_admin_only(&state.pool, request.invites_admin_only)
            .await?
            .into(),
    ))
}
