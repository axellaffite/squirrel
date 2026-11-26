use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use squirrel_auth::HashError;
use squirrel_core::{
    AlbumNameError, ContentHashError, DisplayNameError, NodeNameError, PasswordError, UsernameError,
};
use squirrel_db::DbError;
use squirrel_storage::StorageError;
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
    /// The file cannot be opened in the text editor.
    #[error("{0}")]
    NotEditable(&'static str),
    /// A derived file (thumbnail) the worker has not produced yet.
    #[error("this preview is still being prepared")]
    NotReady,
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
    /// Photo search needs the CLIP text model, not installed on this server.
    #[error("photo search is not set up on this server")]
    MlUnavailable,
    #[error("too many uploads in progress; finish or cancel some first")]
    TooManyUploads,
    #[error("{0}")]
    Conflict(String),
    #[error("this invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error("a file or folder with this name already exists here")]
    NameConflict,
    #[error("this would exceed your storage quota")]
    QuotaExceeded,
    #[error("the server is out of disk space")]
    StorageFull,
    #[error("the uploaded content does not match its SHA-256; upload it again")]
    HashMismatch,
    #[error("{0}")]
    UploadIncomplete(String),
    #[error("the requested range is not satisfiable")]
    RangeNotSatisfiable,
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
            ApiError::NotEditable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "not_editable"),
            ApiError::NotReady => (StatusCode::NOT_FOUND, "not_ready"),
            ApiError::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed"),
            ApiError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            ApiError::UnsupportedMediaType => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
            }
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            ApiError::Busy => (StatusCode::SERVICE_UNAVAILABLE, "busy"),
            ApiError::MlUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "ml_unavailable"),
            ApiError::TooManyUploads => (StatusCode::TOO_MANY_REQUESTS, "too_many_uploads"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::InvitationInvalid => (StatusCode::GONE, "invitation_invalid"),
            ApiError::NameConflict => (StatusCode::CONFLICT, "name_conflict"),
            ApiError::QuotaExceeded => (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded"),
            ApiError::StorageFull => (StatusCode::INSUFFICIENT_STORAGE, "storage_full"),
            ApiError::HashMismatch => (StatusCode::UNPROCESSABLE_ENTITY, "hash_mismatch"),
            ApiError::UploadIncomplete(_) => (StatusCode::CONFLICT, "upload_incomplete"),
            ApiError::RangeNotSatisfiable => {
                (StatusCode::RANGE_NOT_SATISFIABLE, "range_not_satisfiable")
            }
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
            other @ (DbError::UploadBusy | DbError::DynamicAlbum) => {
                ApiError::Conflict(other.to_string())
            }
            DbError::TooManyUploads => ApiError::TooManyUploads,
            DbError::Busy(source) => {
                tracing::warn!(error = %source, "database conflict persisted after retries");
                ApiError::Busy
            }
            DbError::InvalidInput(message) => ApiError::BadRequest(message.into()),
            DbError::Sqlx(source) => ApiError::Internal(source.into()),
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NoSpace => {
                tracing::error!("disk full while writing to the data directory");
                ApiError::StorageFull
            }
            StorageError::NotFound => ApiError::NotFound,
            other => ApiError::Internal(other.into()),
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
validation_from!(
    AlbumNameError,
    UsernameError,
    PasswordError,
    DisplayNameError,
    NodeNameError,
    ContentHashError
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_database_conflicts_map_to_busy() {
        let err = ApiError::from(DbError::Busy(sqlx::Error::PoolTimedOut));
        assert!(matches!(err, ApiError::Busy));
        assert_eq!(err.status_and_code().0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn storage_failures_are_507_with_distinct_codes() {
        assert_eq!(
            ApiError::from(StorageError::NoSpace).status_and_code(),
            (StatusCode::INSUFFICIENT_STORAGE, "storage_full")
        );
        assert_eq!(
            ApiError::from(DbError::QuotaExceeded).status_and_code(),
            (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded")
        );
    }

    #[test]
    fn upload_failures_have_stable_codes() {
        assert_eq!(
            ApiError::HashMismatch.status_and_code(),
            (StatusCode::UNPROCESSABLE_ENTITY, "hash_mismatch")
        );
        assert_eq!(
            ApiError::UploadIncomplete("x".into()).status_and_code(),
            (StatusCode::CONFLICT, "upload_incomplete")
        );
        assert_eq!(
            ApiError::from(DbError::NameConflict).status_and_code(),
            (StatusCode::CONFLICT, "name_conflict")
        );
        assert_eq!(
            ApiError::RangeNotSatisfiable.status_and_code(),
            (StatusCode::RANGE_NOT_SATISFIABLE, "range_not_satisfiable")
        );
    }

    #[test]
    fn a_missing_blob_or_upload_file_is_not_found() {
        assert_eq!(
            ApiError::from(StorageError::NotFound).status_and_code().0,
            StatusCode::NOT_FOUND
        );
    }
}
