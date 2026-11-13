//! File downloads and inline previews.

use std::io::SeekFrom;

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH, RANGE, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use squirrel_core::{ContentHash, NodeKind};
use squirrel_db::nodes::{self, NodeRecord};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::content::{
    ContentPolicy, RangeRequest, content_disposition, entity_tag, etag_matches, resolve_range,
};
use crate::downloads::HeldReader;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiPath, ApiQuery};
use crate::state::AppState;
use crate::upload_runtime::Slot;

/// Browsers may keep a copy but must revalidate it (auth + ETag) every time.
const CACHE_POLICY: &str = "private, no-cache";
const STREAM_BUFFER: usize = 64 * 1024;
const CROSS_ORIGIN_RESOURCE_POLICY: HeaderName =
    HeaderName::from_static("cross-origin-resource-policy");

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ContentQuery {
    /// `true` forces `Content-Disposition: attachment`.
    #[serde(default)]
    pub download: bool,
}

/// What to send once the node and its blob are known.
struct Selection {
    status: StatusCode,
    start: u64,
    len: u64,
    content_range: Option<String>,
}

#[utoipa::path(get, path = "/api/files/{id}/content", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), ContentQuery),
    responses(
        (status = 200, body = Vec<u8>, description = "The file; active content types are served as application/octet-stream"),
        (status = 206, body = Vec<u8>, description = "One byte range"),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 416, body = ErrorBody),
        (status = 503, body = ErrorBody, description = "Too many downloads in progress"),
    ))]
pub async fn file_content(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let owner_id = current.user.id;
    let node = nodes::find_live(&state.pool, owner_id, id)
        .await?
        .filter(|node| node.kind() == NodeKind::File)
        .ok_or(ApiError::NotFound)?;
    let hash = blob_hash(&node)?;
    let policy = ContentPolicy::for_mime(node.mime.as_deref(), query.download);
    let disposition = content_disposition(policy.inline, &node.name);
    let etag = entity_tag(&hash, &policy, &disposition);
    // A 304 carries the same policy headers as the 200: browsers merge them
    // into the cached response, and the global layers would add `DENY`.
    let mut response_headers = policy_headers(&policy, disposition, etag.clone())?;
    if etag_matches(header_str(&headers, IF_NONE_MATCH), &etag) {
        return Ok((StatusCode::NOT_MODIFIED, response_headers).into_response());
    }

    let slot = state
        .downloads
        .acquire(owner_id)
        .await
        .ok_or(ApiError::Busy)?;
    let (store, blob) = (state.storage.clone(), hash.clone());
    let (file, size) = run_storage(move || store.open_blob(&blob))
        .await
        .inspect_err(|err| {
            if matches!(err, ApiError::NotFound) {
                tracing::error!(node_id = %id, %hash, "blob missing from the data directory");
            }
        })?;
    let Some(selection) = select(header_str(&headers, RANGE), size) else {
        let headers = [(CONTENT_RANGE, format!("bytes */{size}"))];
        return Ok((headers, ApiError::RangeNotSatisfiable).into_response());
    };
    response_headers.extend(body_headers(&policy, &selection)?);
    let body = stream(file, &selection, slot).await?;
    Ok((selection.status, response_headers, body).into_response())
}

/// `None`: the range is unsatisfiable.
fn select(range: Option<&str>, size: u64) -> Option<Selection> {
    match resolve_range(range, size) {
        RangeRequest::Full => Some(Selection {
            status: StatusCode::OK,
            start: 0,
            len: size,
            content_range: None,
        }),
        RangeRequest::Partial(range) => {
            let (start, end) = (*range.start(), *range.end());
            Some(Selection {
                status: StatusCode::PARTIAL_CONTENT,
                start,
                len: end - start + 1,
                content_range: Some(format!("bytes {start}-{end}/{size}")),
            })
        }
        RangeRequest::Unsatisfiable => None,
    }
}

fn blob_hash(node: &NodeRecord) -> Result<ContentHash, ApiError> {
    node.blob_sha256
        .as_deref()
        .and_then(|sha| ContentHash::parse(sha).ok())
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "file node {} has no valid blob hash",
                node.id
            ))
        })
}

fn header_str(headers: &HeaderMap, name: HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Headers shared by 200, 206 and 304. The handler sets CSP and
/// `X-Frame-Options` itself; the global layers only add theirs when absent, so
/// these (frameable by the app, sandboxed) win.
fn policy_headers(
    policy: &ContentPolicy,
    disposition: String,
    etag: String,
) -> Result<HeaderMap, ApiError> {
    to_header_map(vec![
        (CONTENT_DISPOSITION, disposition),
        (CONTENT_SECURITY_POLICY, policy.csp.to_owned()),
        (ETAG, etag),
        (CACHE_CONTROL, CACHE_POLICY.to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (X_FRAME_OPTIONS, "SAMEORIGIN".to_owned()),
        (CROSS_ORIGIN_RESOURCE_POLICY, "same-origin".to_owned()),
    ])
}

/// Headers describing the bytes actually sent.
fn body_headers(policy: &ContentPolicy, selection: &Selection) -> Result<HeaderMap, ApiError> {
    let mut pairs = vec![
        (CONTENT_TYPE, policy.content_type.clone()),
        (CONTENT_LENGTH, selection.len.to_string()),
        (ACCEPT_RANGES, "bytes".to_owned()),
    ];
    if let Some(range) = &selection.content_range {
        pairs.push((CONTENT_RANGE, range.clone()));
    }
    to_header_map(pairs)
}

fn to_header_map(pairs: Vec<(HeaderName, String)>) -> Result<HeaderMap, ApiError> {
    pairs
        .into_iter()
        .map(|(name, value)| HeaderValue::try_from(value).map(|value| (name, value)))
        .collect::<Result<HeaderMap, _>>()
        .map_err(|err| ApiError::Internal(err.into()))
}

/// The download slot travels with the body and is released when it finishes
/// streaming or the client disconnects (the body is dropped).
async fn stream(file: std::fs::File, selection: &Selection, slot: Slot) -> Result<Body, ApiError> {
    let mut file = tokio::fs::File::from_std(file);
    file.seek(SeekFrom::Start(selection.start))
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;
    let reader = HeldReader::new(file.take(selection.len), slot);
    Ok(Body::from_stream(ReaderStream::with_capacity(
        reader,
        STREAM_BUFFER,
    )))
}
