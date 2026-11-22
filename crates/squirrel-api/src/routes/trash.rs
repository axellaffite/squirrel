//! The trash: list, restore, delete forever, empty.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_db::DbError;
use squirrel_db::trash::{self, TrashCursor};
use squirrel_ml::FACE_MODEL_ID;
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::{Caller, CurrentUser};
use crate::files_dto::{NodeDto, TrashEntryDto, TrashListing, TrashQuery};
use crate::input::{ApiPath, ApiQuery};
use crate::state::AppState;

#[utoipa::path(get, path = "/api/trash", security(("device_bearer" = []), ("session_cookie" = [])), tag = "trash", params(TrashQuery),
    responses((status = 200, body = TrashListing), (status = 400, body = ErrorBody)))]
pub async fn list_trash(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<TrashQuery>,
) -> Result<Json<TrashListing>, ApiError> {
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_trash)
        .transpose()?;
    let page = trash::list(
        &state.pool,
        caller.user.id,
        after.as_ref(),
        cursor::page_size(query.limit),
    )
    .await?;
    // Trash entries always have `deleted_at` (CHECK constraint).
    let items: Vec<TrashEntryDto> = page
        .items
        .iter()
        .filter_map(|node| {
            node.deleted_at.map(|deleted_at| TrashEntryDto {
                node: node.into(),
                deleted_at,
            })
        })
        .collect();
    let next_cursor = page
        .items
        .last()
        .filter(|_| page.has_more)
        .and_then(|last| {
            last.deleted_at.map(|deleted_at| {
                cursor::encode_trash(&TrashCursor {
                    deleted_at,
                    id: last.id,
                })
            })
        });
    Ok(Json(TrashListing { items, next_cursor }))
}

#[utoipa::path(post, path = "/api/trash/{id}/restore", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "trash",
    params(("id" = Uuid, Path, description = "Trash entry (node) id")),
    responses(
        (status = 200, body = NodeDto, description = "Restored into its folder, or into Files/ if that folder is gone; renamed \"name (n)\" on a clash"),
        (status = 404, body = ErrorBody),
        (status = 507, body = ErrorBody, description = "quota_exceeded")))]
pub async fn restore(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<NodeDto>, ApiError> {
    let mut tx = state.pool.begin().await.map_err(DbError::from)?;
    let restored = trash::restore_in(&mut tx, caller.user.id, id, FACE_MODEL_ID).await?;
    // Photos trashed while their blob was analysed have no scan for this owner.
    for sha in &restored.faces_due {
        squirrel_jobs::enqueue(&mut tx, &squirrel_jobs::kinds::faces(sha, false))
            .await
            .map_err(DbError::from)?;
    }
    // Photos leave their duplicates group in the trash: back, they are grouped
    // again, and none comes back hidden without a group to restore it from.
    squirrel_db::duplicates::unhide_orphans(&mut tx, caller.user.id).await?;
    if squirrel_db::duplicates::needs_grouping(&mut tx, caller.user.id).await? {
        let job = squirrel_jobs::kinds::group_duplicates(caller.user.id);
        squirrel_jobs::request_debounced(&mut tx, &job)
            .await
            .map_err(DbError::from)?;
    }
    tx.commit().await.map_err(DbError::from)?;
    Ok(Json((&restored.node).into()))
}

#[utoipa::path(delete, path = "/api/trash/{id}", security(("session_cookie" = [], "csrf_token" = [])), tag = "trash",
    params(("id" = Uuid, Path, description = "Trash entry (node) id")),
    responses((status = 204, description = "Deleted for good"), (status = 404, body = ErrorBody)))]
pub async fn delete_forever(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    trash::delete_forever(&state.pool, current.user.id, id).await?;
    tracing::info!(user_id = %current.user.id, node_id = %id, "trash entry deleted forever");
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/trash", security(("session_cookie" = [], "csrf_token" = [])), tag = "trash",
    responses((status = 204, description = "Trash emptied")))]
pub async fn empty_trash(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<StatusCode, ApiError> {
    let entries = trash::empty(&state.pool, current.user.id).await?;
    tracing::info!(user_id = %current.user.id, entries, "trash emptied");
    Ok(StatusCode::NO_CONTENT)
}
