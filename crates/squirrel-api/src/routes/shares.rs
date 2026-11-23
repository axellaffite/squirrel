//! Sharing folders, files and albums with other accounts of this instance.
//! Only the owner shares, lists and changes shares; a recipient may leave one.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::Username;
use squirrel_db::access::Access;
use squirrel_db::shares::{self, ShareRecord, Subject};
use squirrel_db::{albums, media, nodes, users};
use uuid::Uuid;

use crate::access;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::NodeDto;
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::routes::albums::{Viewpoint, dtos};
use crate::sharing_dto::{CreateShareRequest, PersonDto, ShareDto, SharedItemDto, SubjectRef};
use crate::state::AppState;

/// The subject's owner and the caller's access to it (`not_found` without any).
pub(crate) async fn subject_access(
    state: &AppState,
    user: Uuid,
    subject: Subject,
) -> Result<(Uuid, Access), ApiError> {
    Ok(match subject {
        Subject::Node(id) => {
            let found = access::node(state, user, id).await?;
            (found.node.owner_id, found.access)
        }
        Subject::Album(id) => {
            let found = access::album(state, user, id).await?;
            (found.owner_id, found.access)
        }
    })
}

/// `subject`, checked to be the caller's own.
async fn owned_subject(state: &AppState, user: Uuid, subject: Subject) -> Result<(), ApiError> {
    let (_, access) = subject_access(state, user, subject).await?;
    access::require_owner(access)
}

#[utoipa::path(post, path = "/api/shares", security(("session_cookie" = [], "csrf_token" = [])), tag = "sharing",
    request_body = CreateShareRequest,
    responses(
        (status = 201, body = ShareDto, description = "Shared, or the existing share's permission changed. Lowering an album share to read removes the photos that person added"),
        (status = 403, body = ErrorBody, description = "Only the owner shares"),
        (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody, description = "No such account, or yourself"),
    ))]
pub async fn create_share(
    State(state): State<AppState>,
    caller: CurrentUser,
    ApiJson(request): ApiJson<CreateShareRequest>,
) -> Result<(StatusCode, Json<ShareDto>), ApiError> {
    let subject = request.subject.subject()?;
    owned_subject(&state, caller.user.id, subject).await?;
    let no_account = || ApiError::Validation("there is no account with this username".into());
    let username = Username::parse(request.username.trim()).map_err(|_| no_account())?;
    let recipient = users::find_by_username(&state.pool, &username)
        .await?
        .filter(|user| user.disabled_at.is_none())
        .ok_or_else(no_account)?;
    if recipient.id == caller.user.id {
        return Err(ApiError::Validation(
            "this is already yours: share it with someone else".into(),
        ));
    }
    let id = shares::upsert(
        &state.pool,
        caller.user.id,
        subject,
        recipient.id,
        request.permission.into(),
    )
    .await?;
    let share = shares::find(&state.pool, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    tracing::info!(user_id = %caller.user.id, share_id = %id, "shared");
    Ok((StatusCode::CREATED, Json(ShareDto::from(&share))))
}

#[utoipa::path(get, path = "/api/shares", security(("session_cookie" = [])), tag = "sharing",
    params(SubjectRef),
    responses(
        (status = 200, body = Vec<ShareDto>, description = "The people it is shared with, in sharing order"),
        (status = 403, body = ErrorBody, description = "Only the owner sees who it is shared with"),
        (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody),
    ))]
pub async fn list_shares(
    State(state): State<AppState>,
    caller: CurrentUser,
    ApiQuery(subject): ApiQuery<SubjectRef>,
) -> Result<Json<Vec<ShareDto>>, ApiError> {
    let subject = subject.subject()?;
    owned_subject(&state, caller.user.id, subject).await?;
    let list = shares::for_subject(&state.pool, subject).await?;
    Ok(Json(list.iter().map(ShareDto::from).collect()))
}

#[utoipa::path(delete, path = "/api/shares/{id}", security(("session_cookie" = [], "csrf_token" = [])), tag = "sharing",
    params(("id" = Uuid, Path, description = "Share id")),
    responses(
        (status = 204, description = "Stopped sharing (owner) or left (recipient). An album recipient's photos leave the album"),
        (status = 404, body = ErrorBody),
    ))]
pub async fn delete_share(
    State(state): State<AppState>,
    caller: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    if shares::delete(&state.pool, caller.user.id, id).await? {
        tracing::info!(user_id = %caller.user.id, share_id = %id, "share removed");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(get, path = "/api/shared-with-me", security(("session_cookie" = [])), tag = "sharing",
    responses((status = 200, body = Vec<SharedItemDto>, description = "Newest first, at most 1000")))]
pub async fn shared_with_me(
    State(state): State<AppState>,
    caller: CurrentUser,
) -> Result<Json<Vec<SharedItemDto>>, ApiError> {
    let list = shares::shared_with(&state.pool, caller.user.id).await?;
    let mut nodes = node_dtos(&state, &list).await?;
    let mut albums = album_dtos(&state, &list).await?;
    let items = list
        .iter()
        .filter_map(|share| {
            let node = share.node_id.and_then(|id| nodes.remove(&id));
            let album = share.album_id.and_then(|id| albums.remove(&id));
            // A subject trashed or deleted meanwhile is left out.
            (node.is_some() || album.is_some()).then(|| SharedItemDto {
                share_id: share.id,
                owner: owner_of(share),
                permission: share.permission().into(),
                shared_at: share.created_at,
                node,
                album,
            })
        })
        .collect();
    Ok(Json(items))
}

fn owner_of(share: &ShareRecord) -> PersonDto {
    PersonDto {
        username: share.person_username.clone(),
        display_name: share.person_display_name.clone(),
    }
}

async fn node_dtos(
    state: &AppState,
    list: &[ShareRecord],
) -> Result<HashMap<Uuid, NodeDto>, ApiError> {
    let ids: Vec<Uuid> = list.iter().filter_map(|share| share.node_id).collect();
    let records = nodes::find_live_many(&state.pool, &ids).await?;
    let hashes: Vec<String> = records
        .iter()
        .filter_map(|node| node.blob_sha256.clone())
        .collect();
    let thumbnails = media::thumbnails_for(&state.pool, &hashes).await?;
    Ok(records
        .iter()
        .map(|node| (node.id, NodeDto::with_thumbnails(node, &thumbnails)))
        .collect())
}

async fn album_dtos(
    state: &AppState,
    list: &[ShareRecord],
) -> Result<HashMap<Uuid, crate::albums_dto::AlbumDto>, ApiError> {
    let shares_by_album: HashMap<Uuid, &ShareRecord> = list
        .iter()
        .filter_map(|share| share.album_id.map(|id| (id, share)))
        .collect();
    let ids: Vec<Uuid> = shares_by_album.keys().copied().collect();
    let records: Vec<_> = albums::find_many(&state.pool, &ids)
        .await?
        .into_iter()
        .filter_map(|record| {
            let share = shares_by_album.get(&record.id)?;
            let viewpoint = Viewpoint {
                access: share.permission().access(),
                shared_by: Some(owner_of(share)),
            };
            Some((record, viewpoint))
        })
        .collect();
    Ok(dtos(state, &records)
        .await?
        .into_iter()
        .map(|album| (album.id, album))
        .collect())
}
