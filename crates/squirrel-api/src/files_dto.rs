//! Request and response bodies for files, uploads and the trash.

use serde::{Deserialize, Serialize};
use squirrel_core::NodeKind;
use squirrel_db::nodes::NodeRecord;
use squirrel_db::uploads::UploadRecord;
use time::OffsetDateTime;
use utoipa::ToSchema;
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
