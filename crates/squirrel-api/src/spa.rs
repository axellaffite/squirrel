use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::error::ApiError;

/// Built SvelteKit output. `allow_missing` lets the backend build before the web UI exists.
#[derive(RustEmbed)]
#[folder = "../../web/build/"]
#[allow_missing = true]
struct WebAssets;

const IMMUTABLE_PREFIX: &str = "_app/immutable/";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return ApiError::NotFound.into_response();
    }
    let asset = WebAssets::get(path).map(|file| (path, file));
    let (served_path, file) =
        match asset.or_else(|| WebAssets::get("index.html").map(|f| ("index.html", f))) {
            Some(found) => found,
            None => return (StatusCode::NOT_FOUND, "web UI is not built").into_response(),
        };
    let cache = if served_path.starts_with(IMMUTABLE_PREFIX) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, file.metadata.mimetype().to_owned()),
            (header::CACHE_CONTROL, cache.to_owned()),
        ],
        file.data,
    )
        .into_response()
}
