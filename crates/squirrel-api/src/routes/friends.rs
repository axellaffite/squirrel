//! Friends: the caller's one-way list of accounts to share with quickly.
//! Adding or removing someone tells them nothing.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::Username;
use squirrel_db::users::{self, UserRecord};
use squirrel_db::{DbError, friends};

use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::friends_dto::{AddFriendRequest, FriendDto, FriendsPage};
use crate::input::{ApiJson, ApiPath};
use crate::sharing_dto::MemberDto;
use crate::state::AppState;

/// The live account named `username`, if any.
async fn live_account(state: &AppState, username: &str) -> Result<Option<UserRecord>, ApiError> {
    let Ok(username) = Username::parse(username.trim()) else {
        return Ok(None);
    };
    Ok(users::find_by_username(&state.pool, &username)
        .await?
        .filter(|user| user.disabled_at.is_none()))
}

#[utoipa::path(get, path = "/api/friends", security(("device_bearer" = []), ("session_cookie" = [])), tag = "friends",
    responses((status = 200, body = FriendsPage)))]
pub async fn list_friends(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<FriendsPage>, ApiError> {
    let list = friends::list(&state.pool, caller.user.id).await?;
    let suggested = friends::suggestions(&state.pool, caller.user.id).await?;
    Ok(Json(FriendsPage {
        friends: list.iter().map(FriendDto::from).collect(),
        suggestions: suggested.iter().map(MemberDto::from).collect(),
    }))
}

#[utoipa::path(post, path = "/api/friends", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "friends",
    request_body = AddFriendRequest,
    responses(
        (status = 201, body = FriendDto, description = "Added, or already a friend"),
        (status = 422, body = ErrorBody, description = "No such account, yourself, or 500 friends already"),
    ))]
pub async fn add_friend(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<AddFriendRequest>,
) -> Result<(StatusCode, Json<FriendDto>), ApiError> {
    let friend = live_account(&state, &request.username)
        .await?
        .ok_or_else(|| ApiError::Validation("there is no account with this username".into()))?;
    if friend.id == caller.user.id {
        return Err(ApiError::Validation("that is you: add someone else".into()));
    }
    let added = friends::add(&state.pool, caller.user.id, friend.id)
        .await
        .map_err(|err| match err {
            DbError::InvalidInput(message) => ApiError::Validation(message.into()),
            other => other.into(),
        })?;
    tracing::info!(user_id = %caller.user.id, friend_id = %friend.id, "friend added");
    Ok((StatusCode::CREATED, Json(FriendDto::from(&added))))
}

#[utoipa::path(delete, path = "/api/friends/{username}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "friends",
    params(("username" = String, Path, description = "The friend's username")),
    responses(
        (status = 204, description = "No longer a friend"),
        (status = 404, body = ErrorBody, description = "Not a friend"),
    ))]
pub async fn remove_friend(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(username): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    let friend = live_account(&state, &username)
        .await?
        .ok_or(ApiError::NotFound)?;
    if friends::remove(&state.pool, caller.user.id, friend.id).await? {
        tracing::info!(user_id = %caller.user.id, friend_id = %friend.id, "friend removed");
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
