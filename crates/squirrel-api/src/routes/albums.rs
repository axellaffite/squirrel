//! Albums of the caller's photos. Another user's album is exactly `not_found`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::AlbumName;
use squirrel_db::albums::{self, AlbumRecord, AlbumUpdate, CoverChange, UpdateOutcome};
use squirrel_db::timeline;
use uuid::Uuid;

use crate::albums_dto::{
    AlbumDto, AlbumItemsQuery, AlbumItemsRequest, CreateAlbumRequest, ItemsChanged, MAX_NODE_IDS,
    UpdateAlbumRequest,
};
use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::photos_dto::{AssetSummary, TimelinePage};
use crate::routes::photos::{photo_page_size, timeline_page};
use crate::state::AppState;

fn check_count(node_ids: &[Uuid], min: usize) -> Result<(), ApiError> {
    if node_ids.len() < min || node_ids.len() > MAX_NODE_IDS {
        return Err(ApiError::Validation(format!(
            "node_ids must hold between {min} and {MAX_NODE_IDS} ids"
        )));
    }
    Ok(())
}

/// The albums, with their covers' summaries looked up in one query.
async fn dtos(
    state: &AppState,
    owner: Uuid,
    records: &[AlbumRecord],
) -> Result<Vec<AlbumDto>, ApiError> {
    let ids: Vec<Uuid> = records.iter().filter_map(|a| a.cover_id).collect();
    let mut covers: HashMap<Uuid, AssetSummary> = timeline::summaries(&state.pool, owner, &ids)
        .await?
        .iter()
        .map(|record| (record.node_id, AssetSummary::from(record)))
        .collect();
    Ok(records
        .iter()
        .map(|album| AlbumDto {
            id: album.id,
            name: album.name.clone(),
            item_count: album.item_count,
            cover: album.cover_id.and_then(|id| covers.remove(&id)),
            updated_at: album.updated_at,
        })
        .collect())
}

async fn album(state: &AppState, owner: Uuid, id: Uuid) -> Result<AlbumDto, ApiError> {
    let record = albums::find(&state.pool, owner, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    dtos(state, owner, std::slice::from_ref(&record))
        .await?
        .pop()
        .ok_or(ApiError::NotFound)
}

#[utoipa::path(get, path = "/api/albums", security(("device_bearer" = []), ("session_cookie" = [])), tag = "albums",
    responses((status = 200, body = Vec<AlbumDto>, description = "Most recently changed first, at most 1000")))]
pub async fn list_albums(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<AlbumDto>>, ApiError> {
    let records = albums::list(&state.pool, caller.user.id).await?;
    Ok(Json(dtos(&state, caller.user.id, &records).await?))
}

#[utoipa::path(post, path = "/api/albums", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums", request_body = CreateAlbumRequest,
    responses((status = 201, body = AlbumDto), (status = 422, body = ErrorBody)))]
pub async fn create_album(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreateAlbumRequest>,
) -> Result<(StatusCode, Json<AlbumDto>), ApiError> {
    let name = AlbumName::parse(&request.name)?;
    check_count(&request.node_ids, 0)?;
    let owner = caller.user.id;
    let id = albums::create(&state.pool, owner, &name, &request.node_ids).await?;
    Ok((StatusCode::CREATED, Json(album(&state, owner, id).await?)))
}

#[utoipa::path(get, path = "/api/albums/{id}", security(("device_bearer" = []), ("session_cookie" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    responses((status = 200, body = AlbumDto), (status = 404, body = ErrorBody)))]
pub async fn get_album(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<AlbumDto>, ApiError> {
    Ok(Json(album(&state, caller.user.id, id).await?))
}

#[utoipa::path(patch, path = "/api/albums/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    request_body = UpdateAlbumRequest,
    responses((status = 200, body = AlbumDto), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn update_album(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<UpdateAlbumRequest>,
) -> Result<Json<AlbumDto>, ApiError> {
    if request.name.is_none() && request.cover_node_id.is_none() {
        return Err(ApiError::Validation(
            "give a new name, a cover_node_id, or both".into(),
        ));
    }
    let changes = AlbumUpdate {
        name: request.name.as_deref().map(AlbumName::parse).transpose()?,
        cover: match request.cover_node_id {
            None => CoverChange::Keep,
            Some(None) => CoverChange::Clear,
            Some(Some(id)) => CoverChange::Set(id),
        },
    };
    let owner = caller.user.id;
    match albums::update(&state.pool, id, owner, &changes).await? {
        UpdateOutcome::Updated => Ok(Json(album(&state, owner, id).await?)),
        UpdateOutcome::NotFound => Err(ApiError::NotFound),
        UpdateOutcome::CoverNotInAlbum => Err(ApiError::Validation(
            "the cover must be a photo of the album".into(),
        )),
    }
}

#[utoipa::path(delete, path = "/api/albums/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    responses((status = 204, description = "Deleted; its photos stay"), (status = 404, body = ErrorBody)))]
pub async fn delete_album(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    if albums::delete(&state.pool, id, caller.user.id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(get, path = "/api/albums/{id}/items", security(("device_bearer" = []), ("session_cookie" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id"), AlbumItemsQuery),
    responses((status = 200, body = TimelinePage), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)))]
pub async fn list_album_items(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<AlbumItemsQuery>,
) -> Result<Json<TimelinePage>, ApiError> {
    let owner = caller.user.id;
    if !albums::exists(&state.pool, owner, id).await? {
        return Err(ApiError::NotFound);
    }
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?;
    let page = albums::items(
        &state.pool,
        owner,
        id,
        after.as_ref(),
        photo_page_size(query.limit),
    )
    .await?;
    Ok(Json(timeline_page(page)))
}

#[utoipa::path(post, path = "/api/albums/{id}/items", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    request_body = AlbumItemsRequest,
    responses((status = 200, body = ItemsChanged), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn add_album_items(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AlbumItemsRequest>,
) -> Result<Json<ItemsChanged>, ApiError> {
    check_count(&request.node_ids, 1)?;
    let changed = albums::add_items(&state.pool, id, caller.user.id, &request.node_ids).await?;
    Ok(Json(ItemsChanged { changed }))
}

#[utoipa::path(post, path = "/api/albums/{id}/items/remove", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    request_body = AlbumItemsRequest,
    responses((status = 200, body = ItemsChanged), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn remove_album_items(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AlbumItemsRequest>,
) -> Result<Json<ItemsChanged>, ApiError> {
    check_count(&request.node_ids, 1)?;
    let changed = albums::remove_items(&state.pool, id, caller.user.id, &request.node_ids).await?;
    Ok(Json(ItemsChanged { changed }))
}
