//! Yjs state of co-edited text files (`collab_docs`, one row per file).
//!
//! No owner checks here: callers resolve access to the node first.

use sqlx::{FromRow, PgConnection, PgExecutor};
use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::DbError;
use crate::nodes::{NODE_COLUMNS, NodeRecord};

#[derive(Debug, Clone, FromRow)]
pub struct CollabDoc {
    pub node_id: Uuid,
    pub epoch: Uuid,
    pub blob_sha256: String,
    pub state: Vec<u8>,
}

pub async fn find<'e>(
    executor: impl PgExecutor<'e>,
    node_id: Uuid,
) -> Result<Option<CollabDoc>, DbError> {
    Ok(sqlx::query_as::<_, CollabDoc>(
        "SELECT node_id, epoch, blob_sha256, state FROM collab_docs WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(executor)
    .await?)
}

/// Records `state`, built from blob `blob`, under a new epoch, unless a row
/// for that same blob already exists (a concurrent opener won): the stored
/// row is returned either way. `None`: the file no longer references `blob`
/// (a save or an upload changed it meanwhile), nothing was written.
pub async fn start<'e>(
    executor: impl PgExecutor<'e> + Copy,
    node_id: Uuid,
    blob: &str,
    state: &[u8],
) -> Result<Option<CollabDoc>, DbError> {
    let inserted = sqlx::query_as::<_, CollabDoc>(
        "INSERT INTO collab_docs (node_id, epoch, blob_sha256, state) \
         SELECT $1, $2, $3, $4 \
          WHERE EXISTS (SELECT 1 FROM nodes WHERE id = $1 AND blob_sha256 = $3) \
         ON CONFLICT (node_id) DO UPDATE \
            SET epoch = EXCLUDED.epoch, blob_sha256 = EXCLUDED.blob_sha256, \
                state = EXCLUDED.state, updated_at = now() \
          WHERE collab_docs.blob_sha256 <> EXCLUDED.blob_sha256 \
         RETURNING node_id, epoch, blob_sha256, state",
    )
    .bind(node_id)
    .bind(Uuid::now_v7())
    .bind(blob)
    .bind(state)
    .fetch_optional(executor)
    .await?;
    if inserted.is_some() {
        return Ok(inserted);
    }
    Ok(find(executor, node_id)
        .await?
        .filter(|doc| doc.blob_sha256 == blob))
}

/// Stores the state matching the file's blob `blob`, keeping the epoch.
/// `false`: the row is gone or belongs to another epoch.
pub async fn save_state(
    conn: &mut PgConnection,
    node_id: Uuid,
    epoch: Uuid,
    blob: &str,
    state: &[u8],
) -> Result<bool, DbError> {
    let done = sqlx::query(
        "UPDATE collab_docs SET blob_sha256 = $3, state = $4, updated_at = now() \
         WHERE node_id = $1 AND epoch = $2",
    )
    .bind(node_id)
    .bind(epoch)
    .bind(blob)
    .bind(state)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Points live file `id` of `owner_id` at blob `blob` (`size` bytes), in place,
/// if it still references `expected`. `None`: the file is gone, trashed or was
/// changed meanwhile. Preconditions: the owner lock is held and the blob row exists.
pub async fn replace_blob(
    conn: &mut PgConnection,
    owner_id: Uuid,
    id: Uuid,
    expected: &str,
    blob: &ContentHash,
    size: i64,
) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!(
        "UPDATE nodes SET blob_sha256 = $4, size = $5, modified_at = now() \
         WHERE id = $1 AND owner_id = $2 AND kind = 'file' AND deleted_at IS NULL \
           AND blob_sha256 = $3 \
         RETURNING {NODE_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(expected)
        .bind(blob.as_str())
        .bind(size)
        .fetch_optional(conn)
        .await?)
}
