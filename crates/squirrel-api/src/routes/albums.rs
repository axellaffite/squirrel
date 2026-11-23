//! Albums of the caller's photos, and albums shared with them. An album that
//! is neither is exactly `not_found`; renaming, choosing the cover and deleting
//! stay with the owner; people it is shared with for writing add and remove
//! their own photos.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::AlbumName;
use squirrel_db::access::Access;
use squirrel_db::albums::{self, AlbumRecord, AlbumUpdate, CoverChange, Remover, UpdateOutcome};
use squirrel_db::{timeline, users};
use uuid::Uuid;

use crate::access;
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
use crate::sharing_dto::MemberDto;
use crate::state::AppState;

fn check_count(node_ids: &[Uuid], min: usize) -> Result<(), ApiError> {
    if node_ids.len() < min || node_ids.len() > MAX_NODE_IDS {
        return Err(ApiError::Validation(format!(
            "node_ids must hold between {min} and {MAX_NODE_IDS} ids"
        )));
    }
    Ok(())
}

/// How the caller sees an album: their access and, for a shared one, its owner.
pub(crate) struct Viewpoint {
    pub access: Access,
    pub shared_by: Option<MemberDto>,
}

impl Viewpoint {
    pub const OWNER: Self = Self {
        access: Access::Owner,
        shared_by: None,
    };
}

/// The albums, with their covers' summaries looked up in one query. Covers are
/// live items of albums the caller may see (any contributor's photo).
pub(crate) async fn dtos(
    state: &AppState,
    records: &[(AlbumRecord, Viewpoint)],
) -> Result<Vec<AlbumDto>, ApiError> {
    let ids: Vec<Uuid> = records.iter().filter_map(|(a, _)| a.cover_id).collect();
    let mut covers: HashMap<Uuid, AssetSummary> = timeline::summaries_any(&state.pool, &ids)
        .await?
        .iter()
        .map(|record| (record.node_id, AssetSummary::from(record)))
        .collect();
    Ok(records
        .iter()
        .map(|(album, viewpoint)| AlbumDto {
            id: album.id,
            name: album.name.clone(),
            item_count: album.item_count,
            cover: album.cover_id.and_then(|id| covers.remove(&id)),
            updated_at: album.updated_at,
            access: viewpoint.access.into(),
            shared_by: viewpoint.shared_by.clone(),
        })
        .collect())
}

async fn album(state: &AppState, user: Uuid, id: Uuid) -> Result<AlbumDto, ApiError> {
    let found = access::album(state, user, id).await?;
    let record = albums::find_any(&state.pool, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let shared_by = if found.access.is_owner() {
        None
    } else {
        users::find_by_id(&state.pool, found.owner_id)
            .await?
            .as_ref()
            .map(MemberDto::from)
    };
    let viewpoint = Viewpoint {
        access: found.access,
        shared_by,
    };
    dtos(state, &[(record, viewpoint)])
        .await?
        .pop()
        .ok_or(ApiError::NotFound)
}

/// Renaming, choosing the cover and deleting: the owner only.
async fn owned(state: &AppState, user: Uuid, id: Uuid) -> Result<(), ApiError> {
    access::require_owner(access::album(state, user, id).await?.access)
}

#[utoipa::path(get, path = "/api/albums", security(("device_bearer" = []), ("session_cookie" = [])), tag = "albums",
    responses((status = 200, body = Vec<AlbumDto>, description = "Most recently changed first, at most 1000")))]
pub async fn list_albums(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<AlbumDto>>, ApiError> {
    let records: Vec<_> = albums::list(&state.pool, caller.user.id)
        .await?
        .into_iter()
        .map(|record| (record, Viewpoint::OWNER))
        .collect();
    Ok(Json(dtos(&state, &records).await?))
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
    responses((status = 200, body = AlbumDto), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody),
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
    owned(&state, owner, id).await?;
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
    responses((status = 204, description = "Deleted; its photos stay"), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody)))]
pub async fn delete_album(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    owned(&state, caller.user.id, id).await?;
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
    access::album(&state, caller.user.id, id).await?;
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?;
    let page = albums::items(
        &state.pool,
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
    responses((status = 200, body = ItemsChanged), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn add_album_items(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AlbumItemsRequest>,
) -> Result<Json<ItemsChanged>, ApiError> {
    check_count(&request.node_ids, 1)?;
    let found = access::album(&state, caller.user.id, id).await?;
    access::require(found.access.can_write())?;
    let changed = albums::add_items(&state.pool, id, caller.user.id, &request.node_ids).await?;
    Ok(Json(ItemsChanged { changed }))
}

#[utoipa::path(post, path = "/api/albums/{id}/items/remove", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "albums",
    params(("id" = Uuid, Path, description = "Album id")),
    request_body = AlbumItemsRequest,
    responses((status = 200, body = ItemsChanged), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn remove_album_items(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AlbumItemsRequest>,
) -> Result<Json<ItemsChanged>, ApiError> {
    check_count(&request.node_ids, 1)?;
    let found = access::album(&state, caller.user.id, id).await?;
    let remover = match found.access {
        Access::Owner => Remover::AlbumOwner,
        Access::Write => Remover::Contributor(caller.user.id),
        Access::Read => return Err(ApiError::Forbidden),
    };
    let changed = albums::remove_items(&state.pool, id, remover, &request.node_ids).await?;
    Ok(Json(ItemsChanged { changed }))
}
