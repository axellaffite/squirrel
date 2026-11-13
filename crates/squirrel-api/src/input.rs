//! `Json`, `Path`, `Query` and raw body extractors whose rejections use the API error envelope
//! instead of axum's plain-text bodies.

use axum::body::Bytes;
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
#[allow(dead_code, reason = "used by the listing routes (Task 16)")]
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

/// Raw request body (upload chunks). Over the route's body limit → 413 envelope.
pub struct ApiBytes(pub Bytes);

impl<S: Send + Sync> FromRequest<S> for ApiBytes {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        Bytes::from_request(req, state)
            .await
            .map(Self)
            .map_err(|rejection| {
                if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::PayloadTooLarge
                } else {
                    ApiError::BadRequest("the request body could not be read".into())
                }
            })
    }
}
