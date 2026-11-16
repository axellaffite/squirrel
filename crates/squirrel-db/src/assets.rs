//! Timeline entries: one per image file node (`assets`). The foreign key on
//! `(node_id, owner_id)` ties each entry to its node and owner; trashed nodes
//! keep their entry (the timeline filters on `nodes.deleted_at`), permanently
//! deleted ones lose it by cascade.

use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgConnection, PgPool};
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

use crate::DbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Photo,
    Screenshot,
}

impl AssetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Screenshot => "screenshot",
        }
    }
}

/// Where `taken_at` came from (`assets.taken_source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenSource {
    Exif,
    Filename,
    /// The uploading phone's capture time (Phase 6).
    Device,
    Upload,
}

impl TakenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exif => "exif",
            Self::Filename => "filename",
            Self::Device => "device",
            Self::Upload => "upload",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAsset {
    pub node_id: Uuid,
    pub owner_id: Uuid,
    pub sha256: String,
    pub kind: AssetKind,
    /// Wall-clock capture time.
    pub taken_at: PrimitiveDateTime,
    pub taken_offset_min: Option<i16>,
    pub taken_source: TakenSource,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AssetRecord {
    pub node_id: Uuid,
    pub owner_id: Uuid,
    pub sha256: String,
    pub kind: String,
    pub taken_at: PrimitiveDateTime,
    pub taken_offset_min: Option<i16>,
    pub taken_source: String,
    pub created_at: OffsetDateTime,
}

/// Creates the node's entry or updates its capture time and kind, except that
/// a provisional time (file name, upload) never replaces an EXIF one: use
/// [`replace`] when the metadata itself changed. `false`: nothing was written
/// (the node, with this owner, no longer exists, or the EXIF time was kept).
/// The node row is locked (`FOR KEY SHARE`) instead of relying on the foreign
/// key, whose violation would abort the caller's whole transaction.
pub async fn upsert(conn: &mut PgConnection, asset: &NewAsset) -> Result<bool, DbError> {
    write(conn, asset, KEEP_EXIF).await
}

/// [`upsert`] without the EXIF guard: for `extract_metadata`, which is the
/// authority on what the file's metadata says.
pub async fn replace(conn: &mut PgConnection, asset: &NewAsset) -> Result<bool, DbError> {
    write(conn, asset, "").await
}

const KEEP_EXIF: &str = "WHERE assets.taken_source <> 'exif' OR EXCLUDED.taken_source = 'exif'";

/// `guard`: a fixed `WHERE` for the conflict update, never built from input.
async fn write(conn: &mut PgConnection, asset: &NewAsset, guard: &str) -> Result<bool, DbError> {
    let sql = format!(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_offset_min, \
                             taken_source) \
         SELECT n.id, n.owner_id, $3, $4, $5, $6, $7 FROM nodes n \
          WHERE n.id = $1 AND n.owner_id = $2 FOR KEY SHARE \
         ON CONFLICT (node_id) DO UPDATE SET kind = EXCLUDED.kind, \
                taken_at = EXCLUDED.taken_at, taken_offset_min = EXCLUDED.taken_offset_min, \
                taken_source = EXCLUDED.taken_source {guard}"
    );
    let written = sqlx::query(&sql)
        .bind(asset.node_id)
        .bind(asset.owner_id)
        .bind(&asset.sha256)
        .bind(asset.kind.as_str())
        .bind(asset.taken_at)
        .bind(asset.taken_offset_min)
        .bind(asset.taken_source.as_str())
        .execute(conn)
        .await?;
    Ok(written.rows_affected() == 1)
}

/// Internal (no owner check): the entry of a node, for tests and the worker.
#[doc(hidden)]
pub async fn find(pool: &PgPool, node_id: Uuid) -> Result<Option<AssetRecord>, DbError> {
    Ok(sqlx::query_as::<_, AssetRecord>(
        "SELECT node_id, owner_id, sha256, kind, taken_at, taken_offset_min, taken_source, \
                created_at \
           FROM assets WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?)
}

/// A file node of some blob, with what deciding its timeline entry needs.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct BlobNode {
    pub node_id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub mime: Option<String>,
    pub created_at: OffsetDateTime,
}

/// Internal (worker only, no owner check): every file node, live or trashed,
/// of every owner, that references `sha256`.
#[doc(hidden)]
pub async fn for_blob<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
) -> Result<Vec<BlobNode>, DbError> {
    Ok(sqlx::query_as::<_, BlobNode>(
        "SELECT id AS node_id, owner_id, name, mime, created_at FROM nodes \
          WHERE blob_sha256 = $1 AND kind = 'file' ORDER BY id",
    )
    .bind(sha256)
    .fetch_all(executor)
    .await?)
}
