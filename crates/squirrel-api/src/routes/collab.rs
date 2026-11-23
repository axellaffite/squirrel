//! Live co-editing of text files: opening a file's shared document, and the
//! WebSocket carrying the y-sync protocol (see [`crate::collab`]).
//! Browser sessions only (the phone app has no editor).

use axum::Json;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use squirrel_auth::hash_token;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::access;
use crate::collab::{self, CloseReason, socket};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiPath, ApiQuery};
use crate::session::SESSION_COOKIE;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct CollabSession {
    /// Pass it when connecting; a connection with an older epoch is closed
    /// with 4409 (the file changed outside the editor: open it again).
    pub epoch: Uuid,
    /// `false`: the document is shown live but cannot be changed.
    pub can_write: bool,
    /// Largest text the document may hold, in bytes of UTF-8.
    pub max_bytes: u64,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct CollabSocketQuery {
    pub epoch: Uuid,
}

#[utoipa::path(post, path = "/api/files/{id}/collab", security(("session_cookie" = [], "csrf_token" = [])), tag = "collab",
    params(("id" = Uuid, Path, description = "File id")),
    responses(
        (status = 200, body = CollabSession),
        (status = 404, body = ErrorBody),
        (status = 422, body = ErrorBody, description = "not_editable: not UTF-8 text, or too large"),
    ))]
pub async fn open_collab(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<CollabSession>, ApiError> {
    let found = access::file(&state, current.user.id, id).await?;
    let doc = collab::open_doc(&state, &found.node).await?;
    Ok(Json(CollabSession {
        epoch: doc.epoch,
        can_write: found.access.can_write(),
        max_bytes: u64::try_from(state.config.collab.max_bytes).unwrap_or(u64::MAX),
    }))
}

/// Upgrades to the co-editing WebSocket (y-websocket protocol). Refusals after
/// the upgrade are close codes: 4401 signed out, 4404 no access, 4409 stale
/// epoch, 4413 too large, 4429 busy.
#[utoipa::path(get, path = "/api/files/{id}/collab/ws", security(("session_cookie" = [])), tag = "collab",
    params(("id" = Uuid, Path, description = "File id"), CollabSocketQuery),
    responses(
        (status = 101, description = "Switching to the co-editing WebSocket"),
        (status = 403, body = ErrorBody, description = "cross-site request (Origin does not match Host)"),
    ))]
pub async fn collab_socket(
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
    headers: HeaderMap,
    jar: CookieJar,
    current: Result<CurrentUser, ApiError>,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<CollabSocketQuery>,
) -> Result<Response, ApiError> {
    // Browsers send cookies on cross-site WebSocket upgrades: the page must
    // come from this very server.
    if !same_origin(&headers) {
        return Err(ApiError::Forbidden);
    }
    let upgrade = upgrade.max_message_size(state.config.collab.max_bytes * 2 + 64 * 1024);
    let connection = match connection(&state, current, &jar, id, query.epoch).await? {
        Ok(connection) => connection,
        Err(reason) => {
            return Ok(
                upgrade.on_upgrade(move |ws| socket::close(ws, reason.code(), reason.text()))
            );
        }
    };
    Ok(upgrade.on_upgrade(move |ws| socket::run(state, ws, connection)))
}

/// Who connects and to what, or the close code refusing them.
async fn connection(
    state: &AppState,
    current: Result<CurrentUser, ApiError>,
    jar: &CookieJar,
    id: Uuid,
    epoch: Uuid,
) -> Result<Result<socket::Connection, CloseReason>, ApiError> {
    let (Ok(current), Some(cookie)) = (current, jar.get(SESSION_COOKIE)) else {
        return Ok(Err(CloseReason::SignedOut));
    };
    let found = match access::file(state, current.user.id, id).await {
        Ok(found) => found,
        Err(ApiError::NotFound) => return Ok(Err(CloseReason::Gone)),
        Err(err) => return Err(err),
    };
    Ok(Ok(socket::Connection {
        user_id: current.user.id,
        session_hash: hash_token(cookie.value()),
        can_write: found.access.can_write(),
        node: found.node,
        epoch,
    }))
}

/// Whether `Origin` names the host the request was sent to.
fn same_origin(headers: &HeaderMap) -> bool {
    let origin = headers.get(ORIGIN).and_then(|value| value.to_str().ok());
    let host = headers.get(HOST).and_then(|value| value.to_str().ok());
    let (Some(origin), Some(host)) = (origin, host) else {
        return false;
    };
    origin
        .parse::<Uri>()
        .ok()
        .and_then(|uri| {
            uri.authority()
                .map(|authority| authority.as_str().to_owned())
        })
        .is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(origin: Option<&'static str>, host: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static(host));
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_static(origin));
        }
        headers
    }

    #[test]
    fn only_the_server_own_origin_is_accepted() {
        assert!(same_origin(&headers(
            Some("https://nuc.local:8443"),
            "nuc.local:8443"
        )));
        assert!(same_origin(&headers(
            Some("http://LOCALHOST:8080"),
            "localhost:8080"
        )));
        assert!(!same_origin(&headers(
            Some("https://evil.example"),
            "nuc.local:8443"
        )));
        assert!(!same_origin(&headers(
            Some("https://nuc.local"),
            "nuc.local:8443"
        )));
        assert!(!same_origin(&headers(Some("null"), "nuc.local")));
        assert!(!same_origin(&headers(None, "nuc.local")));
    }
}
