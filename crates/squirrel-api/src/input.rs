//! `Json`, `Path`, `Query` and raw body extractors whose rejections use the API error envelope
//! instead of axum's plain-text bodies.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;

use crate::error::ApiError;

/// JSON request body. Requires `Content-Type: application/json`, which also
/// keeps cross-site HTML forms (login CSRF) away from the handlers.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        axum::Json::<T>::from_request(req, state)
            .await
            .map(|axum::Json(value)| Self(value))
            .map_err(json_error)
    }
}

/// Messages are fixed strings: serde's own messages may echo submitted values.
fn json_error(rejection: JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => ApiError::UnsupportedMediaType,
        JsonRejection::JsonSyntaxError(_) => {
            ApiError::BadRequest("the request body is not valid JSON".into())
        }
        JsonRejection::JsonDataError(_) => ApiError::Validation(
            "the request body is missing a field or has a field of the wrong type".into(),
        ),
        other if other.status() == StatusCode::PAYLOAD_TOO_LARGE => ApiError::PayloadTooLarge,
        _ => ApiError::BadRequest("the request body could not be read".into()),
    }
}

/// Path parameters; a malformed value (e.g. not a UUID) is a 400 envelope.
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    axum::extract::Path<T>: FromRequestParts<S, Rejection = PathRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Self(value))
            .map_err(|_| ApiError::BadRequest("invalid path parameter".into()))
    }
}

/// Query string; a malformed value is a 400 envelope.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    axum::extract::Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|_| ApiError::BadRequest("invalid query parameter".into()))
    }
}

/// Refuses a query string naming a parameter outside `allowed` (a typo such
/// as `contry=FR` must not silently widen a filter). Works across several
/// `ApiQuery` extractors of one route, which each ignore the others' fields.
pub fn known_params(raw: Option<&str>, allowed: &[&str]) -> Result<(), ApiError> {
    let unknown = raw
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').map_or(pair, |(key, _)| key))
        .map(|key| percent_encoding::percent_decode_str(key).decode_utf8_lossy())
        .any(|key| !allowed.contains(&key.as_ref()));
    if unknown {
        // A fixed message: never echo the submitted name.
        return Err(ApiError::Validation(
            "unknown query parameter for this route".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod known_params_tests {
    use super::*;

    #[test]
    fn only_listed_parameters_pass() {
        let allowed = ["q", "country", "camera_make"];
        assert!(known_params(None, &allowed).is_ok());
        assert!(known_params(Some(""), &allowed).is_ok());
        assert!(known_params(Some("q=dog&country=FR&"), &allowed).is_ok());
        assert!(known_params(Some("camera%5Fmake=Canon"), &allowed).is_ok());
        assert!(known_params(Some("q=dog&contry=FR"), &allowed).is_err());
        assert!(known_params(Some("flag"), &allowed).is_err());
    }
}
