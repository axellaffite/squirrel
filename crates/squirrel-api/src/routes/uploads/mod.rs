//! Resumable, deduplicating chunked uploads.

pub mod chunks;
pub mod complete;
pub(crate) mod create;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sqlx::PgConnection;
use squirrel_auth::{DeviceToken, hash_token};
use squirrel_core::MediaKind;
use squirrel_db::nodes::NodeRecord;
use squirrel_db::{DbError, uploads};
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::{Caller, UploadCaller};
use crate::files_dto::{CreateUploadRequest, DeduplicatedUpload, UploadSessionDto};
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;

/// Queues media processing of a new file node in the transaction creating it,
/// so a committed image or document always has its processing queued.
pub(crate) async fn enqueue_media(
    conn: &mut PgConnection,
    node: &NodeRecord,
) -> Result<(), DbError> {
    if node.mime.as_deref().and_then(MediaKind::for_mime).is_some() {
        squirrel_jobs::enqueue(conn, &squirrel_jobs::kinds::process_media(node.id, false)).await?;
    }
    Ok(())
}

#[utoipa::path(post, path = "/api/uploads", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "uploads",
    request_body = CreateUploadRequest,
    responses(
        (status = 200, body = DeduplicatedUpload, description = "Content already in your library: linked, nothing to upload"),
        (status = 201, body = UploadSessionDto, description = "Session opened or resumed: send the missing chunks. A device also gets a new `upload_token` (the previous one stops working)"),
        (status = 404, body = ErrorBody, description = "Destination folder not found"),
        (status = 422, body = ErrorBody),
        (status = 429, body = ErrorBody, description = "rate_limited, or too_many_uploads: too many open sessions, or they would reserve more than half the free space"),
        (status = 507, body = ErrorBody, description = "quota_exceeded or storage_full"),
    ))]
pub async fn create_upload(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreateUploadRequest>,
) -> Result<Response, ApiError> {
    let owner_id = caller.user.id;
    if let Some(limiter) = &state.uploads.create_rate
        && !limiter.check(owner_id)
    {
        tracing::warn!(user_id = %owner_id, "upload creation rate limited");
        return Err(ApiError::RateLimited);
    }
    let input = create::UploadInput::parse(request, caller.device().map(|device| device.id))?;
    if let Some(node) = create::link_existing(&state, owner_id, &input).await? {
        let body = DeduplicatedUpload {
            deduplicated: true,
            node: (&node).into(),
        };
        return Ok((StatusCode::OK, Json(body)).into_response());
    }
    let session = create::open_session(&state, owner_id, &input).await?;
    let received = uploads::received_chunks(&state.pool, session.id).await?;
    let mut body = UploadSessionDto::new(&session, &received);
    if let Some(device) = caller.device() {
        // A fresh token on every open or resume: the previous one stops working.
        let token = DeviceToken::upload();
        if uploads::set_upload_token(
            &state.pool,
            owner_id,
            session.id,
            device.id,
            &hash_token(&token),
        )
        .await?
        {
            body.upload_token = Some(token);
        }
    }
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

#[utoipa::path(get, path = "/api/uploads/{id}", security(("device_bearer" = []), ("upload_token" = []), ("session_cookie" = [])), tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses((status = 200, body = UploadSessionDto), (status = 404, body = ErrorBody)))]
pub async fn upload_status(
    State(state): State<AppState>,
    caller: UploadCaller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<UploadSessionDto>, ApiError> {
    let session = uploads::find(&state.pool, caller.owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let received = uploads::received_chunks(&state.pool, id).await?;
    Ok(Json(UploadSessionDto::new(&session, &received)))
}

#[utoipa::path(delete, path = "/api/uploads/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses(
        (status = 204, description = "Upload aborted and its bytes deleted"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The upload is being completed"),
    ))]
pub async fn abort_upload(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    uploads::abort(&state.pool, caller.user.id, id).await?;
    let store = state.storage.clone();
    run_storage(move || store.discard_upload(id)).await?;
    tracing::info!(user_id = %caller.user.id, upload_id = %id, "upload aborted");
    Ok(StatusCode::NO_CONTENT)
}
