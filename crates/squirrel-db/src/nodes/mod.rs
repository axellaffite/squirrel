//! The per-user file tree. Every function is scoped by `owner_id`: another
//! user's node behaves exactly like a missing one.

mod insert;
mod read;
mod tree;

pub use insert::{ClientTaken, NewFile, free_name, insert_file};
pub use read::{
    ListCursor, PathSegment, find_any, find_live, find_live_file, find_live_folder, find_live_many,
    list_children, path_to,
};
pub use tree::{NodeChange, create_folder, update};

use sqlx::{FromRow, PgConnection};
use squirrel_core::NodeKind;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

/// Top-level folders every user owns. They cannot be renamed, moved or trashed.
pub const DEFAULT_ROOTS: [&str; 3] = ["Photos", "Documents", "Files"];
/// Where restored items go when their original folder is no longer live.
pub const RESTORE_FALLBACK_ROOT: &str = "Files";
pub(crate) const LIVE_NAME_CONSTRAINT: &str = "nodes_live_name_key";
pub(crate) const NODE_COLUMNS: &str = "id, owner_id, parent_id, name, kind, blob_sha256, size, \
     mime, created_at, modified_at, deleted_at";

#[derive(Debug, Clone, FromRow)]
pub struct NodeRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    kind: String,
    pub blob_sha256: Option<String>,
    pub size: i64,
    pub mime: Option<String>,
    pub created_at: OffsetDateTime,
    pub modified_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

impl NodeRecord {
    /// The CHECK constraint guarantees a known kind. An unknown one is treated as
    /// a folder, for which no content is ever served.
    pub fn kind(&self) -> NodeKind {
        self.kind.parse().unwrap_or_else(|_| {
            tracing::warn!(node_id = %self.id, kind = %self.kind, "unknown node kind in database");
            NodeKind::Folder
        })
    }

    pub fn is_folder(&self) -> bool {
        self.kind() == NodeKind::Folder
    }

    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }
}

/// Creates the default roots for a new user, inside the caller's transaction.
pub(crate) async fn create_default_roots(
    conn: &mut PgConnection,
    owner_id: Uuid,
) -> Result<(), DbError> {
    for name in DEFAULT_ROOTS {
        sqlx::query(
            "INSERT INTO nodes (id, owner_id, parent_id, name, kind) VALUES ($1, $2, NULL, $3, 'folder')",
        )
        .bind(Uuid::now_v7())
        .bind(owner_id)
        .bind(name)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
