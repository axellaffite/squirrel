//! `GET /api/files/{id}/thumbnail`: the worker's WebP variants of a file.

use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use squirrel_core::NodeKind;
use squirrel_db::media::{self, MediaState};
use squirrel_db::nodes;
use squirrel_storage::Variant;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::content::{
    CROSS_ORIGIN_RESOURCE_POLICY, SANDBOX_CSP, blob_hash, etag_matches, header_str, to_header_map,
};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
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
    /// `thumb` (grid, 360 px tall) or `preview` (lightbox, 1440 px).
    #[param(inline)]
    pub size: ThumbnailSize,
    /// The `thumbnail.version` from the listing. Only a URL with the current
    /// version is cached as immutable.
    pub v: Option<i16>,
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

#[utoipa::path(get, path = "/api/files/{id}/thumbnail", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), ThumbnailQuery),
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/webp"),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "not_found, or not_ready while the worker has not produced it"),
    ))]
pub async fn thumbnail(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // Owner-scoped first: another user's node is exactly `not_found`, even when
    // the same bytes (and so the same derived files) exist.
    let node = nodes::find_live(&state.pool, current.user.id, id)
        .await?
        .filter(|node| node.kind() == NodeKind::File)
        .ok_or(ApiError::NotFound)?;
    let hash = blob_hash(&node)?;
    let media = media::find(&state.pool, hash.as_str())
        .await?
        .ok_or(ApiError::NotFound)?;
    match media.thumbs_state() {
        MediaState::Ready => {}
        MediaState::Pending => return Err(ApiError::NotReady),
        MediaState::Failed | MediaState::Unsupported => return Err(ApiError::NotFound),
    }
    let variant = query.size.variant();
    let version = media.thumbs_version;
    let etag = format!("\"{}-{}-v{version}\"", hash.as_str(), variant.stem());
    // A 304 carries the same policy headers as the 200 (see `routes::content`).
    let mut response_headers = to_header_map(vec![
        (CACHE_CONTROL, cache_policy(query.v, version).to_owned()),
        (ETAG, etag.clone()),
        (CONTENT_SECURITY_POLICY, SANDBOX_CSP.to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (X_FRAME_OPTIONS, "SAMEORIGIN".to_owned()),
        (CROSS_ORIGIN_RESOURCE_POLICY, "same-origin".to_owned()),
    ])?;
    if etag_matches(header_str(&headers, IF_NONE_MATCH), &etag) {
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
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    Ok((StatusCode::OK, response_headers, bytes).into_response())
}
