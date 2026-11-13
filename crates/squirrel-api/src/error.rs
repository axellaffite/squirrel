use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use squirrel_auth::HashError;
use squirrel_core::{DisplayNameError, PasswordError, UsernameError};
use squirrel_db::DbError;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Validation(String),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("authentication required")]
    Unauthorized,
    #[error("you are not allowed to do this")]
    Forbidden,
    /// The `X-CSRF-Token` header is missing or does not match the session.
    #[error("your session needs to be refreshed, please retry")]
    CsrfInvalid,
    #[error("not found")]
    NotFound,
    #[error("this method is not allowed here")]
    MethodNotAllowed,
    #[error("the request body is too large")]
    PayloadTooLarge,
    #[error("the request body must be JSON (Content-Type: application/json)")]
    UnsupportedMediaType,
    #[error("too many requests, wait a moment and try again")]
    RateLimited,
    #[error("the server is busy, try again in a moment")]
    Busy,
    #[error("{0}")]
    Conflict(String),
    #[error("this invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error("a file or folder with this name already exists here")]
    NameConflict,
    #[error("this would exceed your storage quota")]
    QuotaExceeded,
    #[error("internal server error")]
    Internal(#[source] anyhow::Error),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable machine-readable code, e.g. `invalid_credentials`.
    pub code: String,
    /// Human-readable message, safe to display.
    pub message: String,
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            ApiError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::CsrfInvalid => (StatusCode::FORBIDDEN, "csrf_invalid"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            ApiError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            ApiError::UnsupportedMediaType => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
            }
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            ApiError::Busy => (StatusCode::SERVICE_UNAVAILABLE, "busy"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::InvitationInvalid => (StatusCode::GONE, "invitation_invalid"),
            ApiError::NameConflict => (StatusCode::CONFLICT, "name_conflict"),
            ApiError::QuotaExceeded => (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Internal(source) = &self {
            tracing::error!(error = ?source, "internal error");
        }
        let (status, code) = self.status_and_code();
        let body = ErrorBody {
            error: ErrorDetail {
                code: code.to_owned(),
                message: self.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFirstUser => ApiError::Conflict("setup is already complete".into()),
            DbError::UsernameTaken => ApiError::Conflict("this username is already taken".into()),
            DbError::InvitationInvalid => ApiError::InvitationInvalid,
            DbError::NotFound => ApiError::NotFound,
            DbError::NameConflict => ApiError::NameConflict,
            DbError::QuotaExceeded => ApiError::QuotaExceeded,
            other @ (DbError::ProtectedRoot | DbError::MoveIntoSelf) => {
                ApiError::Validation(other.to_string())
            }
            other @ DbError::UploadBusy => ApiError::Conflict(other.to_string()),
            DbError::Busy(source) => {
                tracing::warn!(error = %source, "database conflict persisted after retries");
                ApiError::Busy
            }
            DbError::InvalidInput(message) => ApiError::BadRequest(message.into()),
            DbError::Sqlx(source) => ApiError::Internal(source.into()),
        }
    }
}

impl From<HashError> for ApiError {
    fn from(err: HashError) -> Self {
        ApiError::Internal(err.into())
    }
}

macro_rules! validation_from {
    ($($ty:ty),*) => {$(
        impl From<$ty> for ApiError {
            fn from(err: $ty) -> Self {
                ApiError::Validation(err.to_string())
            }
        }
    )*};
}
validation_from!(UsernameError, PasswordError, DisplayNameError);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_database_conflicts_map_to_busy() {
        let err = ApiError::from(DbError::Busy(sqlx::Error::PoolTimedOut));
        assert!(matches!(err, ApiError::Busy));
        assert_eq!(err.status_and_code().0, StatusCode::SERVICE_UNAVAILABLE);
    }
}
