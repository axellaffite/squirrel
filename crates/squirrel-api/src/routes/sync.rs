//! The phone's sync protocol: which assets are missing, and where to put them.

use axum::Json;
use axum::extract::State;
use squirrel_core::NodeName;
use squirrel_db::device_assets::{self, SyncItem};
use squirrel_db::sync_folders;

use crate::error::{ApiError, ErrorBody};
use crate::extract::DeviceCaller;
use crate::input::ApiJson;
use crate::state::AppState;
use crate::sync_dto::{SyncCheckRequest, SyncCheckResponse, SyncFolderRequest, SyncFolderResponse};

#[utoipa::path(post, path = "/api/sync/check", tag = "sync",
    operation_id = "sync_check",
    request_body = SyncCheckRequest,
    security(("device_bearer" = [])),
    responses(
        (status = 200, body = SyncCheckResponse),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Not a device"),
        (status = 422, body = ErrorBody, description = "The whole batch was refused"),
        (status = 429, body = ErrorBody),
    ))]
pub async fn sync_check(
    State(state): State<AppState>,
    caller: DeviceCaller,
    ApiJson(request): ApiJson<SyncCheckRequest>,
) -> Result<Json<SyncCheckResponse>, ApiError> {
    if let Some(limiter) = &state.sync_rate
        && !limiter.check(caller.device.id)
    {
        tracing::warn!(user_id = %caller.user.id, device_id = %caller.device.id,
            "sync check rate limited");
        return Err(ApiError::RateLimited);
    }
    request.validate()?;
    let items: Vec<SyncItem<'_>> = request
        .items
        .iter()
        .map(|item| SyncItem {
            device_asset_id: &item.device_asset_id,
            sha256: &item.sha256,
        })
        .collect();
    let missing =
        device_assets::missing(&state.pool, caller.device.id, caller.user.id, &items).await?;
    tracing::debug!(device_id = %caller.device.id, checked = items.len(), missing = missing.len(),
        "sync check");
    Ok(Json(SyncCheckResponse { missing }))
}

/// Years a backup folder may be named after.
const YEARS: std::ops::RangeInclusive<i32> = 1900..=9999;
/// Used when the device's name cannot name a folder (`.` or `..`).
const FALLBACK_FOLDER: &str = "Phone";

/// The device's name as a folder name (`/` is not allowed in names).
fn device_folder_name(device_name: &str) -> NodeName {
    NodeName::parse(&device_name.replace('/', "-"))
        .or_else(|_| NodeName::parse(FALLBACK_FOLDER))
        .expect("the fallback is a valid name")
}

#[utoipa::path(post, path = "/api/sync/folder", tag = "sync",
    operation_id = "sync_folder",
    request_body = SyncFolderRequest,
    security(("device_bearer" = [])),
    responses(
        (status = 200, body = SyncFolderResponse, description = "`Photos/<device name>/<year>`, created if needed"),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Not a device"),
        (status = 422, body = ErrorBody),
    ))]
pub async fn sync_folder(
    State(state): State<AppState>,
    caller: DeviceCaller,
    ApiJson(request): ApiJson<SyncFolderRequest>,
) -> Result<Json<SyncFolderResponse>, ApiError> {
    if !YEARS.contains(&request.year) {
        return Err(ApiError::Validation(
            "year must be between 1900 and 9999".into(),
        ));
    }
    let year = NodeName::parse(&request.year.to_string())?;
    let folder_id = sync_folders::ensure_year_folder(
        &state.pool,
        caller.user.id,
        caller.device.id,
        &device_folder_name(&caller.device.name),
        &year,
    )
    .await?;
    Ok(Json(SyncFolderResponse { folder_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_become_folder_names() {
        assert_eq!(device_folder_name("Pixel 8").as_str(), "Pixel 8");
        assert_eq!(device_folder_name("A/B").as_str(), "A-B");
        assert_eq!(device_folder_name("..").as_str(), "Phone");
    }
}
