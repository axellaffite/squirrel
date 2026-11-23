//! `POST /api/uploads/{id}/complete`: verify, store the blob, create the node.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::{ContentHash, NodeName};
use squirrel_db::nodes::{self, NewFile, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::uploads::{self, UploadRecord};
use squirrel_db::{DbError, blobs};
use squirrel_storage::{StorageError, mime};

use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::files_dto::NodeDto;
use crate::input::ApiPath;
use crate::state::AppState;

/// How a failed completion leaves the session.
enum Failure {
    /// The client can fix the cause and complete again (missing chunks, quota).
    Retry(ApiError),
    /// The session and its file are discarded (bad hash, upload file or
    /// destination gone, or a failure after the file moved into the blob store).
    Discard(ApiError),
}

fn retry(err: impl Into<ApiError>) -> Failure {
    Failure::Retry(err.into())
}

fn discard(err: impl Into<ApiError>) -> Failure {
    Failure::Discard(err.into())
}

#[utoipa::path(post, path = "/api/uploads/{id}/complete", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses(
        (status = 201, body = NodeDto, description = "File created; its name may carry a \" (n)\" suffix if the name was taken"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "upload_incomplete, already completing, or destination folder gone"),
        (status = 422, body = ErrorBody, description = "hash_mismatch: the upload was discarded"),
        (status = 503, body = ErrorBody, description = "busy: too many completions at once; retry"),
        (status = 507, body = ErrorBody),
    ))]
pub async fn complete_upload(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<(StatusCode, Json<NodeDto>), ApiError> {
    let owner_id = caller.user.id;
    let slot = state
        .uploads
        .completions
        .acquire(owner_id)
        .await
        .ok_or_else(|| {
            tracing::warn!(user_id = %owner_id, upload_id = %id, "no completion slot, answering busy");
            ApiError::Busy
        })?;
    // Held from the claim until the blob is adopted and linked (or the session
    // released or discarded): no chunk write can land after verification.
    let guard = state
        .uploads
        .locks
        .completion(id, state.config.upload_limits.lock_wait)
        .await
        .ok_or(DbError::UploadBusy)?;
    // From the claim on, the work runs to the end even if the client
    // disconnects: a dropped request must not leave the session stuck in
    // `completing` (the claim itself runs in the task, so it cannot be cut
    // off between claiming and starting the work either).
    let task_state = state.clone();
    let task = tokio::spawn(async move {
        let _held = (slot, guard);
        let expires_at = OffsetDateTime::now_utc() + task_state.config.upload_ttl;
        let session = uploads::claim(&task_state.pool, owner_id, id, expires_at).await?;
        complete_claimed(&task_state, &session, expires_at).await
    });
    let node = task.await.map_err(|err| ApiError::Internal(err.into()))??;
    Ok((StatusCode::CREATED, Json((&node).into())))
}

async fn complete_claimed(
    state: &AppState,
    session: &UploadRecord,
    expires_at: OffsetDateTime,
) -> Result<NodeRecord, ApiError> {
    let (owner_id, id) = (session.owner_id, session.id);
    match finish(state, session).await {
        Ok(node) => {
            tracing::info!(user_id = %owner_id, upload_id = %id, node_id = %node.id,
                size = node.size, "upload completed");
            Ok(node)
        }
        Err(Failure::Retry(err)) => {
            let uploader = session.created_by;
            if let Err(release) = uploads::release(&state.pool, uploader, id, expires_at).await {
                // The claim lease lets a later complete take the session again.
                tracing::error!(upload_id = %id, error = %release,
                    "could not release an upload session after a failed completion");
            }
            Err(err)
        }
        Err(Failure::Discard(err)) => {
            discard_session(state, id).await;
            Err(err)
        }
    }
}

async fn finish(state: &AppState, session: &UploadRecord) -> Result<NodeRecord, Failure> {
    let received = uploads::received_chunks(&state.pool, session.id)
        .await
        .map_err(retry)?;
    let expected = usize::try_from(session.chunk_count).unwrap_or_default();
    if received.len() < expected {
        let missing = expected - received.len();
        return Err(retry(ApiError::UploadIncomplete(format!(
            "{missing} chunk(s) have not been received yet"
        ))));
    }
    let claimed = ContentHash::parse(&session.sha256)
        .map_err(|err| discard(ApiError::Internal(err.into())))?;
    let name =
        NodeName::parse(&session.name).map_err(|err| discard(ApiError::Internal(err.into())))?;
    let (store, id) = (state.storage.clone(), session.id);
    let hashed = run_storage(move || store.hash_upload(id))
        .await
        .map_err(|err| match err {
            // The upload file is gone: retrying cannot bring it back.
            ApiError::NotFound => discard(err),
            other => Failure::Retry(other),
        })?;
    if hashed.hash != claimed || i64::try_from(hashed.size).ok() != Some(session.size) {
        tracing::warn!(upload_id = %session.id, "uploaded content does not match its sha256");
        return Err(discard(ApiError::HashMismatch));
    }
    let mime = mime::detect(&hashed.head, name.as_str());
    store_and_link(state, session, &claimed, &name, &mime).await
}

/// One transaction: owner lock and quota, blob lock, move the file into the
/// blob store (idempotent), record blob and node, drop the session.
async fn store_and_link(
    state: &AppState,
    session: &UploadRecord,
    hash: &ContentHash,
    name: &NodeName,
    mime: &str,
) -> Result<NodeRecord, Failure> {
    let owner_id = session.owner_id;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|err| retry(DbError::from(err)))?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await.map_err(retry)?;
    nodes::find_live_folder(&mut *tx, owner_id, session.parent_id)
        .await
        .map_err(retry)?
        .ok_or_else(|| {
            discard(ApiError::Conflict(
                "the destination folder no longer exists".into(),
            ))
        })?;
    quota::ensure_room(
        &mut tx,
        owner_id,
        quota_bytes,
        session.size,
        Pending::Ignore,
    )
    .await
    .map_err(retry)?;
    // `adopt_upload` requires the per-blob lock.
    blobs::lock(&mut tx, hash).await.map_err(retry)?;

    let (store, id, blob) = (state.storage.clone(), session.id, hash.clone());
    let size = u64::try_from(session.size).unwrap_or_default();
    tokio::task::spawn_blocking(move || store.adopt_upload(id, &blob, size))
        .await
        .map_err(|err| retry(ApiError::Internal(err.into())))?
        .map_err(|err| match err {
            // The upload file is gone: completing again cannot work.
            StorageError::Moved(_) => discard(err),
            other => retry(other),
        })?;
    // The upload file has moved: from here on a failure cannot be retried.
    // Content already catalogued keeps its type: every node of a blob reports
    // the blob's MIME (as dedup links do); fresh content gets the sniffed one.
    let blob = blobs::insert_if_absent(&mut tx, hash, session.size, mime)
        .await
        .map_err(discard)?;
    let mime = blob.mime.as_str();
    let file = NewFile {
        owner_id,
        parent_id: session.parent_id,
        name,
        blob: hash,
        size: session.size,
        mime,
        client_taken: session.client_taken(),
    };
    let node = nodes::insert_file(&mut tx, &file).await.map_err(discard)?;
    super::create::record_device_asset(&mut tx, session.device_asset(), &node)
        .await
        .map_err(discard)?;
    super::enqueue_media(&mut tx, &node)
        .await
        .map_err(discard)?;
    uploads::delete(&mut *tx, session.id)
        .await
        .map_err(discard)?;
    tx.commit()
        .await
        .map_err(|err| discard(DbError::from(err)))?;
    Ok(node)
}

/// Best effort: the expiry sweep catches anything left behind.
async fn discard_session(state: &AppState, id: Uuid) {
    if let Err(err) = uploads::delete(&state.pool, id).await {
        tracing::warn!(upload_id = %id, error = %err, "could not delete upload session");
    }
    let store = state.storage.clone();
    if let Err(err) = run_storage(move || store.discard_upload(id)).await {
        tracing::warn!(upload_id = %id, error = %err, "could not delete upload file");
    }
}
