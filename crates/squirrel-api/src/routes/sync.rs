//! The phone's sync protocol: which assets are missing, and where to put them.

use axum::Json;
use axum::extract::State;
use squirrel_db::device_assets::{self, SyncItem};

use crate::error::{ApiError, ErrorBody};
use crate::extract::DeviceCaller;
use crate::input::ApiJson;
use crate::state::AppState;
use crate::sync_dto::{SyncCheckRequest, SyncCheckResponse};

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
