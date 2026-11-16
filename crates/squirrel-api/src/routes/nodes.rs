//! Browsing and editing the file tree.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::NodeName;
use std::collections::HashMap;

use squirrel_db::media::{self, ThumbRef};
use squirrel_db::nodes::{self, ListCursor, NodeChange, NodeRecord};
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::{
    CreateFolderRequest, ListNodesQuery, NodeDetails, NodeDto, NodeListing, PathSegmentDto,
    UpdateNodeRequest,
};
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

#[utoipa::path(get, path = "/api/nodes", tag = "files", params(ListNodesQuery),
    responses((status = 200, body = NodeListing), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)))]
pub async fn list_nodes(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(query): ApiQuery<ListNodesQuery>,
) -> Result<Json<NodeListing>, ApiError> {
    let owner_id = current.user.id;
    // `list_children` answers an empty page for a foreign or trashed parent.
    if let Some(parent) = query.parent_id {
        nodes::find_live_folder(&state.pool, owner_id, parent)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
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

#[utoipa::path(get, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 200, body = NodeDetails), (status = 404, body = ErrorBody)))]
pub async fn get_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<NodeDetails>, ApiError> {
    let owner_id = current.user.id;
    let node = nodes::find_live(&state.pool, owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let path = nodes::path_to(&state.pool, owner_id, id).await?;
    let thumbnails = thumbnails_of(&state, std::slice::from_ref(&node)).await?;
    Ok(Json(NodeDetails {
        node: NodeDto::with_thumbnails(&node, &thumbnails),
        path: path
            .into_iter()
            .map(|s| PathSegmentDto {
                id: s.id,
                name: s.name,
            })
            .collect(),
    }))
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

#[utoipa::path(post, path = "/api/folders", tag = "files", request_body = CreateFolderRequest,
    responses((status = 201, body = NodeDto), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_folder(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(request): ApiJson<CreateFolderRequest>,
) -> Result<(StatusCode, Json<NodeDto>), ApiError> {
    let name = NodeName::parse(&request.name)?;
    let node = nodes::create_folder(&state.pool, current.user.id, request.parent_id, &name).await?;
    Ok((StatusCode::CREATED, Json((&node).into())))
}

#[utoipa::path(patch, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    request_body = UpdateNodeRequest,
    responses((status = 200, body = NodeDto), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn update_node(
    State(state): State<AppState>,
    current: CurrentUser,
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
    let node = nodes::update(&state.pool, current.user.id, id, &change).await?;
    Ok(Json((&node).into()))
}

#[utoipa::path(delete, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 204, description = "Moved to the trash with everything inside it"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn trash_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    squirrel_db::trash::trash(&state.pool, current.user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
