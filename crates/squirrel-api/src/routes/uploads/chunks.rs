//! `PUT /api/uploads/{id}/chunks/{index}`: one chunk at its offset.

use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::UploadPlan;
use squirrel_db::DbError;
use squirrel_db::uploads::{self, UploadRecord};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiBytes, ApiPath};
use crate::state::AppState;

/// Stores one chunk at its offset. Chunks may arrive in any order and may be
/// re-sent; each must have exactly the planned length.
#[utoipa::path(put, path = "/api/uploads/{id}/chunks/{index}", tag = "uploads",
    params(
        ("id" = Uuid, Path, description = "Upload id"),
        ("index" = u32, Path, description = "Zero-based chunk index"),
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 204, description = "Chunk stored"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The upload is being completed"),
        (status = 413, body = ErrorBody),
        (status = 422, body = ErrorBody, description = "Wrong index or length"),
        (status = 507, body = ErrorBody),
    ))]
pub async fn put_chunk(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((id, index)): ApiPath<(Uuid, u32)>,
    ApiBytes(body): ApiBytes,
) -> Result<StatusCode, ApiError> {
    let owner_id = current.user.id;
    let session = uploads::find(&state.pool, owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !session.is_open() {
        return Err(DbError::UploadBusy.into());
    }
    let offset = chunk_offset(&session, index, body.len())?;
    let store = state.storage.clone();
    run_storage(move || store.write_chunk(id, offset, &body)).await?;
    let index =
        i32::try_from(index).map_err(|_| ApiError::Validation("chunk index too large".into()))?;
    let expires_at = OffsetDateTime::now_utc() + state.config.upload_ttl;
    if !uploads::record_chunk(&state.pool, owner_id, id, index, expires_at).await? {
        // Completed, aborted or expired while the bytes were being written.
        return Err(DbError::UploadBusy.into());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Where chunk `index` goes, after checking it exists and has exactly the
/// planned length.
fn chunk_offset(session: &UploadRecord, index: u32, len: usize) -> Result<u64, ApiError> {
    let plan = UploadPlan::new(
        u64::try_from(session.size).unwrap_or_default(),
        u64::try_from(session.chunk_size).unwrap_or_default(),
    )
    .ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!(
            "upload {} has an invalid chunk size",
            session.id
        ))
    })?;
    let chunk = u64::from(index);
    let (Some(offset), Some(expected)) = (plan.offset(chunk), plan.chunk_len(chunk)) else {
        return Err(ApiError::Validation(format!(
            "chunk index must be below {}",
            plan.chunk_count()
        )));
    };
    if u64::try_from(len).ok() != Some(expected) {
        return Err(ApiError::Validation(format!(
            "chunk {index} must be exactly {expected} bytes"
        )));
    }
    Ok(offset)
}
