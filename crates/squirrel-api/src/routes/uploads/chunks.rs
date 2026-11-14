//! `PUT /api/uploads/{id}/chunks/{index}`: one chunk at its offset.

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use squirrel_core::UploadPlan;
use squirrel_db::DbError;
use squirrel_db::uploads::{self, ChunkWrite, UploadRecord};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiPath;
use crate::state::AppState;

/// Where a chunk goes and how long it must be.
struct Slot {
    offset: u64,
    len: u64,
}

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
        (status = 404, body = ErrorBody, description = "No such upload (or it was aborted, completed or expired)"),
        (status = 409, body = ErrorBody, description = "The upload is being completed"),
        (status = 413, body = ErrorBody, description = "Longer than the planned chunk"),
        (status = 422, body = ErrorBody, description = "Wrong index or length"),
        (status = 503, body = ErrorBody, description = "busy: too many chunk uploads at once"),
        (status = 507, body = ErrorBody),
    ))]
pub async fn put_chunk(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((id, index)): ApiPath<(Uuid, u32)>,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let owner_id = current.user.id;
    let session = open_session(&state, owner_id, id).await?;
    let slot = chunk_slot(&session, index)?;
    // Everything below happens before the body is read.
    check_declared_length(&headers, index, slot.len)?;
    let _slot = state
        .uploads
        .chunks
        .acquire(owner_id)
        .await
        .ok_or(ApiError::Busy)?;
    ensure_free_space(&state, slot.len).await?;
    let data = read_chunk(body, index, slot.len).await?;

    // Completion takes this lock exclusively from its claim until the blob is
    // stored: the session is re-checked and written under it.
    let _guard = state
        .uploads
        .locks
        .chunk(id, state.config.upload_limits.lock_wait)
        .await
        .ok_or(DbError::UploadBusy)?;
    open_session(&state, owner_id, id).await?;
    let store = state.storage.clone();
    run_storage(move || store.write_chunk(id, slot.offset, &data)).await?;
    let index =
        i32::try_from(index).map_err(|_| ApiError::Validation("chunk index too large".into()))?;
    let expires_at = OffsetDateTime::now_utc() + state.config.upload_ttl;
    match uploads::record_chunk(&state.pool, owner_id, id, index, expires_at).await? {
        ChunkWrite::Recorded => Ok(StatusCode::NO_CONTENT),
        ChunkWrite::Busy => Err(DbError::UploadBusy.into()),
        ChunkWrite::Gone => Err(ApiError::NotFound),
    }
}

async fn open_session(
    state: &AppState,
    owner_id: Uuid,
    id: Uuid,
) -> Result<UploadRecord, ApiError> {
    let session = uploads::find(&state.pool, owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !session.is_open() {
        return Err(DbError::UploadBusy.into());
    }
    Ok(session)
}

/// Where chunk `index` goes and its planned length.
fn chunk_slot(session: &UploadRecord, index: u32) -> Result<Slot, ApiError> {
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
    match (plan.offset(chunk), plan.chunk_len(chunk)) {
        (Some(offset), Some(len)) => Ok(Slot { offset, len }),
        _ => Err(ApiError::Validation(format!(
            "chunk index must be below {}",
            plan.chunk_count()
        ))),
    }
}

fn wrong_length(index: u32, expected: u64, actual: u64) -> ApiError {
    if actual > expected {
        ApiError::PayloadTooLarge
    } else {
        ApiError::Validation(format!("chunk {index} must be exactly {expected} bytes"))
    }
}

/// Refuses a body whose declared `Content-Length` is not the planned length.
fn check_declared_length(headers: &HeaderMap, index: u32, expected: u64) -> Result<(), ApiError> {
    let Some(value) = headers.get(header::CONTENT_LENGTH) else {
        return Ok(());
    };
    let declared = value
        .to_str()
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| ApiError::BadRequest("invalid Content-Length".into()))?;
    if declared == expected {
        Ok(())
    } else {
        Err(wrong_length(index, expected, declared))
    }
}

/// Reads at most `expected` bytes and checks the body had exactly that many.
async fn read_chunk(body: Body, index: u32, expected: u64) -> Result<Bytes, ApiError> {
    let limit = usize::try_from(expected).unwrap_or(usize::MAX);
    let data = Limited::new(body, limit)
        .collect()
        .await
        .map_err(|err| {
            if err.is::<LengthLimitError>() {
                ApiError::PayloadTooLarge
            } else {
                ApiError::BadRequest("the request body could not be read".into())
            }
        })?
        .to_bytes();
    let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
    if len != expected {
        return Err(wrong_length(index, expected, len));
    }
    Ok(data)
}

/// Refuses a chunk that would take the disk below `min_free_bytes`.
async fn ensure_free_space(state: &AppState, len: u64) -> Result<(), ApiError> {
    let store = state.storage.clone();
    let available = run_storage(move || store.available_space()).await?;
    if available.saturating_sub(len) < state.config.min_free_bytes {
        tracing::warn!(available, len, "refusing chunk: not enough free disk space");
        return Err(ApiError::StorageFull);
    }
    Ok(())
}
