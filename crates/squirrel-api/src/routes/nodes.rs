//! Browsing and editing the file tree.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::NodeName;
use std::collections::HashMap;

use squirrel_db::access::NodeAccess;
use squirrel_db::media::{self, ThumbRef};
use squirrel_db::nodes::{self, ListCursor, NodeChange, NodeRecord, PathSegment};
use squirrel_db::users;
use uuid::Uuid;

use crate::access;
use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::{Caller, CurrentUser};
use crate::files_dto::{
    CreateFolderRequest, ListNodesQuery, NodeDetails, NodeDto, NodeListing, PathSegmentDto,
    UpdateNodeRequest,
};
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::sharing_dto::PersonDto;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/nodes", security(("device_bearer" = []), ("session_cookie" = [])), tag = "files", params(ListNodesQuery),
    responses((status = 200, body = NodeListing), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)))]
pub async fn list_nodes(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<ListNodesQuery>,
) -> Result<Json<NodeListing>, ApiError> {
    // Without a parent: the caller's own top-level folders. With one: any
    // folder the caller may see, listed in its owner's tree.
    let owner_id = match query.parent_id {
        Some(parent) => {
            access::folder(&state, caller.user.id, parent)
                .await?
                .node
                .owner_id
        }
        None => caller.user.id,
    };
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_folder)
        .transpose()?;
    let page = nodes::list_children(
        &state.pool,
        owner_id,
        query.parent_id,
        after.as_ref(),
        cursor::page_size(query.limit),
    )
    .await?;
    let next_cursor = page.items.last().filter(|_| page.has_more).map(|last| {
        cursor::encode_folder(&ListCursor {
            kind: last.kind(),
            name: last.name.clone(),
        })
    });
    let thumbnails = thumbnails_of(&state, &page.items).await?;
    Ok(Json(NodeListing {
        items: page
            .items
            .iter()
            .map(|node| NodeDto::with_thumbnails(node, &thumbnails))
            .collect(),
        next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/nodes/{id}", security(("device_bearer" = []), ("session_cookie" = [])), tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 200, body = NodeDetails), (status = 404, body = ErrorBody)))]
pub async fn get_node(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<NodeDetails>, ApiError> {
    let found = access::node(&state, caller.user.id, id).await?;
    let node = &found.node;
    let path = visible_path(
        nodes::path_to(&state.pool, node.owner_id, id).await?,
        &found,
    );
    let shared_by = if found.access.is_owner() {
        None
    } else {
        users::find_by_id(&state.pool, node.owner_id)
            .await?
            .as_ref()
            .map(PersonDto::from)
    };
    let thumbnails = thumbnails_of(&state, std::slice::from_ref(node)).await?;
    Ok(Json(NodeDetails {
        node: NodeDto::with_thumbnails(node, &thumbnails),
        path: path
            .into_iter()
            .map(|s| PathSegmentDto {
                id: s.id,
                name: s.name,
            })
            .collect(),
        access: found.access.into(),
        shared_by,
    }))
}

/// The part of the owner's path the caller may see: all of it for the owner,
/// from the shared folder down for a share, nothing for an album photo.
fn visible_path(path: Vec<PathSegment>, found: &NodeAccess) -> Vec<PathSegment> {
    if found.access.is_owner() {
        return path;
    }
    match found.share_root {
        Some(root) => path
            .into_iter()
            .skip_while(|segment| segment.id != root)
            .collect(),
        None => Vec::new(),
    }
}

/// Ready thumbnails of `nodes` (the caller's own), by blob hash: one query per page.
async fn thumbnails_of(
    state: &AppState,
    nodes: &[NodeRecord],
) -> Result<HashMap<String, ThumbRef>, ApiError> {
    let hashes: Vec<String> = nodes
        .iter()
        .filter_map(|node| node.blob_sha256.clone())
        .collect();
    Ok(media::thumbnails_for(&state.pool, &hashes).await?)
}

#[utoipa::path(post, path = "/api/folders", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "files", request_body = CreateFolderRequest,
    responses((status = 201, body = NodeDto), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_folder(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreateFolderRequest>,
) -> Result<(StatusCode, Json<NodeDto>), ApiError> {
    let name = NodeName::parse(&request.name)?;
    let parent = access::writable_folder(&state, caller.user.id, request.parent_id).await?;
    let node =
        nodes::create_folder(&state.pool, parent.node.owner_id, request.parent_id, &name).await?;
    Ok((StatusCode::CREATED, Json((&node).into())))
}

#[utoipa::path(patch, path = "/api/nodes/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    request_body = UpdateNodeRequest,
    responses((status = 200, body = NodeDto), (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn update_node(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<UpdateNodeRequest>,
) -> Result<Json<NodeDto>, ApiError> {
    if request.name.is_none() && request.parent_id.is_none() {
        return Err(ApiError::Validation(
            "give a new name, a new parent_id, or both".into(),
        ));
    }
    let change = NodeChange {
        name: request.name.as_deref().map(NodeName::parse).transpose()?,
        parent_id: request.parent_id,
    };
    let found = access::changeable(&state, caller.user.id, id).await?;
    let owner_id = found.node.owner_id;
    if let Some(target) = request
        .parent_id
        .filter(|&t| Some(t) != found.node.parent_id)
    {
        // Moves stay inside the owner's tree, into a folder the caller may write to.
        let target = access::writable_folder(&state, caller.user.id, target).await?;
        if target.node.owner_id != owner_id {
            return Err(ApiError::NotFound);
        }
    }
    let node = nodes::update(&state.pool, owner_id, id, &change).await?;
    Ok(Json((&node).into()))
}

#[utoipa::path(delete, path = "/api/nodes/{id}", security(("session_cookie" = [], "csrf_token" = [])), tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 204, description = "Moved to the trash with everything inside it"),
              (status = 403, body = ErrorBody, description = "Shared with you for reading only"), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn trash_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Trashed items go to their owner's trash, whoever trashes them.
    let found = access::changeable(&state, current.user.id, id).await?;
    squirrel_db::trash::trash(&state.pool, found.node.owner_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
