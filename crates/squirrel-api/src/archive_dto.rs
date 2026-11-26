//! Request and response bodies of zip downloads.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Files per zip, at most.
pub const MAX_ARCHIVE_FILES: usize = 500;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateArchiveRequest {
    /// 1–500 file ids, each one the caller may download (their own files, or
    /// files in something shared with them). Duplicates are collapsed; the
    /// zip keeps the order given.
    pub node_ids: Vec<Uuid>,
}

/// A zip ready to download with `GET /api/files/archive/{ticket}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ArchiveTicket {
    /// Opaque; usable only by the caller, as many times as needed, for 10 minutes.
    pub ticket: String,
    /// Suggested file name, e.g. `Squirrel 12 photos 2026-11-26.zip`.
    pub name: String,
    /// Exact byte length of the zip (its `Content-Length`).
    pub size: i64,
    /// Files in the zip.
    pub count: i64,
}
