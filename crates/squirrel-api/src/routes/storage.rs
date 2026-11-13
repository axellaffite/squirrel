//! The caller's storage usage.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use squirrel_db::quota;
use utoipa::ToSchema;

use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct StorageUsageDto {
    /// Logical bytes of live files: what the quota limits.
    pub used_bytes: i64,
    /// Bytes in the trash (purged after 30 days, not counted against the quota).
    pub trash_bytes: i64,
    /// `null`: unlimited.
    pub quota_bytes: Option<i64>,
}

#[utoipa::path(get, path = "/api/storage", tag = "storage",
    responses((status = 200, body = StorageUsageDto), (status = 401, body = ErrorBody)))]
pub async fn storage_usage(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<StorageUsageDto>, ApiError> {
    let usage = quota::usage(&state.pool, current.user.id).await?;
    Ok(Json(StorageUsageDto {
        used_bytes: usage.used_bytes,
        trash_bytes: usage.trash_bytes,
        quota_bytes: usage.quota_bytes,
    }))
}
