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
    Validation(String),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("authentication required")]
    Unauthorized,
    #[error("you are not allowed to do this")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("this invitation is invalid, expired or already used")]
    InvitationInvalid,
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
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            ApiError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::InvitationInvalid => (StatusCode::GONE, "invitation_invalid"),
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
