use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_auth::{generate_token, hash_token};
use squirrel_db::invitations::{self, NewInvitation};
use squirrel_db::settings;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::client_ip::ClientIp;
use crate::dto::{CreateInvitationRequest, CreatedInvitation, InvitationDto};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;

const EMAIL_HINT_MAX_LEN: usize = 254;

#[utoipa::path(post, path = "/api/invitations", tag = "invitations",
    request_body = CreateInvitationRequest,
    responses((status = 201, body = CreatedInvitation), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_invitation(
    State(state): State<AppState>,
    current: CurrentUser,
    client_ip: ClientIp,
    ApiJson(request): ApiJson<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<CreatedInvitation>), ApiError> {
    let instance = settings::get(&state.pool).await?;
    if !instance.allows_invites_from(current.user.role()) {
        return Err(ApiError::Forbidden);
    }
    let email_hint = request
        .email_hint
        .map(|hint| hint.trim().to_owned())
        .filter(|hint| !hint.is_empty());
    if email_hint
        .as_ref()
        .is_some_and(|hint| hint.chars().count() > EMAIL_HINT_MAX_LEN)
    {
        return Err(ApiError::Validation(format!(
            "email hint must be at most {EMAIL_HINT_MAX_LEN} characters"
        )));
    }

    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = OffsetDateTime::now_utc();
    let record = invitations::create(
        &state.pool,
        &NewInvitation {
            created_by: current.user.id,
            token_hash: &token_hash,
            email_hint: email_hint.as_deref(),
            expires_at: now + state.config.invitation_ttl,
        },
    )
    .await?;
    tracing::info!(user_id = %current.user.id, username = %current.user.username,
        invitation_id = %record.id, %client_ip, "invitation created");
    let created = CreatedInvitation {
        invitation: InvitationDto::new(&record, now),
        token,
    };
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(get, path = "/api/invitations", tag = "invitations",
    responses((status = 200, body = Vec<InvitationDto>), (status = 401, body = ErrorBody)))]
pub async fn list_invitations(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<InvitationDto>>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let records = invitations::list_created_by(&state.pool, current.user.id).await?;
    Ok(Json(
        records
            .iter()
            .map(|record| InvitationDto::new(record, now))
            .collect(),
    ))
}

#[utoipa::path(delete, path = "/api/invitations/{id}", tag = "invitations",
    params(("id" = Uuid, Path, description = "Invitation id")),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    current: CurrentUser,
    client_ip: ClientIp,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    if invitations::revoke(&state.pool, id, current.user.id).await? {
        tracing::info!(user_id = %current.user.id, username = %current.user.username,
            invitation_id = %id, %client_ip, "invitation revoked");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
