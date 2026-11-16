//! Request and response bodies of albums.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::photos_dto::AssetSummary;

/// Photos per add/remove request (and at creation), at most.
pub const MAX_NODE_IDS: usize = 500;

#[derive(Debug, Serialize, ToSchema)]
pub struct AlbumDto {
    pub id: Uuid,
    pub name: String,
    /// Photos in the album (trashed ones are not counted).
    pub item_count: i64,
    /// The chosen cover, else the newest photo; `null` for an empty album.
    pub cover: Option<AssetSummary>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAlbumRequest {
    /// 1–100 characters.
    pub name: String,
    /// Photos to put in it right away (at most 500); ids that are not the
    /// caller's photos are skipped.
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
}

/// Rename and/or choose the cover. At least one field is required.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAlbumRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// Must be a photo of the album.
    #[serde(default)]
    pub cover_node_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AlbumItemsRequest {
    /// 1–500 photo (file node) ids; ids that are not the caller's photos are skipped.
    pub node_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ItemsChanged {
    /// Photos actually added or removed.
    pub changed: u64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AlbumItemsQuery {
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Default 200, at most 500.
    pub limit: Option<u32>,
}
