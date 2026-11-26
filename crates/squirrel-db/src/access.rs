//! Who may do what with a node or an album: its owner, or someone it is
//! shared with. Callers answer `not_found` when there is no access at all, so
//! another user's unshared item stays indistinguishable from a missing one.
//!
//! - A node is reached through a share on itself or on one of its ancestors
//!   (the strongest permission wins), or read-only as a live item of an album
//!   the caller owns or that is shared with them (for a dynamic album: a
//!   photo matching its rule).
//! - Trashed nodes are never reachable through a share.

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::DbError;
use crate::album_rules::RULE_MATCHES;
use crate::nodes::{NODE_COLUMNS, NodeRecord};
use crate::timeline::FROM;

/// Ancestor walks stop here, so a corrupted cycle cannot loop forever.
const MAX_DEPTH: i32 = 1000;

/// What the caller may do, weakest first (so `>=` compares strength).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    Read,
    Write,
    Owner,
}

impl Access {
    pub fn can_write(self) -> bool {
        self >= Self::Write
    }

    pub fn is_owner(self) -> bool {
        self == Self::Owner
    }

    pub(crate) fn from_permission(permission: &str) -> Self {
        if permission == "write" {
            Self::Write
        } else {
            Self::Read
        }
    }
}

/// A live node and what the caller may do with it.
#[derive(Debug, Clone)]
pub struct NodeAccess {
    pub node: NodeRecord,
    pub access: Access,
    /// For a share: the topmost shared ancestor (or the node itself), where the
    /// caller's view of the tree starts. `None` for the owner, and for a photo
    /// reached only through an album.
    pub share_root: Option<Uuid>,
}

#[derive(FromRow)]
struct ShareHit {
    /// 2 = write, 1 = read.
    strength: Option<i32>,
    /// The shared ancestor furthest from the node.
    root: Option<Uuid>,
}

/// The live node `id` with the caller's access, or `None` without any.
pub async fn node(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<NodeAccess>, DbError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND deleted_at IS NULL");
    let Some(node) = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    if node.owner_id == user_id {
        return Ok(Some(NodeAccess {
            node,
            access: Access::Owner,
            share_root: None,
        }));
    }
    let hit = sqlx::query_as::<_, ShareHit>(
        "WITH RECURSIVE chain (id, parent_id, depth) AS ( \
             SELECT id, parent_id, 0 FROM nodes WHERE id = $1 \
           UNION ALL \
             SELECT n.id, n.parent_id, c.depth + 1 \
               FROM nodes n JOIN chain c ON n.id = c.parent_id \
              WHERE c.depth < $3 \
         ) \
         SELECT max(CASE s.permission WHEN 'write' THEN 2 ELSE 1 END) AS strength, \
                (array_agg(c.id ORDER BY c.depth DESC))[1] AS root \
           FROM chain c JOIN shares s ON s.node_id = c.id AND s.recipient_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .bind(MAX_DEPTH)
    .fetch_one(pool)
    .await?;
    if let Some(strength) = hit.strength {
        let access = if strength >= 2 {
            Access::Write
        } else {
            Access::Read
        };
        return Ok(Some(NodeAccess {
            node,
            access,
            share_root: hit.root,
        }));
    }
    if in_visible_album(pool, user_id, id).await? {
        return Ok(Some(NodeAccess {
            node,
            access: Access::Read,
            share_root: None,
        }));
    }
    Ok(None)
}

/// Whether `node_id` is a live, visible photo of an album `user_id` owns or
/// that is shared with them.
async fn in_visible_album(pool: &PgPool, user_id: Uuid, node_id: Uuid) -> Result<bool, DbError> {
    let sql = format!(
        "SELECT EXISTS ( \
           SELECT 1 FROM album_items i \
             JOIN assets a ON a.node_id = i.node_id AND a.owner_id = i.owner_id \
                          AND a.hidden_at IS NULL \
             JOIN albums al ON al.id = i.album_id \
            WHERE i.node_id = $1 \
              AND (al.owner_id = $2 \
                   OR EXISTS (SELECT 1 FROM shares s \
                               WHERE s.album_id = al.id AND s.recipient_id = $2))) \
         OR EXISTS ( \
           SELECT 1 {FROM} \
             JOIN album_rules r ON r.owner_id = a.owner_id \
             JOIN albums al ON al.id = r.album_id \
            WHERE a.node_id = $1 AND {RULE_MATCHES} \
              AND (al.owner_id = $2 \
                   OR EXISTS (SELECT 1 FROM shares s \
                               WHERE s.album_id = al.id AND s.recipient_id = $2)))"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(node_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?)
}

/// An album and what the caller may do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumAccess {
    pub owner_id: Uuid,
    pub access: Access,
}

/// The caller's access to album `id`, or `None` without any.
pub async fn album(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<AlbumAccess>, DbError> {
    #[derive(FromRow)]
    struct Row {
        owner_id: Uuid,
        permission: Option<String>,
    }
    let row = sqlx::query_as::<_, Row>(
        "SELECT al.owner_id, s.permission FROM albums al \
           LEFT JOIN shares s ON s.album_id = al.id AND s.recipient_id = $2 \
          WHERE al.id = $1",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|row| {
        let access = if row.owner_id == user_id {
            Some(Access::Owner)
        } else {
            row.permission.as_deref().map(Access::from_permission)
        }?;
        Some(AlbumAccess {
            owner_id: row.owner_id,
            access,
        })
    }))
}
