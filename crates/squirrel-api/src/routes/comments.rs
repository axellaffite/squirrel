//! Comment threads on folders, files and albums. Whoever can see the subject
//! reads and writes its thread; authors edit theirs; authors and the subject's
//! owner delete.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_db::comments::{self, MAX_BODY_CHARS};
use squirrel_db::shares::Subject;
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::routes::shares::subject_access;
use crate::sharing_dto::{
    CommentDto, CommentPage, CommentsQuery, CreateCommentRequest, EditCommentRequest,
};
use crate::state::AppState;

const DEFAULT_COMMENTS: u32 = 100;
const MAX_COMMENTS: u32 = 200;

/// A trimmed body of 1 to [`MAX_BODY_CHARS`] characters.
fn body(raw: &str) -> Result<&str, ApiError> {
    let text = raw.trim();
    let chars = text.chars().count();
    if chars == 0 || chars > MAX_BODY_CHARS {
        return Err(ApiError::Validation(format!(
            "a comment holds 1 to {MAX_BODY_CHARS} characters"
        )));
    }
    Ok(text)
}

/// The comment's subject, which the caller must still be able to see.
async fn visible_subject(state: &AppState, user: Uuid, id: Uuid) -> Result<Subject, ApiError> {
    let subject = comments::subject_of(&state.pool, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    subject_access(state, user, subject).await?;
    Ok(subject)
}

#[utoipa::path(get, path = "/api/comments", security(("device_bearer" = []), ("session_cookie" = [])), tag = "comments",
    params(CommentsQuery),
    responses((status = 200, body = CommentPage), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn list_comments(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<CommentsQuery>,
) -> Result<Json<CommentPage>, ApiError> {
    let subject = query.subject()?;
    subject_access(&state, caller.user.id, subject).await?;
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_comment)
        .transpose()?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_COMMENTS)
        .clamp(1, MAX_COMMENTS);
    let page = comments::list(&state.pool, subject, after, limit).await?;
    let next_cursor = page.items.last().filter(|_| page.has_more).map(|last| {
        cursor::encode_comment(&comments::CommentCursor {
            created_at: last.created_at,
            id: last.id,
        })
    });
    Ok(Json(CommentPage {
        items: page
            .items
            .iter()
            .map(|record| CommentDto::new(record, caller.user.id))
            .collect(),
        next_cursor,
    }))
}

#[utoipa::path(post, path = "/api/comments", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "comments",
    request_body = CreateCommentRequest,
    responses((status = 201, body = CommentDto), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn create_comment(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentDto>), ApiError> {
    let subject = request.subject.subject()?;
    let text = body(&request.body)?;
    let (owner_id, _) = subject_access(&state, caller.user.id, subject).await?;
    let record = comments::create(&state.pool, owner_id, subject, caller.user.id, text).await?;
    Ok((
        StatusCode::CREATED,
        Json(CommentDto::new(&record, caller.user.id)),
    ))
}

#[utoipa::path(patch, path = "/api/comments/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "comments",
    params(("id" = Uuid, Path, description = "Comment id")),
    request_body = EditCommentRequest,
    responses((status = 200, body = CommentDto),
              (status = 403, body = ErrorBody, description = "Only its author edits a comment"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn edit_comment(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<EditCommentRequest>,
) -> Result<Json<CommentDto>, ApiError> {
    let text = body(&request.body)?;
    visible_subject(&state, caller.user.id, id).await?;
    if !comments::edit(&state.pool, caller.user.id, id, text).await? {
        return Err(ApiError::Forbidden);
    }
    let record = comments::find(&state.pool, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(CommentDto::new(&record, caller.user.id)))
}

#[utoipa::path(delete, path = "/api/comments/{id}", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "comments",
    params(("id" = Uuid, Path, description = "Comment id")),
    responses((status = 204, description = "Deleted"),
              (status = 403, body = ErrorBody, description = "Only its author or the owner of what it is about"),
              (status = 404, body = ErrorBody)))]
pub async fn delete_comment(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    visible_subject(&state, caller.user.id, id).await?;
    if comments::delete(&state.pool, caller.user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_are_trimmed_and_bounded() {
        assert_eq!(body("  hi  ").unwrap(), "hi");
        assert!(body("   ").is_err());
        assert!(body(&"é".repeat(MAX_BODY_CHARS)).is_ok());
        assert!(body(&"é".repeat(MAX_BODY_CHARS + 1)).is_err());
    }
}
