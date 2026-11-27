//! `GET /api/files/{id}/thumbnail`: the worker's stored variants of a file
//! (grid thumbnails, PDF previews) and photo previews rendered on request
//! (see [`super::preview_render`]).

use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media::{self, MediaRecord, MediaState};
use squirrel_db::nodes::NodeRecord;
use squirrel_storage::Variant;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::preview_render;
use crate::access;
use crate::blocking::run_storage;
use crate::content::{
    CROSS_ORIGIN_RESOURCE_POLICY, SANDBOX_CSP, blob_hash, etag_matches, header_str, to_header_map,
};
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::holder::Holder;
use crate::input::{ApiPath, ApiQuery};
use crate::state::AppState;

/// A URL naming the current version always maps to the same bytes: cache for
/// a year without revalidating (logout clears the cache, see `routes::auth`).
const CACHE_IMMUTABLE: &str = "private, max-age=31536000, immutable";
/// Any other URL (no `v`, or an older one) may change: revalidate by ETag.
const CACHE_REVALIDATE: &str = "private, no-cache";

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ThumbnailQuery {
    /// `thumb` (grid, 360 px tall) or `preview` (lightbox).
    #[param(inline)]
    pub size: ThumbnailSize,
    /// The `thumbnail.version` from the listing. Only a thumbnail URL with
    /// the current version is cached as immutable.
    pub v: Option<i16>,
    /// `preview` of a photo only: the long side wanted, in device pixels. Rounded
    /// up to 1440, 2160 or 3200 (capped), never above the original's size.
    /// Without it: 1440. PDF previews are always 1440.
    #[param(minimum = 1)]
    pub px: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    Thumb,
    Preview,
}

impl ThumbnailSize {
    fn variant(self) -> Variant {
        match self {
            Self::Thumb => Variant::Thumb,
            Self::Preview => Variant::Preview,
        }
    }
}

fn cache_policy(requested: Option<i16>, current: i16) -> &'static str {
    if requested == Some(current) {
        CACHE_IMMUTABLE
    } else {
        CACHE_REVALIDATE
    }
}

/// The headers every answer of this route carries (a 304 too, see `routes::content`).
pub(super) fn policy_headers(cache_control: &str, etag: &str) -> Result<HeaderMap, ApiError> {
    let pairs: Vec<(HeaderName, String)> = vec![
        (CACHE_CONTROL, cache_control.to_owned()),
        (ETAG, etag.to_owned()),
        (CONTENT_SECURITY_POLICY, SANDBOX_CSP.to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (X_FRAME_OPTIONS, "SAMEORIGIN".to_owned()),
        (CROSS_ORIGIN_RESOURCE_POLICY, "same-origin".to_owned()),
    ];
    to_header_map(pairs)
}

/// `200` with `bytes`, or `304` when the client already has `etag`.
pub(super) fn image_response(
    request: &HeaderMap,
    mut headers: HeaderMap,
    etag: &str,
    content_type: &'static str,
    bytes: Vec<u8>,
) -> Response {
    if etag_matches(header_str(request, IF_NONE_MATCH), etag) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    (StatusCode::OK, headers, bytes).into_response()
}

#[utoipa::path(get, path = "/api/files/{id}/thumbnail", security(("device_bearer" = []), ("session_cookie" = [])), tag = "files",
    params(("id" = Uuid, Path, description = "File id"), ThumbnailQuery),
    responses(
        (status = 200, description = "WebP, or JPEG for an opaque photo preview", content(
            (Vec<u8> = "image/webp"),
            (Vec<u8> = "image/jpeg"),
        )),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "not_found, or not_ready while the worker has not produced it"),
        (status = 429, body = ErrorBody, description = "rate_limited: too many previews rendered; retry after `Retry-After`"),
        (status = 503, body = ErrorBody, description = "busy: every preview renderer is in use; retry after `Retry-After`"),
    ))]
pub async fn thumbnail(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Access first: a node the caller may not see is exactly `not_found`, even
    // when the same bytes (and so the same derived files) exist.
    check_px(&query)?;
    let node = access::file(&state, caller.user.id, id).await?.node;
    serve(
        &state,
        &node,
        Holder::User(caller.user.id),
        &query,
        &headers,
    )
    .await
}

/// `px=0` is refused before anything is looked up.
pub(crate) fn check_px(query: &ThumbnailQuery) -> Result<(), ApiError> {
    if query.px == Some(0) {
        return Err(ApiError::BadRequest("px must be at least 1".into()));
    }
    Ok(())
}

/// Answers a thumbnail or preview request for file `node`, already checked
/// to be readable. Preview renders are counted against `holder`.
pub(crate) async fn serve(
    state: &AppState,
    node: &NodeRecord,
    holder: Holder,
    query: &ThumbnailQuery,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let id = node.id;
    let hash = blob_hash(node)?;
    let media = media::find(&state.pool, hash.as_str())
        .await?
        .ok_or(ApiError::NotFound)?;
    match media.thumbs_state() {
        MediaState::Ready => {}
        MediaState::Pending => return Err(ApiError::NotReady),
        MediaState::Failed | MediaState::Unsupported => return Err(ApiError::NotFound),
    }
    if matches!(query.size, ThumbnailSize::Preview) && media.kind() == MediaKind::Image {
        let request = preview_render::PreviewRequest {
            id,
            holder,
            hash,
            media: &media,
            px: query.px,
        };
        return preview_render::serve(state, request, headers).await;
    }
    stored(state, id, hash, &media, query, headers).await
}

/// A variant the worker stored: thumbnails, and PDF previews (1440, `px` ignored).
async fn stored(
    state: &AppState,
    id: Uuid,
    hash: ContentHash,
    media: &MediaRecord,
    query: &ThumbnailQuery,
    request: &HeaderMap,
) -> Result<Response, ApiError> {
    let variant = query.size.variant();
    let version = media.thumbs_version;
    let etag = format!("\"{}-{}-v{version}\"", hash.as_str(), variant.stem());
    let response_headers = policy_headers(cache_policy(query.v, version), &etag)?;
    if etag_matches(header_str(request, IF_NONE_MATCH), &etag) {
        return Ok((StatusCode::NOT_MODIFIED, response_headers).into_response());
    }
    // Small (≤ ~300 KB): read whole, without a download slot (a grid asks for dozens at once).
    let derived = state.storage.derived().clone();
    let bytes = run_storage(move || derived.read(&hash, variant, version))
        .await
        .map_err(|err| match err {
            // The row says ready but the file is gone (restored backup, manual
            // deletion): behave as not ready; regenerating it is the worker's job.
            ApiError::NotFound => {
                tracing::warn!(node_id = %id, "thumbnail recorded as ready but missing on disk");
                ApiError::NotReady
            }
            other => other,
        })?;
    Ok(image_response(
        request,
        response_headers,
        &etag,
        "image/webp",
        bytes,
    ))
}
