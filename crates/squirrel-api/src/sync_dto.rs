//! Request and response bodies for the phone's sync protocol.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use squirrel_core::ContentHash;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;

/// Items one sync check may carry.
pub const MAX_CHECK_ITEMS: usize = 500;
const DEVICE_ASSET_ID_MAX_BYTES: usize = 256;

/// A phone's id for an asset: 1–256 bytes, no control characters (Android
/// `"images:1234"`, iOS `PHAsset.localIdentifier`).
pub fn device_asset_id(raw: &str) -> Result<&str, ApiError> {
    if raw.is_empty() || raw.len() > DEVICE_ASSET_ID_MAX_BYTES {
        return Err(ApiError::Validation(
            "device_asset_id must be 1 to 256 bytes".into(),
        ));
    }
    if raw.chars().any(char::is_control) {
        return Err(ApiError::Validation(
            "device_asset_id must not contain control characters".into(),
        ));
    }
    Ok(raw)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SyncCheckItem {
    /// The phone's stable id for the asset, 1–256 bytes.
    pub device_asset_id: String,
    /// SHA-256 of the asset's original bytes, 64 lowercase hex characters.
    pub sha256: String,
}

/// Which of these assets does the server miss?
#[derive(Debug, Deserialize, ToSchema)]
pub struct SyncCheckRequest {
    /// 1–500 items, ids unique within the batch.
    pub items: Vec<SyncCheckItem>,
}

impl SyncCheckRequest {
    /// The whole batch is refused if any item is invalid.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.items.is_empty() || self.items.len() > MAX_CHECK_ITEMS {
            return Err(ApiError::Validation(format!(
                "items must hold 1 to {MAX_CHECK_ITEMS} entries"
            )));
        }
        let mut seen = HashSet::with_capacity(self.items.len());
        for item in &self.items {
            device_asset_id(&item.device_asset_id)?;
            ContentHash::parse(&item.sha256)?;
            if !seen.insert(item.device_asset_id.as_str()) {
                return Err(ApiError::Validation(
                    "device_asset_id values must be unique within a batch".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncCheckResponse {
    /// The ids to upload, in request order. Everything else is already backed up.
    pub missing: Vec<String>,
}

/// Where to put this year's backups.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SyncFolderRequest {
    /// The asset's local capture year, 1900–9999.
    pub year: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncFolderResponse {
    /// `Photos/<device name>/<year>`, created if needed.
    pub folder_id: Uuid,
}
