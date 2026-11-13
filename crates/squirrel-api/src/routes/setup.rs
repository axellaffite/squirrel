use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_password;
use squirrel_core::{DisplayName, Password, Username};
use squirrel_db::users::{self, NewUser};

use crate::blocking::run_blocking;
use crate::dto::{AccountRequest, MeResponse, SetupStatus};
use crate::error::{ApiError, ErrorBody};
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/setup/status", tag = "setup", responses((status = 200, body = SetupStatus)))]
pub async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    let needs_setup = !users::any_exists(&state.pool).await?;
    Ok(Json(SetupStatus { needs_setup }))
}

#[utoipa::path(
    post, path = "/api/setup/admin", tag = "setup",
    request_body = AccountRequest,
    responses(
        (status = 201, body = MeResponse, description = "Admin created and logged in"),
        (status = 409, body = ErrorBody, description = "Setup already complete"),
        (status = 422, body = ErrorBody),
    )
)]
pub async fn create_first_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<AccountRequest>,
) -> Result<(StatusCode, CookieJar, Json<MeResponse>), ApiError> {
    if users::any_exists(&state.pool).await? {
        return Err(ApiError::Conflict("setup is already complete".into()));
    }
    let new_user = prepare_new_user(request).await?;
    let user = users::create_first_admin(&state.pool, &new_user).await?;
    let (cookie, me) = session::start(&state, &user, &headers).await?;
    Ok((StatusCode::CREATED, jar.add(cookie), Json(me)))
}

/// Validates the account fields and hashes the password off the async executor.
pub async fn prepare_new_user(request: AccountRequest) -> Result<NewUser, ApiError> {
    let username = Username::parse(&request.username)?;
    let display_name = DisplayName::parse(&request.display_name)?;
    let password = Password::parse(&request.password)?;
    let password_hash = run_blocking(move || hash_password(&password)).await??;
    Ok(NewUser {
        username,
        display_name,
        password_hash,
    })
}
