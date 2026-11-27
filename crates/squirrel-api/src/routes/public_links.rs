//! The owner's side of public links: make, list and revoke the links of one
//! of their folders, files or albums.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_auth::{generate_token, hash_password, hash_token};
use squirrel_core::Password;
use squirrel_db::public_links::{self, NewPublicLink};
use squirrel_db::shares::Subject;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::access;
use crate::blocking::run_hashing;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::public_links_dto::{
    CreatePublicLinkRequest, CreatedPublicLink, PublicLinkDto, PublicLinkList,
};
use crate::routes::shares::subject_access;
use crate::sharing_dto::SubjectRef;
use crate::state::AppState;

/// `subject`, checked to be the caller's own.
async fn owned(state: &AppState, user: Uuid, subject: Subject) -> Result<(), ApiError> {
    let (_, access) = subject_access(state, user, subject).await?;
    access::require_owner(access)
}

#[utoipa::path(get, path = "/api/public-links", security(("device_bearer" = []), ("session_cookie" = [])), tag = "sharing",
    params(SubjectRef),
    responses(
        (status = 200, body = PublicLinkList),
        (status = 403, body = ErrorBody, description = "Only the owner sees its links"),
        (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody),
    ))]
pub async fn list_public_links(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(subject): ApiQuery<SubjectRef>,
) -> Result<Json<PublicLinkList>, ApiError> {
    let subject = subject.subject()?;
    owned(&state, caller.user.id, subject).await?;
    let now = OffsetDateTime::now_utc();
    let links = public_links::for_subject(&state.pool, subject).await?;
    Ok(Json(PublicLinkList {
        enabled: state.config.public_url.is_some(),
        links: links
            .iter()
            .map(|link| PublicLinkDto::new(link, now))
            .collect(),
    }))
}

#[utoipa::path(post, path = "/api/public-links", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "sharing",
    request_body = CreatePublicLinkRequest,
    responses(
        (status = 201, body = CreatedPublicLink, description = "The link and its URL, shown only now"),
        (status = 403, body = ErrorBody, description = "Only the owner makes links"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Public links are not set up on this server"),
        (status = 422, body = ErrorBody, description = "An expiry in the past, or a password too short or too long"),
        (status = 503, body = ErrorBody, description = "busy: no password hashing slot"),
    ))]
pub async fn create_public_link(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreatePublicLinkRequest>,
) -> Result<(StatusCode, Json<CreatedPublicLink>), ApiError> {
    let subject = request.subject.subject()?;
    owned(&state, caller.user.id, subject).await?;
    let public_url = state.config.public_url.clone().ok_or_else(|| {
        ApiError::Conflict(
            "public links are not set up on this server (SQUIRREL_PUBLIC_URL)".into(),
        )
    })?;
    if public_links::count_for_subject(&state.pool, subject).await? >= public_links::MAX_LINKS {
        return Err(ApiError::Conflict(format!(
            "an item has at most {} public links: revoke some first",
            public_links::MAX_LINKS
        )));
    }
    let now = OffsetDateTime::now_utc();
    if request.expires_at.is_some_and(|at| at <= now) {
        return Err(ApiError::Validation(
            "the expiry must be in the future".into(),
        ));
    }
    let password_hash = match request.password.as_deref() {
        Some(raw) => {
            let password = Password::parse(raw)?;
            Some(run_hashing(&state, move || hash_password(&password)).await??)
        }
        None => None,
    };
    let token = generate_token();
    let link = public_links::create(
        &state.pool,
        &NewPublicLink {
            owner_id: caller.user.id,
            subject,
            token_hash: &hash_token(&token),
            password_hash: password_hash.as_deref(),
            expires_at: request.expires_at,
        },
    )
    .await?;
    tracing::info!(user_id = %caller.user.id, link_id = %link.id, "public link created");
    Ok((
        StatusCode::CREATED,
        Json(CreatedPublicLink {
            link: PublicLinkDto::new(&link, now),
            url: public_url.link(&token),
        }),
    ))
}

#[utoipa::path(delete, path = "/api/public-links/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "sharing",
    params(("id" = Uuid, Path, description = "Link id")),
    responses(
        (status = 204, description = "Revoked: the link no longer opens"),
        (status = 404, body = ErrorBody, description = "No such link of the caller's"),
    ))]
pub async fn delete_public_link(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    if public_links::delete(&state.pool, caller.user.id, id).await? {
        tracing::info!(user_id = %caller.user.id, link_id = %id, "public link revoked");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
