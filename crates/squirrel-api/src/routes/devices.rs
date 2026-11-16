//! Device management: the calling phone (`/api/devices/current`, device
//! logout), and the web's list and revocation (`/api/devices`).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_db::devices;
use uuid::Uuid;

use crate::devices_dto::{
    AccountDto, CurrentDeviceResponse, DeviceDto, RenameDeviceRequest, device_name,
};
use crate::error::{ApiError, ErrorBody};
use crate::extract::{CurrentUser, DeviceCaller};
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;

fn current_response(
    caller: &DeviceCaller,
    device: &devices::DeviceRecord,
) -> CurrentDeviceResponse {
    CurrentDeviceResponse {
        device: DeviceDto::new(device, true),
        account: AccountDto::from(&caller.user),
        backup_folder_id: device.backup_folder_id,
    }
}

#[utoipa::path(get, path = "/api/devices/current", tag = "devices",
    operation_id = "get_current_device",
    security(("device_bearer" = [])),
    responses((status = 200, body = CurrentDeviceResponse), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Not a device")))]
pub async fn get_current_device(caller: DeviceCaller) -> Json<CurrentDeviceResponse> {
    Json(current_response(&caller, &caller.device))
}

#[utoipa::path(patch, path = "/api/devices/current", tag = "devices",
    operation_id = "rename_current_device",
    request_body = RenameDeviceRequest,
    security(("device_bearer" = [])),
    responses((status = 200, body = CurrentDeviceResponse), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn rename_current_device(
    State(state): State<AppState>,
    caller: DeviceCaller,
    ApiJson(request): ApiJson<RenameDeviceRequest>,
) -> Result<Json<CurrentDeviceResponse>, ApiError> {
    let name = device_name(&request.name)?;
    let device = devices::rename(&state.pool, caller.user.id, caller.device.id, &name)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    Ok(Json(current_response(&caller, &device)))
}

#[utoipa::path(post, path = "/api/auth/device/logout", tag = "devices",
    operation_id = "device_logout",
    security(("device_bearer" = [])),
    responses((status = 204, description = "This device is signed out; its tokens no longer work"),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn device_logout(
    State(state): State<AppState>,
    caller: DeviceCaller,
) -> Result<StatusCode, ApiError> {
    devices::revoke(&state.pool, caller.user.id, caller.device.id).await?;
    tracing::info!(user_id = %caller.user.id, device_id = %caller.device.id, revoked_by = "self",
        "device revoked");
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/devices", tag = "devices",
    operation_id = "list_devices",
    security(("session_cookie" = [])),
    responses((status = 200, body = Vec<DeviceDto>, description = "Live devices first, most recently seen first, then revoked ones"),
              (status = 401, body = ErrorBody)))]
pub async fn list_devices(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<DeviceDto>>, ApiError> {
    let records = devices::list_for_owner(&state.pool, current.user.id).await?;
    Ok(Json(
        records
            .iter()
            .map(|record| DeviceDto::new(record, false))
            .collect(),
    ))
}

#[utoipa::path(delete, path = "/api/devices/{id}", tag = "devices",
    operation_id = "revoke_device",
    params(("id" = Uuid, Path, description = "Device id")),
    security(("session_cookie" = [], "csrf_token" = [])),
    responses((status = 204, description = "Revoked (or already revoked): its tokens no longer work; what it backed up stays"),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)))]
pub async fn revoke_device(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !devices::revoke(&state.pool, current.user.id, id).await? {
        return Err(ApiError::NotFound);
    }
    tracing::info!(user_id = %current.user.id, device_id = %id, revoked_by = "web",
        "device revoked");
    Ok(StatusCode::NO_CONTENT)
}
