//! Photo previews rendered on request: bucket from `px`, cache lookup (no
//! slot), else one render per key in the sandbox, behind the per-user render
//! limit (429) and the render slots (503), both with `Retry-After`.

use axum::http::header::{IF_NONE_MATCH, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::blobs;
use squirrel_db::media::MediaRecord;
use squirrel_media::exec::ExecResponse;
use squirrel_media::preview::{
    PREVIEW_VERSION, PreviewEncoding, PreviewFormat, bucket_for, bucket_for_source,
};
use squirrel_sandbox::exec::ExecError;
use squirrel_sandbox::preview::PreviewJob;
use squirrel_storage::{CacheKey, Variant, preview_cache};
use uuid::Uuid;

use super::thumbnails::{image_response, policy_headers};
use crate::blocking::run_storage;
use crate::content::{etag_matches, header_str};
use crate::error::ApiError;
use crate::state::AppState;

/// Rendered previews may change with the renderer (`PREVIEW_VERSION`):
/// clients revalidate within a week.
const CACHE_PREVIEW: &str = "private, max-age=604800";
/// A stored preview served while rendering is impossible: revalidate.
const CACHE_FALLBACK: &str = "private, no-cache";
const RETRY_AFTER_SECS: HeaderValue = HeaderValue::from_static("1");

pub(super) struct PreviewRequest<'a> {
    pub id: Uuid,
    pub user: Uuid,
    pub hash: ContentHash,
    pub media: &'a MediaRecord,
    pub px: Option<u32>,
}

/// What a render produced: the bytes, or why there is none.
enum Rendered {
    Preview(Vec<u8>, PreviewFormat),
    Impossible(String),
}

pub(super) async fn serve(
    state: &AppState,
    request: PreviewRequest<'_>,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let (width, height) = displayed_size(request.media);
    let bucket = bucket_for_source(bucket_for(request.px), width, height);
    let etag = format!("\"{}-p{bucket}-r{PREVIEW_VERSION}\"", request.hash.as_str());
    let policy = policy_headers(CACHE_PREVIEW, &etag)?;
    if etag_matches(header_str(headers, IF_NONE_MATCH), &etag) {
        return Ok((StatusCode::NOT_MODIFIED, policy).into_response());
    }
    let key = CacheKey {
        hash: request.hash.clone(),
        bucket,
    };
    if let Some((bytes, format)) = cached(state, &key).await? {
        return Ok(image_response(
            headers,
            policy,
            &etag,
            format.content_type(),
            bytes,
        ));
    }
    if !state.previews.allow_render(request.user) {
        return Ok(retry_later(ApiError::RateLimited));
    }
    // One render per key: whoever waited finds it cached.
    let lock = state.previews.key_lock(&key);
    let _rendering = lock.lock().await;
    if let Some((bytes, format)) = cached(state, &key).await? {
        return Ok(image_response(
            headers,
            policy,
            &etag,
            format.content_type(),
            bytes,
        ));
    }
    let Some(_slot) = state.previews.slot().await else {
        tracing::warn!("no preview render slot available, answering busy");
        return Ok(retry_later(ApiError::Busy));
    };
    match render(state, &request, &key).await {
        Ok(Rendered::Preview(bytes, format)) => Ok(image_response(
            headers,
            policy,
            &etag,
            format.content_type(),
            bytes,
        )),
        Ok(Rendered::Impossible(reason)) => {
            tracing::warn!(node_id = %request.id, %reason, "cannot render a preview");
            stored_fallback(state, &request, headers).await
        }
        Err(ApiError::Busy) => Ok(retry_later(ApiError::Busy)),
        Err(err) => Err(err),
    }
}

fn displayed_size(media: &MediaRecord) -> (u32, u32) {
    let side = |value: Option<i32>| value.and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
    (side(media.width), side(media.height))
}

fn retry_later(err: ApiError) -> Response {
    let mut response = err.into_response();
    response.headers_mut().insert(RETRY_AFTER, RETRY_AFTER_SECS);
    response
}

async fn cached(
    state: &AppState,
    key: &CacheKey,
) -> Result<Option<(Vec<u8>, PreviewFormat)>, ApiError> {
    let Some(cache) = state.preview_cache.clone() else {
        return Ok(None);
    };
    let key = key.clone();
    let found = run_storage(move || cache.read(&key)).await?;
    Ok(found.and_then(|(bytes, extension)| {
        PreviewFormat::from_extension(extension).map(|format| (bytes, format))
    }))
}

/// Renders into a scratch directory, then installs the result in the cache
/// (when there is room) and returns its bytes.
async fn render(
    state: &AppState,
    request: &PreviewRequest<'_>,
    key: &CacheKey,
) -> Result<Rendered, ApiError> {
    let hash = request.hash.clone();
    let blob = blobs::find(&state.pool, &hash)
        .await?
        .ok_or(ApiError::NotFound)?;
    let derived = state.storage.derived().clone();
    let scratch = run_storage(move || derived.scratch()).await?;
    let job = PreviewJob {
        input: state.storage.blob_path(&hash),
        out_dir: scratch.path().to_owned(),
        bucket: key.bucket,
        encoding: PreviewEncoding::Display,
        heif: MediaKind::is_heif_family(&blob.mime),
    };
    let preview = match state.previews.renderer.render(job).await {
        Ok(ExecResponse::Preview { preview }) => preview,
        Ok(ExecResponse::Unsupported { reason } | ExecResponse::Failed { reason, .. }) => {
            return Ok(Rendered::Impossible(reason));
        }
        Ok(other) => {
            return Ok(Rendered::Impossible(format!(
                "unexpected sandbox answer {other:?}"
            )));
        }
        Err(ExecError::TimedOut(after)) => {
            tracing::warn!(node_id = %request.id, ?after, "preview render timed out");
            return Err(ApiError::Busy);
        }
        Err(err) => return Ok(Rendered::Impossible(err.to_string())),
    };
    let format = preview.format;
    let free = state.available_space().await?;
    let cache = state.preview_cache.clone();
    let key = key.clone();
    let bytes = run_storage(move || {
        let source = scratch.path().join(format.file_name());
        // The child has exited: nothing changes the scratch directory any more.
        let bytes = preview_cache::read_rendered(&source)?;
        if let Some(cache) = cache
            && let Err(err) = cache.insert(&key, &source, format.extension(), free)
        {
            // Served anyway; the next request renders again.
            tracing::warn!(error = %err, "could not cache a rendered preview");
        }
        Ok(bytes)
    })
    .await?;
    Ok(Rendered::Preview(bytes, format))
}

/// The worker's stored 1440 px preview, while one exists (until
/// `admin media drop-previews`), for a photo that cannot be rendered now.
async fn stored_fallback(
    state: &AppState,
    request: &PreviewRequest<'_>,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let version = request.media.thumbs_version;
    let etag = format!("\"{}-preview-v{version}\"", request.hash.as_str());
    let policy = policy_headers(CACHE_FALLBACK, &etag)?;
    let (derived, hash) = (state.storage.derived().clone(), request.hash.clone());
    let bytes = run_storage(move || derived.read(&hash, Variant::Preview, version)).await?;
    Ok(image_response(headers, policy, &etag, "image/webp", bytes))
}
