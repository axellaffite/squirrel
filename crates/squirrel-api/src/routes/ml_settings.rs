//! The caller's machine learning settings: face recognition on or off.
//! Browser session only: turning faces off deletes data, which a stolen
//! phone token must not be able to do.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use squirrel_db::{faces, ml_settings};
use utoipa::ToSchema;

use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiJson;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct MlSettingsDto {
    /// Faces are detected and grouped into people (on by default).
    pub faces_enabled: bool,
    /// Faces stored for the caller.
    pub faces_count: i64,
    /// People (groups of faces) of the caller.
    pub people_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMlSettings {
    /// `false` deletes every face and person of the caller at once.
    pub faces_enabled: bool,
}

async fn current(state: &AppState, user: uuid::Uuid) -> Result<MlSettingsDto, ApiError> {
    let enabled = ml_settings::faces_enabled(&state.pool, user).await?;
    let counts = faces::counts(&state.pool, user).await?;
    Ok(MlSettingsDto {
        faces_enabled: enabled,
        faces_count: counts.faces,
        people_count: counts.people,
    })
}

#[utoipa::path(get, path = "/api/me/ml-settings", security(("session_cookie" = [])), tag = "ml",
    responses((status = 200, body = MlSettingsDto), (status = 401, body = ErrorBody)))]
pub async fn get_ml_settings(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> Result<Json<MlSettingsDto>, ApiError> {
    Ok(Json(current(&state, current_user.user.id).await?))
}

#[utoipa::path(put, path = "/api/me/ml-settings", security(("session_cookie" = [], "csrf_token" = [])), tag = "ml",
    request_body = UpdateMlSettings,
    responses((status = 200, body = MlSettingsDto), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn update_ml_settings(
    State(state): State<AppState>,
    current_user: CurrentUser,
    ApiJson(request): ApiJson<UpdateMlSettings>,
) -> Result<Json<MlSettingsDto>, ApiError> {
    let user = current_user.user.id;
    if request.faces_enabled {
        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(squirrel_db::DbError::from)?;
        ml_settings::set_faces_enabled(&mut *tx, user, true).await?;
        squirrel_jobs::enqueue(&mut tx, &squirrel_jobs::kinds::face_backfill(user, None))
            .await
            .map_err(squirrel_db::DbError::from)?;
        tx.commit().await.map_err(squirrel_db::DbError::from)?;
    } else {
        let deleted = faces::turn_off(&state.pool, user).await?;
        tracing::info!(user = %user, faces = deleted.faces, people = deleted.people, "faces turned off: data deleted");
    }
    Ok(Json(current(&state, user).await?))
}
