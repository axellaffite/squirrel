//! The visitor's side of public links: `/api/public/{token}/…`, without an
//! account. Every node is reached through [`public_links::node_in_link`], so
//! nothing outside the link is ever served. An unknown, expired or revoked
//! token, a trashed item and a disabled owner are all `not_found`.

pub mod browse;
pub mod files;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use squirrel_auth::{hash_token, verify_password};
use squirrel_db::albums;
use squirrel_db::nodes::NodeRecord;
use squirrel_db::public_links::{self, ActiveLink, PublicLinkRecord};
use squirrel_db::shares::Subject;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_hashing_in;
use crate::error::{ApiError, ErrorBody};
use crate::files_dto::NodeDto;
use crate::input::{ApiJson, ApiPath};
use crate::public_links_dto::{PublicAlbumDto, PublicLinkInfo, PublicLinkKind, UnlockRequest};
use crate::public_unlock;
use crate::routes::nodes::thumbnails_of;
use crate::state::AppState;

/// Characters of a link token ([`squirrel_auth::generate_token`]).
const TOKEN_CHARS: usize = 43;

/// Whether `token` is shaped like a link token; checked before any hashing
/// or database lookup.
fn well_formed(token: &str) -> bool {
    token.len() == TOKEN_CHARS
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The active link of `token`, locked or not.
async fn find(state: &AppState, token: &str) -> Result<ActiveLink, ApiError> {
    if !well_formed(token) {
        return Err(ApiError::NotFound);
    }
    public_links::find_active(&state.pool, &hash_token(token))
        .await?
        .ok_or(ApiError::NotFound)
}

fn unlocked(state: &AppState, jar: &CookieJar, link: &PublicLinkRecord) -> bool {
    public_unlock::is_unlocked(
        jar,
        link,
        OffsetDateTime::now_utc(),
        state.config.public_unlock_ttl,
    )
}

/// The active link of `token`, which the visitor has unlocked (or which has
/// no password), else `not_found` / `link_locked`.
pub(crate) async fn open(
    state: &AppState,
    token: &str,
    jar: &CookieJar,
) -> Result<PublicLinkRecord, ApiError> {
    let found = find(state, token).await?;
    if unlocked(state, jar, &found.link) {
        Ok(found.link)
    } else {
        Err(ApiError::LinkLocked)
    }
}

/// The live node `id` inside `link`, else `not_found`.
pub(crate) async fn node(
    state: &AppState,
    link: &PublicLinkRecord,
    id: Uuid,
) -> Result<NodeRecord, ApiError> {
    public_links::node_in_link(&state.pool, link, id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// A file inside `link`, else `not_found`.
pub(crate) async fn file(
    state: &AppState,
    link: &PublicLinkRecord,
    id: Uuid,
) -> Result<NodeRecord, ApiError> {
    let found = node(state, link, id).await?;
    if found.is_folder() {
        return Err(ApiError::NotFound);
    }
    Ok(found)
}

/// `node` as a visitor sees it: nothing above the link's root.
pub(crate) async fn visible_dto(
    state: &AppState,
    link: &PublicLinkRecord,
    node: &NodeRecord,
) -> Result<NodeDto, ApiError> {
    let thumbnails = thumbnails_of(state, std::slice::from_ref(node)).await?;
    let mut dto = NodeDto::with_thumbnails(node, &thumbnails);
    if link.subject() == Subject::Node(node.id) {
        dto.parent_id = None;
        dto.is_root = false;
    }
    Ok(dto)
}

#[utoipa::path(get, path = "/api/public/{token}", security(()), tag = "public",
    params(("token" = String, Path, description = "The token from the link")),
    responses(
        (status = 200, body = PublicLinkInfo, description = "What the link opens, or `locked` until its password is given"),
        (status = 404, body = ErrorBody, description = "Unknown, expired or revoked link, or its item is gone"),
        (status = 429, body = ErrorBody),
    ))]
pub async fn public_link_info(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath(token): ApiPath<String>,
) -> Result<Json<PublicLinkInfo>, ApiError> {
    let found = find(&state, &token).await?;
    if !unlocked(&state, &jar, &found.link) {
        return Ok(Json(PublicLinkInfo::locked()));
    }
    let link = &found.link;
    let (kind, node, album) = match link.subject() {
        Subject::Node(id) => {
            let root = node(&state, link, id).await?;
            let kind = if root.is_folder() {
                PublicLinkKind::Folder
            } else {
                PublicLinkKind::File
            };
            (kind, Some(visible_dto(&state, link, &root).await?), None)
        }
        Subject::Album(id) => {
            let album = albums::find_any(&state.pool, id)
                .await?
                .ok_or(ApiError::NotFound)?;
            let dto = PublicAlbumDto {
                id: album.id,
                name: album.name,
                item_count: album.item_count,
                cover_id: album.cover_id,
            };
            (PublicLinkKind::Album, None, Some(dto))
        }
    };
    Ok(Json(PublicLinkInfo {
        locked: false,
        kind: Some(kind),
        shared_by: Some(found.owner_display_name),
        expires_at: link.expires_at,
        node,
        album,
    }))
}

#[utoipa::path(post, path = "/api/public/{token}/unlock", security(()), tag = "public",
    params(("token" = String, Path, description = "The token from the link")),
    request_body = UnlockRequest,
    responses(
        (status = 204, description = "Unlocked: a cookie scoped to this link is set (none is needed without a password)"),
        (status = 401, body = ErrorBody, description = "invalid_credentials: wrong password"),
        (status = 404, body = ErrorBody),
        (status = 429, body = ErrorBody),
        (status = 503, body = ErrorBody, description = "busy: no password hashing slot"),
    ))]
pub async fn unlock_public_link(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath(token): ApiPath<String>,
    ApiJson(request): ApiJson<UnlockRequest>,
) -> Result<(StatusCode, CookieJar), ApiError> {
    let found = find(&state, &token).await?;
    let link = found.link;
    // A link without a password is open anyway (its info says so).
    let Some(stored_hash) = link.password_hash.clone() else {
        return Ok((StatusCode::NO_CONTENT, jar));
    };
    let candidate = request.password;
    let permits = state.public_hash_permits.clone();
    let valid = run_hashing_in(&state, &permits, move || {
        verify_password(&candidate, &stored_hash)
    })
    .await??;
    if !valid {
        tracing::info!(link_id = %link.id, "wrong public link password");
        return Err(ApiError::InvalidCredentials);
    }
    let cookie = public_unlock::cookie(
        &token,
        &link,
        OffsetDateTime::now_utc(),
        state.config.public_unlock_ttl,
        state.config.cookie_secure,
    )
    .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("a locked link has a password")))?;
    Ok((StatusCode::NO_CONTENT, jar.add(cookie)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_43_url_safe_characters_make_a_token() {
        let token = squirrel_auth::generate_token();
        assert!(well_formed(&token));
        assert!(!well_formed(&token[1..]));
        assert!(!well_formed(&format!("{token}a")));
        assert!(!well_formed(&format!("{}/", &token[1..])));
        assert!(!well_formed(""));
    }
}
