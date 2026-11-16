//! Signing phones in with device tokens: `POST /api/auth/device/login` and
//! `POST /api/auth/device/refresh`. Neither sets a cookie.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_auth::{DeviceToken, hash_token, parse_refresh};
use squirrel_db::devices::{self, DeviceRecord, NewAccess, NewDevice, Refresh, Rotation};
use squirrel_db::users;
use time::OffsetDateTime;

use crate::client_ip::ClientIp;
use crate::devices_dto::{
    AccountDto, DeviceDto, DeviceLoginRequest, DeviceLoginResponse, RefreshRequest,
    RefreshResponse, TokenPair,
};
use crate::error::{ApiError, ErrorBody};
use crate::input::ApiJson;
use crate::routes::auth::authenticate;
use crate::state::AppState;

/// Freshly generated tokens, with the hashes to store.
struct Issued {
    access: String,
    refresh: String,
    access_expires_at: OffsetDateTime,
    refresh_expires_at: OffsetDateTime,
}

impl Issued {
    fn new(state: &AppState) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            access: DeviceToken::access(),
            refresh: DeviceToken::refresh(),
            access_expires_at: now + state.config.device_access_ttl,
            refresh_expires_at: now + state.config.device_refresh_ttl,
        }
    }

    fn pair(self, refresh_expires_at: OffsetDateTime) -> TokenPair {
        TokenPair {
            access_token: self.access,
            access_expires_at: self.access_expires_at,
            refresh_token: self.refresh,
            refresh_expires_at,
        }
    }
}

#[utoipa::path(post, path = "/api/auth/device/login", tag = "devices",
    operation_id = "device_login",
    request_body = DeviceLoginRequest,
    security(()),
    responses(
        (status = 201, body = DeviceLoginResponse, description = "Signed in: a new device and its tokens"),
        (status = 401, body = ErrorBody, description = "invalid_credentials"),
        (status = 422, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ))]
pub async fn device_login(
    State(state): State<AppState>,
    client_ip: ClientIp,
    ApiJson(request): ApiJson<DeviceLoginRequest>,
) -> Result<(StatusCode, Json<DeviceLoginResponse>), ApiError> {
    let device = request.device.validate()?;
    let user = authenticate(&state, &client_ip, &request.username, request.password).await?;
    let issued = Issued::new(&state);
    let (access_hash, refresh_hash) = (hash_token(&issued.access), hash_token(&issued.refresh));
    let record = devices::create(
        &state.pool,
        &NewDevice {
            owner_id: user.id,
            name: &device.name,
            platform: device.platform.as_str(),
            model: device.model.as_deref(),
            app_version: &device.app_version,
            refresh_hash: &refresh_hash,
            refresh_expires_at: issued.refresh_expires_at,
            access: NewAccess {
                hash: &access_hash,
                expires_at: issued.access_expires_at,
            },
        },
    )
    .await?;
    tracing::info!(user_id = %user.id, username = %user.username, device_id = %record.id,
        platform = %record.platform, %client_ip, "device login succeeded");
    let body = DeviceLoginResponse {
        device: DeviceDto::new(&record, true),
        account: AccountDto::from(&user),
        tokens: issued.pair(record.refresh_expires_at),
    };
    Ok((StatusCode::CREATED, Json(body)))
}

#[utoipa::path(post, path = "/api/auth/device/refresh", tag = "devices",
    operation_id = "refresh_device_token",
    request_body = RefreshRequest,
    security(()),
    responses(
        (status = 200, body = RefreshResponse, description = "New tokens; the refresh token was rotated"),
        (status = 401, body = ErrorBody, description = "unauthorized: sign in again"),
        (status = 429, body = ErrorBody),
    ))]
pub async fn device_refresh(
    State(state): State<AppState>,
    client_ip: ClientIp,
    ApiJson(request): ApiJson<RefreshRequest>,
) -> Result<Json<RefreshResponse>, ApiError> {
    // Shape first: garbage, sessions and access tokens never reach the database.
    let presented = parse_refresh(&request.refresh_token).ok_or(ApiError::Unauthorized)?;
    let issued = Issued::new(&state);
    let (access_hash, refresh_hash) = (hash_token(&issued.access), hash_token(&issued.refresh));
    let rotation = devices::rotate_refresh(
        &state.pool,
        &Refresh {
            presented: &hash_token(presented),
            new_refresh: &refresh_hash,
            refresh_expires_at: issued.refresh_expires_at,
            access: NewAccess {
                hash: &access_hash,
                expires_at: issued.access_expires_at,
            },
        },
    )
    .await?;
    let device = match rotation {
        Rotation::Rotated(device) => device,
        Rotation::Reused {
            device_id,
            owner_id,
        } => {
            tracing::warn!(user_id = %owner_id, %device_id, %client_ip, revoked_by = "reuse",
                "refresh token reuse: device revoked");
            return Err(ApiError::Unauthorized);
        }
        Rotation::Expired | Rotation::Unknown => return Err(ApiError::Unauthorized),
    };
    ensure_owner_active(&state, &device).await?;
    Ok(Json(RefreshResponse {
        tokens: issued.pair(device.refresh_expires_at),
    }))
}

/// A disabled account's devices cannot refresh (their access tokens are
/// already refused by the extractor).
async fn ensure_owner_active(state: &AppState, device: &DeviceRecord) -> Result<(), ApiError> {
    users::find_by_id(&state.pool, device.owner_id)
        .await?
        .filter(|user| !user.is_disabled())
        .map(|_| ())
        .ok_or(ApiError::Unauthorized)
}
