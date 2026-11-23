//! Request and response bodies for files, uploads and the trash.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use squirrel_core::NodeKind;
use squirrel_db::media::ThumbRef;
use squirrel_db::nodes::NodeRecord;
use squirrel_db::uploads::UploadRecord;

use crate::sharing_dto::{AccessLevel, MemberDto};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeDto {
    pub id: Uuid,
    /// `null` for the top-level folders.
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub kind: NodeKind,
    /// Bytes; 0 for folders.
    pub size: i64,
    /// Detected from the content at upload; `null` for folders.
    pub mime: Option<String>,
    /// Top-level folders cannot be renamed, moved or deleted.
    pub is_root: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub modified_at: OffsetDateTime,
    /// Set when a thumbnail is ready (`GET /api/files/{id}/thumbnail`); `null`
    /// otherwise, and in listings that do not look thumbnails up.
    pub thumbnail: Option<ThumbnailRef>,
}

/// A ready thumbnail of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ThumbnailRef {
    /// Pass as `v` to the thumbnail URL: a new version is a new URL.
    pub version: i16,
    /// A compact blurred placeholder (blurha.sh) to show while it loads.
    pub blurhash: Option<String>,
}

impl From<&ThumbRef> for ThumbnailRef {
    fn from(thumb: &ThumbRef) -> Self {
        Self {
            version: thumb.version,
            blurhash: thumb.blurhash.clone(),
        }
    }
}

impl NodeDto {
    /// The node, with its thumbnail when `thumbnails` (by blob hash) has one.
    pub fn with_thumbnails(node: &NodeRecord, thumbnails: &HashMap<String, ThumbRef>) -> Self {
        let thumbnail = node
            .blob_sha256
            .as_ref()
            .and_then(|sha| thumbnails.get(sha))
            .map(ThumbnailRef::from);
        Self {
            thumbnail,
            ..Self::from(node)
        }
    }
}

impl From<&NodeRecord> for NodeDto {
    fn from(node: &NodeRecord) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent_id,
            name: node.name.clone(),
            kind: node.kind(),
            size: node.size,
            mime: node.mime.clone(),
            is_root: node.is_root(),
            created_at: node.created_at,
            modified_at: node.modified_at,
            thumbnail: None,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUploadRequest {
    /// Destination folder.
    pub parent_id: Uuid,
    pub name: String,
    /// Total size in bytes.
    pub size: u64,
    /// SHA-256 of the whole file, 64 lowercase hex characters.
    pub sha256: String,
    /// When the photo or video was taken, by the client's clock: RFC 3339
    /// with its UTC offset (`2024-07-14T18:30:05+02:00`). Used for the
    /// timeline when the file itself (EXIF, name) has no date.
    #[serde(default)]
    #[schema(format = DateTime)]
    pub taken_at: Option<String>,
    /// The phone's id for this asset (devices only): recorded so the sync
    /// check knows it is backed up.
    #[serde(default)]
    pub device_asset_id: Option<String>,
}

/// An upload session: send the missing chunks, then complete it.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadSessionDto {
    pub upload_id: Uuid,
    pub chunk_size: u64,
    pub chunk_count: u32,
    /// Chunk indexes already stored, ascending.
    pub received_chunks: Vec<u32>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// Devices only, when the session is opened or resumed: `squ_…`, sent as
    /// `Authorization: Upload <token>` for this upload's chunks and status
    /// (background transfers that outlive the access token). Shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
}

impl UploadSessionDto {
    pub fn new(record: &UploadRecord, received: &[i32]) -> Self {
        Self {
            upload_id: record.id,
            chunk_size: u64::try_from(record.chunk_size).unwrap_or_default(),
            chunk_count: u32::try_from(record.chunk_count).unwrap_or_default(),
            received_chunks: received
                .iter()
                .filter_map(|&i| u32::try_from(i).ok())
                .collect(),
            expires_at: record.expires_at,
            upload_token: None,
        }
    }
}

/// The content was already in the caller's library: a node was created without
/// uploading anything.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeduplicatedUpload {
    /// Always `true`.
    pub deduplicated: bool,
    pub node: NodeDto,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListNodesQuery {
    /// Folder to list; omit it for the top-level folders.
    pub parent_id: Option<Uuid>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Page size, default 100, at most 500.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeListing {
    /// Folders first, then files; names in byte order.
    pub items: Vec<NodeDto>,
    /// Pass back as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PathSegmentDto {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeDetails {
    pub node: NodeDto,
    /// Ancestors from the top-level folder down, excluding the node itself.
    /// For a node shared with the caller: from the shared folder down (empty
    /// when the node itself is what was shared).
    pub path: Vec<PathSegmentDto>,
    /// What the caller may do with it.
    pub access: AccessLevel,
    /// Who shared it with the caller; `null` for the caller's own node.
    pub shared_by: Option<MemberDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    pub parent_id: Uuid,
    pub name: String,
}

/// Rename and/or move. At least one field is required.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNodeRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// New parent folder.
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrashQuery {
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Page size, default 100, at most 500.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrashEntryDto {
    /// `parent_id` is where it was deleted from.
    pub node: NodeDto,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub deleted_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrashListing {
    /// Newest first. Folders appear once; their contents come back with them.
    pub items: Vec<TrashEntryDto>,
    /// Pass back as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}
