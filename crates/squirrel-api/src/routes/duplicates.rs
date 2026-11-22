//! The caller's duplicates: groups of near-identical photos, keeping one
//! (the others are hidden everywhere, never deleted) and restoring hidden
//! ones. Browser session only (the phone only stops listing hidden photos).
//! Another user's group is `not_found`.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use squirrel_db::duplicates::review;
use uuid::Uuid;

use crate::cursor;
use crate::duplicates_dto::{
    DuplicateGroupDto, DuplicatesPage, DuplicatesQuery, KeepDuplicateRequest,
    RestoreDuplicatesRequest, group_page_size,
};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiJson, ApiPath, ApiQuery, known_params};
use crate::routes::people::edited;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/duplicates", security(("session_cookie" = [])), tag = "duplicates",
    params(DuplicatesQuery),
    responses((status = 200, body = DuplicatesPage), (status = 400, body = ErrorBody, description = "Invalid cursor"),
              (status = 422, body = ErrorBody)))]
pub async fn list_duplicates(
    State(state): State<AppState>,
    current: CurrentUser,
    RawQuery(raw): RawQuery,
    ApiQuery(query): ApiQuery<DuplicatesQuery>,
) -> Result<Json<DuplicatesPage>, ApiError> {
    known_params(raw.as_deref(), &["state", "cursor", "limit"])?;
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?;
    let page = review::list(
        &state.pool,
        current.user.id,
        query.state.unwrap_or_default().into(),
        after.as_ref(),
        group_page_size(query.limit),
    )
    .await?;
    let open_count = review::open_count(&state.pool, current.user.id).await?;
    let next_cursor = page
        .items
        .last()
        .filter(|_| page.has_more)
        .map(|last| cursor::encode_timeline(&last.group.cursor()));
    Ok(Json(DuplicatesPage {
        items: page.items.iter().map(DuplicateGroupDto::from).collect(),
        open_count,
        next_cursor,
    }))
}

#[utoipa::path(post, path = "/api/duplicates/{id}/keep", security(("session_cookie" = [], "csrf_token" = [])), tag = "duplicates",
    params(("id" = Uuid, Path, description = "Group id")),
    request_body = KeepDuplicateRequest,
    responses((status = 200, body = DuplicateGroupDto, description = "Resolved: every other photo is hidden"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody, description = "Not a photo of this group")))]
pub async fn keep_duplicate(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<KeepDuplicateRequest>,
) -> Result<Json<DuplicateGroupDto>, ApiError> {
    let edit = review::keep(&state.pool, current.user.id, id, request.asset).await?;
    let group = edited(edit, "asset is not a photo of this group")?;
    tracing::info!(user_id = %current.user.id, group_id = %id, kept = %request.asset,
        "duplicate kept");
    Ok(Json(DuplicateGroupDto::from(&group)))
}

#[utoipa::path(post, path = "/api/duplicates/{id}/restore", security(("session_cookie" = [], "csrf_token" = [])), tag = "duplicates",
    params(("id" = Uuid, Path, description = "Group id")),
    request_body = RestoreDuplicatesRequest,
    responses((status = 200, body = DuplicateGroupDto, description = "Shown again; open again once nothing is hidden"),
              (status = 204, description = "Shown again; the group had one photo left and is gone"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody, description = "Not a photo of this group")))]
pub async fn restore_duplicates(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<RestoreDuplicatesRequest>,
) -> Result<Response, ApiError> {
    let edit = review::restore(&state.pool, current.user.id, id, request.asset).await?;
    let group = edited(edit, "asset is not a photo of this group")?;
    tracing::info!(user_id = %current.user.id, group_id = %id, "duplicates restored");
    Ok(match group {
        Some(group) => Json(DuplicateGroupDto::from(&group)).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}
