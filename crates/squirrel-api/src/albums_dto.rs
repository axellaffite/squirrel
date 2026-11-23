//! Request and response bodies of albums.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::photos_dto::AssetSummary;
use crate::sharing_dto::{AccessLevel, MemberDto};

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
    /// What the caller may do with it.
    pub access: AccessLevel,
    /// Who shared it with the caller; `null` for the caller's own album.
    pub shared_by: Option<MemberDto>,
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
    /// A photo of the album to show as its cover; `null` goes back to the
    /// newest photo; absent keeps the current choice.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub cover_node_id: Option<Option<Uuid>>,
}

/// A field that is present, `null` included (`Some(None)`); an absent one is
/// `None` through `#[serde(default)]`.
pub(crate) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AlbumItemsRequest {
    /// 1–500 photo (file node) ids. Adding: ids that are not the caller's
    /// photos are skipped. Removing: the album's owner removes any photo,
    /// someone it is shared with only their own.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cover(json: &str) -> Option<Option<Uuid>> {
        serde_json::from_str::<UpdateAlbumRequest>(json)
            .unwrap()
            .cover_node_id
    }

    #[test]
    fn the_cover_is_kept_cleared_or_set() {
        let id = Uuid::now_v7();
        assert_eq!(cover(r#"{"name": "x"}"#), None);
        assert_eq!(cover(r#"{"cover_node_id": null}"#), Some(None));
        assert_eq!(
            cover(&format!(r#"{{"cover_node_id": "{id}"}}"#)),
            Some(Some(id))
        );
        assert!(serde_json::from_str::<UpdateAlbumRequest>(r#"{"cover_node_id": 3}"#).is_err());
    }
}
