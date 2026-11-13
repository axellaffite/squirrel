//! Resumable, deduplicating chunked uploads.

pub mod chunks;
mod create;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use squirrel_db::uploads;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::{CreateUploadRequest, DeduplicatedUpload, UploadSessionDto};
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;

#[utoipa::path(post, path = "/api/uploads", tag = "uploads",
    request_body = CreateUploadRequest,
    responses(
        (status = 200, body = DeduplicatedUpload, description = "Content already in your library: linked, nothing to upload"),
        (status = 201, body = UploadSessionDto, description = "Session opened or resumed: send the missing chunks"),
        (status = 404, body = ErrorBody, description = "Destination folder not found"),
        (status = 422, body = ErrorBody),
        (status = 507, body = ErrorBody, description = "quota_exceeded or storage_full"),
    ))]
pub async fn create_upload(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(request): ApiJson<CreateUploadRequest>,
) -> Result<Response, ApiError> {
    let input = create::UploadInput::parse(request)?;
    let owner_id = current.user.id;
    if let Some(node) = create::link_existing(&state, owner_id, &input).await? {
        let body = DeduplicatedUpload {
            deduplicated: true,
            node: (&node).into(),
        };
        return Ok((StatusCode::OK, Json(body)).into_response());
    }
    let session = create::open_session(&state, owner_id, &input).await?;
    let received = uploads::received_chunks(&state.pool, session.id).await?;
    let body = UploadSessionDto::new(&session, &received);
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

#[utoipa::path(get, path = "/api/uploads/{id}", tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses((status = 200, body = UploadSessionDto), (status = 404, body = ErrorBody)))]
pub async fn upload_status(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<UploadSessionDto>, ApiError> {
    let session = uploads::find(&state.pool, current.user.id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let received = uploads::received_chunks(&state.pool, id).await?;
    Ok(Json(UploadSessionDto::new(&session, &received)))
}

#[utoipa::path(delete, path = "/api/uploads/{id}", tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses(
        (status = 204, description = "Upload aborted and its bytes deleted"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The upload is being completed"),
    ))]
pub async fn abort_upload(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    uploads::abort(&state.pool, current.user.id, id).await?;
    let store = state.storage.clone();
    run_storage(move || store.discard_upload(id)).await?;
    tracing::info!(user_id = %current.user.id, upload_id = %id, "upload aborted");
    Ok(StatusCode::NO_CONTENT)
}
