use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_token;
use squirrel_db::{invitations, users};

use crate::client_ip::ClientIp;
use crate::dto::{AccountRequest, JoinStatus, MeResponse};
use crate::error::{ApiError, ErrorBody};
use crate::input::{ApiJson, ApiPath};
use crate::routes::setup::prepare_new_user;
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/join/{token}", tag = "invitations",
    params(("token" = String, Path, description = "Invitation token from the link")),
    responses((status = 200, body = JoinStatus)))]
pub async fn check_invitation(
    State(state): State<AppState>,
    ApiPath(token): ApiPath<String>,
) -> Result<Json<JoinStatus>, ApiError> {
    let valid = invitations::is_redeemable(&state.pool, &hash_token(&token)).await?;
    Ok(Json(JoinStatus { valid }))
}

#[utoipa::path(post, path = "/api/join/{token}", tag = "invitations",
    params(("token" = String, Path, description = "Invitation token from the link")),
    request_body = AccountRequest,
    responses((status = 201, body = MeResponse), (status = 409, body = ErrorBody),
              (status = 410, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn accept_invitation(
    State(state): State<AppState>,
    ApiPath(token): ApiPath<String>,
    client_ip: ClientIp,
    jar: CookieJar,
    headers: HeaderMap,
    ApiJson(request): ApiJson<AccountRequest>,
) -> Result<(StatusCode, CookieJar, Json<MeResponse>), ApiError> {
    let token_hash = hash_token(&token);
    // Cheap pre-check to avoid hashing a password for a dead link;
    // `create_with_invitation` remains the authoritative atomic check.
    if !invitations::is_redeemable(&state.pool, &token_hash).await? {
        tracing::warn!(%client_ip, reason = "invalid invitation", "join failed");
        return Err(ApiError::InvitationInvalid);
    }
    let new_user = prepare_new_user(&state, request).await?;
    let user = users::create_with_invitation(&state.pool, &new_user, &token_hash)
        .await
        .inspect_err(|err| {
            tracing::warn!(username = %new_user.username.as_str(), %client_ip,
                reason = %err, "join failed");
        })?;
    tracing::info!(user_id = %user.id, username = %user.username,
        invited_by = ?user.invited_by, %client_ip, "user joined with an invitation");
    let (cookie, me) = session::start(&state, &user, &headers, &jar).await?;
    Ok((StatusCode::CREATED, jar.add(cookie), Json(me)))
}
