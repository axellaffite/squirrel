use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use squirrel_db::sessions;

use crate::dto::MeResponse;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/auth/me", tag = "auth",
    responses((status = 200, body = MeResponse), (status = 401, body = ErrorBody)))]
pub async fn me(current: CurrentUser) -> Json<MeResponse> {
    Json(MeResponse::new(
        &current.user,
        current.session.csrf_token.clone(),
    ))
}

#[utoipa::path(post, path = "/api/auth/logout", tag = "auth",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn logout(
    State(state): State<AppState>,
    current: CurrentUser,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    sessions::delete_by_id(&state.pool, current.session.id).await?;
    Ok((StatusCode::NO_CONTENT, jar.remove(session::removal())))
}

#[utoipa::path(post, path = "/api/auth/logout-all", tag = "auth",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn logout_all(
    State(state): State<AppState>,
    current: CurrentUser,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    sessions::delete_all_for_user(&state.pool, current.user.id).await?;
    Ok((StatusCode::NO_CONTENT, jar.remove(session::removal())))
}
