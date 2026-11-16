//! Request and response bodies for devices (phones) and their tokens.

use serde::{Deserialize, Serialize};
use squirrel_core::Role;
use squirrel_db::devices::DeviceRecord;
use squirrel_db::users::UserRecord;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;

const NAME_MAX_CHARS: usize = 64;
const MODEL_MAX_CHARS: usize = 64;
const APP_VERSION_MAX_CHARS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DevicePlatform {
    Android,
    Ios,
}

impl DevicePlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }

    /// The database CHECK guarantees a known value; anything else reads as Android.
    fn from_db(value: &str) -> Self {
        match value {
            "ios" => Self::Ios,
            _ => Self::Android,
        }
    }
}

/// The phone signing in.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceInfo {
    /// Shown in Settings › Devices and used as the backup folder's name. 1–64 characters.
    pub name: String,
    pub platform: DevicePlatform,
    /// Hardware model, e.g. "Pixel 8". At most 64 characters.
    #[serde(default)]
    pub model: Option<String>,
    /// The app's version, 1–32 characters.
    pub app_version: String,
}

/// A validated [`DeviceInfo`].
pub struct ValidDevice {
    pub name: String,
    pub platform: DevicePlatform,
    pub model: Option<String>,
    pub app_version: String,
}

impl DeviceInfo {
    pub fn validate(self) -> Result<ValidDevice, ApiError> {
        let model = self
            .model
            .map(|model| model.trim().to_owned())
            .filter(|model| !model.is_empty());
        Ok(ValidDevice {
            name: device_name(&self.name)?,
            platform: self.platform,
            model: model
                .map(|model| text("model", &model, MODEL_MAX_CHARS))
                .transpose()?,
            app_version: text(
                "app_version",
                self.app_version.trim(),
                APP_VERSION_MAX_CHARS,
            )?,
        })
    }
}

/// A device name: trimmed, 1–64 characters, no control characters.
pub fn device_name(raw: &str) -> Result<String, ApiError> {
    text("name", raw.trim(), NAME_MAX_CHARS)
}

fn text(field: &str, value: &str, max_chars: usize) -> Result<String, ApiError> {
    let chars = value.chars().count();
    if chars == 0 || chars > max_chars {
        return Err(ApiError::Validation(format!(
            "{field} must be 1 to {max_chars} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::Validation(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(value.to_owned())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeviceLoginRequest {
    pub username: String,
    pub password: String,
    pub device: DeviceInfo,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// The current (or, after a lost response, the previous) `sqr_…` token.
    pub refresh_token: String,
}

/// Tokens of one device. Send the access token as `Authorization: Bearer
/// <access_token>`; before it expires, trade the refresh token for a new pair
/// (`POST /api/auth/device/refresh`). Every refresh rotates the refresh token.
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenPair {
    /// `sqa_…`, valid 15 minutes.
    pub access_token: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub access_expires_at: OffsetDateTime,
    /// `sqr_…`. Store it securely; it is shown once.
    pub refresh_token: String,
    /// Pushed back on every refresh.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub refresh_expires_at: OffsetDateTime,
}

/// The signed-in user, as a device sees it.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
}

impl From<&UserRecord> for AccountDto {
    fn from(user: &UserRecord) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceDto {
    pub id: Uuid,
    pub name: String,
    pub platform: DevicePlatform,
    pub model: Option<String>,
    pub app_version: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub last_seen_at: OffsetDateTime,
    /// Set once the device was signed out; its tokens no longer work.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub revoked_at: Option<OffsetDateTime>,
    /// Whether this is the calling device.
    pub current: bool,
}

impl DeviceDto {
    pub fn new(record: &DeviceRecord, current: bool) -> Self {
        Self {
            id: record.id,
            name: record.name.clone(),
            platform: DevicePlatform::from_db(&record.platform),
            model: record.model.clone(),
            app_version: record.app_version.clone(),
            created_at: record.created_at,
            last_seen_at: record.last_seen_at,
            revoked_at: record.revoked_at,
            current,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceLoginResponse {
    pub device: DeviceDto,
    pub account: AccountDto,
    pub tokens: TokenPair,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshResponse {
    pub tokens: TokenPair,
}

/// The calling device and its account.
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentDeviceResponse {
    pub device: DeviceDto,
    pub account: AccountDto,
    /// `Photos/<device name>`, once `POST /api/sync/folder` created it.
    pub backup_folder_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameDeviceRequest {
    /// 1–64 characters.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, version: &str) -> DeviceInfo {
        DeviceInfo {
            name: name.into(),
            platform: DevicePlatform::Android,
            model: Some("  ".into()),
            app_version: version.into(),
        }
    }

    #[test]
    fn device_info_is_trimmed_and_bounded() {
        let valid = info("  Pixel 8 ", "0.1.0").validate().unwrap();
        assert_eq!(valid.name, "Pixel 8");
        assert_eq!(valid.model, None);
        assert!(info("", "0.1.0").validate().is_err());
        assert!(info(&"x".repeat(65), "0.1.0").validate().is_err());
        assert!(info(&"é".repeat(64), "0.1.0").validate().is_ok());
        assert!(info("a\nb", "0.1.0").validate().is_err());
        assert!(info("Pixel", "").validate().is_err());
        assert!(info("Pixel", &"1".repeat(33)).validate().is_err());
    }
}
