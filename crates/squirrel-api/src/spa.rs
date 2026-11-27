use axum::http::{Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::error::ApiError;

/// Built SvelteKit output. `allow_missing` lets the backend build before the web UI exists.
#[derive(RustEmbed)]
#[folder = "../../web/build/"]
#[allow_missing = true]
struct WebAssets;

const IMMUTABLE_PREFIX: &str = "_app/immutable/";

/// Serves built assets, and `index.html` for client-side routes. Only GET/HEAD
/// are accepted, and a missing file (a path with an extension) is a real 404.
pub async fn serve(method: Method, uri: Uri) -> Response {
    respond(&method, uri.path(), |_| true)
}

/// The public listener's pages: built assets, and `index.html` for public
/// link pages (`/s/{token}`) only. Every other route is a 404, so the private
/// app is not even served to the internet.
pub async fn serve_public(method: Method, uri: Uri) -> Response {
    // The app's entry page only ever answers through a link page.
    if uri.path().trim_start_matches('/') == "index.html" {
        return ApiError::NotFound.into_response();
    }
    respond(&method, uri.path(), is_public_page)
}

/// Client-side routes the public listener serves.
fn is_public_page(path: &str) -> bool {
    path.strip_prefix("s/")
        .is_some_and(|token| !token.is_empty() && !token.contains('/'))
}

fn respond(method: &Method, path: &str, page_allowed: impl Fn(&str) -> bool) -> Response {
    let path = path.trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return ApiError::NotFound.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return ApiError::MethodNotAllowed.into_response();
    }
    let asset = WebAssets::get(path).map(|file| (path, file));
    let fallback = || {
        if looks_like_file(path) || !page_allowed(path) {
            None
        } else {
            WebAssets::get("index.html").map(|f| ("index.html", f))
        }
    };
    let (served_path, file) = match asset.or_else(fallback) {
        Some(found) => found,
        None if looks_like_file(path) || !page_allowed(path) => {
            return ApiError::NotFound.into_response();
        }
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

/// Client-side routes never contain a dot in their last segment; assets do.
fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_listener_serves_link_pages_only() {
        assert!(is_public_page("s/abc"));
        assert!(!is_public_page("s/"));
        assert!(!is_public_page("s/abc/def"));
        assert!(!is_public_page(""));
        assert!(!is_public_page("files"));
        assert!(!is_public_page("login"));
        assert!(!is_public_page("shared"));
    }
}
