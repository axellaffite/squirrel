use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use squirrel_auth::{burn_verification_time, verify_password};
use squirrel_core::{Password, Username};
use squirrel_db::{sessions, users};
use time::OffsetDateTime;

use crate::blocking::run_hashing;
use crate::client_ip::ClientIp;
use crate::dto::{LoginRequest, MeResponse};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiJson;
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
    client_ip: ClientIp,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    let revoked = sessions::delete_all_for_user(&state.pool, current.user.id).await?;
    tracing::info!(
        user_id = %current.user.id,
        username = %current.user.username,
        %client_ip,
        revoked,
        "logged out of all sessions"
    );
    Ok((StatusCode::NO_CONTENT, jar.remove(session::removal())))
}

/// Inputs longer than this are rejected before hashing (protects the CPU).
const MAX_PASSWORD_BYTES: usize = Password::MAX_LEN * 4;

#[utoipa::path(post, path = "/api/auth/login", tag = "auth",
    request_body = LoginRequest,
    responses((status = 200, body = MeResponse), (status = 401, body = ErrorBody), (status = 429)))]
pub async fn login(
    State(state): State<AppState>,
    client_ip: ClientIp,
    jar: CookieJar,
    headers: HeaderMap,
    ApiJson(request): ApiJson<LoginRequest>,
) -> Result<(CookieJar, Json<MeResponse>), ApiError> {
    if request.password.len() > MAX_PASSWORD_BYTES {
        tracing::warn!(%client_ip, reason = "password too long", "login failed");
        return Err(ApiError::InvalidCredentials);
    }
    // Only a syntactically valid username is logged: a password typed into the
    // username field must never reach the logs.
    let username = Username::parse(&request.username).ok();
    let logged_name = username.as_ref().map_or("<invalid>", Username::as_str);
    let user = match &username {
        Some(username) => users::find_by_username(&state.pool, username).await?,
        None => None,
    };
    let now = OffsetDateTime::now_utc();
    let Some(user) = user.filter(|u| !u.is_disabled() && !u.is_locked(now)) else {
        tracing::warn!(
            username = logged_name,
            %client_ip,
            reason = "unknown, disabled or locked account",
            "login failed"
        );
        burn_verification(&state, request.password).await?;
        return Err(ApiError::InvalidCredentials);
    };

    if !verify(&state, request.password, user.password_hash.clone()).await? {
        let failures = users::record_login_failure(&state.pool, user.id).await?;
        tracing::warn!(user_id = %user.id, username = %user.username, %client_ip,
            failures, reason = "wrong password", "login failed");
        if failures >= users::LOCKOUT_THRESHOLD {
            tracing::warn!(user_id = %user.id, username = %user.username, %client_ip,
                failures, "account temporarily locked after repeated login failures");
        }
        return Err(ApiError::InvalidCredentials);
    }
    users::record_login_success(&state.pool, user.id).await?;
    tracing::info!(user_id = %user.id, username = %user.username, %client_ip, "login succeeded");
    let (cookie, me) = session::start(&state, &user, &headers, &jar).await?;
    Ok((jar.add(cookie), Json(me)))
}

async fn burn_verification(state: &AppState, password: String) -> Result<(), ApiError> {
    run_hashing(state, move || burn_verification_time(&password)).await
}

async fn verify(state: &AppState, password: String, stored_hash: String) -> Result<bool, ApiError> {
    Ok(run_hashing(state, move || verify_password(&password, &stored_hash)).await??)
}
