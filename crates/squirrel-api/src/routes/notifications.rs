//! The notification inbox: shares made to the caller and comments on what
//! they can see. Listing re-checks access, so a notification about something
//! no longer visible is dropped instead of shown.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_db::notifications::{self, NotificationCursor, NotificationRecord};
use squirrel_db::shares::Subject;
use squirrel_db::{DbError, access};
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::input::{ApiJson, ApiQuery};
use crate::notifications_dto::{
    MarkReadRequest, NotificationDto, NotificationPage, NotificationsQuery, UnreadCountDto,
};
use crate::state::AppState;

const DEFAULT_NOTIFICATIONS: u32 = 30;
const MAX_NOTIFICATIONS: u32 = 100;
/// Ids marked read in one request at most.
const MAX_MARK_READ: usize = 500;

/// Announces share `share_id` to its recipient. Never fails the request.
pub(crate) async fn announce_share(state: &AppState, share_id: Uuid) {
    if let Err(error) = notifications::for_share(&state.pool, share_id).await {
        tracing::warn!(%share_id, %error, "share notification not recorded");
    }
}

/// Announces comment `comment_id` to everyone who sees its subject. Never
/// fails the request.
pub(crate) async fn announce_comment(state: &AppState, comment_id: Uuid) {
    match notifications::for_comment(&state.pool, comment_id).await {
        Ok(count) => tracing::debug!(%comment_id, count, "comment announced"),
        Err(error) => tracing::warn!(%comment_id, %error, "comment notification not recorded"),
    }
}

/// Whether `user` still sees the subject; for a file, the folder holding it
/// when they may open that folder too.
async fn visibility(
    state: &AppState,
    user: Uuid,
    record: &NotificationRecord,
) -> Result<Option<Option<Uuid>>, DbError> {
    Ok(match record.subject() {
        Subject::Album(id) => access::album(&state.pool, user, id).await?.map(|_| None),
        Subject::Node(id) => access::node(&state.pool, user, id).await?.map(|found| {
            let parent_visible = found.access.is_owner()
                || found.share_root.is_some_and(|root| root != found.node.id);
            found.node.parent_id.filter(|_| parent_visible)
        }),
    })
}

#[utoipa::path(get, path = "/api/notifications", security(("device_bearer" = []), ("session_cookie" = [])), tag = "notifications",
    params(NotificationsQuery),
    responses((status = 200, body = NotificationPage), (status = 400, body = ErrorBody)))]
pub async fn list_notifications(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<NotificationsQuery>,
) -> Result<Json<NotificationPage>, ApiError> {
    let user = caller.user.id;
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_notification)
        .transpose()?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_NOTIFICATIONS)
        .clamp(1, MAX_NOTIFICATIONS);
    let page = notifications::list(&state.pool, user, after, limit).await?;
    let mut items = Vec::with_capacity(page.items.len());
    let mut gone = Vec::new();
    for record in &page.items {
        match visibility(&state, user, record).await? {
            Some(folder_id) => items.push(NotificationDto::new(record, folder_id)),
            None => gone.push(record.id),
        }
    }
    notifications::delete(&state.pool, user, &gone).await?;
    // The cursor follows the page as read, dropped items included.
    let next_cursor = page.items.last().filter(|_| page.has_more).map(|last| {
        cursor::encode_notification(&NotificationCursor {
            created_at: last.created_at,
            id: last.id,
        })
    });
    Ok(Json(NotificationPage { items, next_cursor }))
}

#[utoipa::path(get, path = "/api/notifications/unread-count", security(("device_bearer" = []), ("session_cookie" = [])), tag = "notifications",
    responses((status = 200, body = UnreadCountDto)))]
pub async fn unread_count(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<UnreadCountDto>, ApiError> {
    let unread = notifications::unread_count(&state.pool, caller.user.id).await?;
    Ok(Json(UnreadCountDto { unread }))
}

#[utoipa::path(post, path = "/api/notifications/read", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "notifications",
    request_body = MarkReadRequest,
    responses((status = 200, body = UnreadCountDto, description = "Marked read; the unread count left"),
              (status = 422, body = ErrorBody)))]
pub async fn mark_read(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<MarkReadRequest>,
) -> Result<(StatusCode, Json<UnreadCountDto>), ApiError> {
    if request
        .ids
        .as_ref()
        .is_some_and(|ids| ids.len() > MAX_MARK_READ)
    {
        return Err(ApiError::Validation(format!(
            "mark at most {MAX_MARK_READ} notifications at once"
        )));
    }
    let user = caller.user.id;
    notifications::mark_read(&state.pool, user, request.ids.as_deref()).await?;
    let unread = notifications::unread_count(&state.pool, user).await?;
    Ok((StatusCode::OK, Json(UnreadCountDto { unread })))
}
