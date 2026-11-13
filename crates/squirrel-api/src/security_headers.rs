//! Response headers applied to every response (API and SPA).

use axum::Router;
use axum::extract::Request;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::Response;
use tower_http::set_header::SetResponseHeaderLayer;

/// No `script-src` here: the SPA's own `<meta>` CSP (SvelteKit, hash mode)
/// covers scripts, including its inline bootstrap.
const CSP: &str = "frame-ancestors 'none'; base-uri 'none'; object-src 'none'";
const HSTS: &str = "max-age=31536000";

/// `hsts` should follow `cookie_secure`: only claim HTTPS when it is in use.
pub fn apply(router: Router, hsts: bool) -> Router {
    let router = [
        (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (X_FRAME_OPTIONS, "DENY"),
        (REFERRER_POLICY, "no-referrer"),
        (CONTENT_SECURITY_POLICY, CSP),
    ]
    .into_iter()
    .fold(router, |router, (name, value)| {
        router.layer(header_layer(name, value))
    });
    let router = if hsts {
        router.layer(header_layer(STRICT_TRANSPORT_SECURITY, HSTS))
    } else {
        router
    };
    router.layer(middleware::from_fn(no_store_for_api))
}

fn header_layer(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value))
}

/// API responses carry session-specific data: never cache them.
async fn no_store_for_api(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_api = path == "/api" || path.starts_with("/api/");
    let response = next.run(request).await;
    if !is_api {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    parts
        .headers
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Response::from_parts(parts, body)
}
