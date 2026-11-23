//! Shares: one of an owner's folders, files or albums opened to another
//! account, for reading or writing. Access itself is resolved by
//! [`crate::access`]; this module creates, lists and removes shares.

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::access::Access;

/// Shares listed at most (per subject, or shared with one person).
pub const MAX_SHARES: i64 = 1000;

/// What is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Node(Uuid),
    Album(Uuid),
}

impl Subject {
    fn node_id(self) -> Option<Uuid> {
        match self {
            Self::Node(id) => Some(id),
            Self::Album(_) => None,
        }
    }

    fn album_id(self) -> Option<Uuid> {
        match self {
            Self::Album(id) => Some(id),
            Self::Node(_) => None,
        }
    }
}

/// Read or write. The owner's own access is never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn access(self) -> Access {
        match self {
            Self::Read => Access::Read,
            Self::Write => Access::Write,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ShareRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub recipient_id: Uuid,
    pub node_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
    permission: String,
    pub created_at: OffsetDateTime,
    /// The other side: the recipient in a subject's list, the owner in
    /// "shared with me".
    pub person_username: String,
    pub person_display_name: String,
}

impl ShareRecord {
    pub fn permission(&self) -> Permission {
        if self.permission == "write" {
            Permission::Write
        } else {
            Permission::Read
        }
    }
}

const COLUMNS: &str =
    "s.id, s.owner_id, s.recipient_id, s.node_id, s.album_id, s.permission, s.created_at";

/// Shares `subject` (one of `owner_id`'s, checked by the caller) with
/// `recipient_id`, or changes the permission of the existing share.
pub async fn upsert(
    pool: &PgPool,
    owner_id: Uuid,
    subject: Subject,
    recipient_id: Uuid,
    permission: Permission,
) -> Result<Uuid, DbError> {
    if owner_id == recipient_id {
        return Err(DbError::InvalidInput("you cannot share with yourself"));
    }
    let mut tx = pool.begin().await?;
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, permission FROM shares \
          WHERE recipient_id = $1 AND (node_id = $2 OR album_id = $3) FOR UPDATE",
    )
    .bind(recipient_id)
    .bind(subject.node_id())
    .bind(subject.album_id())
    .fetch_optional(&mut *tx)
    .await?;
    let id = match existing {
        Some((id, previous)) => {
            sqlx::query("UPDATE shares SET permission = $2 WHERE id = $1")
                .bind(id)
                .bind(permission.as_str())
                .execute(&mut *tx)
                .await?;
            if previous == "write" && permission == Permission::Read {
                drop_contributions(&mut tx, subject, recipient_id).await?;
            }
            id
        }
        None => {
            let id = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO shares (id, owner_id, recipient_id, node_id, album_id, permission) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(owner_id)
            .bind(recipient_id)
            .bind(subject.node_id())
            .bind(subject.album_id())
            .bind(permission.as_str())
            .execute(&mut *tx)
            .await?;
            id
        }
    };
    tx.commit().await?;
    Ok(id)
}

/// A recipient who can no longer write to an album loses the photos they added.
async fn drop_contributions(
    conn: &mut sqlx::PgConnection,
    subject: Subject,
    recipient_id: Uuid,
) -> Result<(), DbError> {
    if let Subject::Album(album_id) = subject {
        sqlx::query("DELETE FROM album_items WHERE album_id = $1 AND owner_id = $2")
            .bind(album_id)
            .bind(recipient_id)
            .execute(&mut *conn)
            .await?;
        // A chosen cover is always the album owner's photo: nothing to clear.
        sqlx::query("UPDATE albums SET updated_at = now() WHERE id = $1")
            .bind(album_id)
            .execute(conn)
            .await?;
    }
    Ok(())
}

/// The people `subject` is shared with, in sharing order.
pub async fn for_subject(pool: &PgPool, subject: Subject) -> Result<Vec<ShareRecord>, DbError> {
    let sql = format!(
        "SELECT {COLUMNS}, u.username AS person_username, u.display_name AS person_display_name \
           FROM shares s JOIN users u ON u.id = s.recipient_id \
          WHERE s.node_id = $1 OR s.album_id = $2 \
          ORDER BY s.created_at, s.id LIMIT $3"
    );
    Ok(sqlx::query_as::<_, ShareRecord>(&sql)
        .bind(subject.node_id())
        .bind(subject.album_id())
        .bind(MAX_SHARES)
        .fetch_all(pool)
        .await?)
}

/// One share, looked up by id.
pub async fn find(pool: &PgPool, id: Uuid) -> Result<Option<ShareRecord>, DbError> {
    let sql = format!(
        "SELECT {COLUMNS}, u.username AS person_username, u.display_name AS person_display_name \
           FROM shares s JOIN users u ON u.id = s.recipient_id WHERE s.id = $1"
    );
    Ok(sqlx::query_as::<_, ShareRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Removes share `id` if `user_id` is its owner or its recipient (leaving a
/// share). `false`: no such share for them. An album recipient's photos leave
/// the album with the share.
pub async fn delete(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, DbError> {
    let mut tx = pool.begin().await?;
    let removed: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
        "DELETE FROM shares WHERE id = $1 AND (owner_id = $2 OR recipient_id = $2) \
         RETURNING recipient_id, album_id",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((recipient_id, album_id)) = removed else {
        return Ok(false);
    };
    if let Some(album_id) = album_id {
        drop_contributions(&mut tx, Subject::Album(album_id), recipient_id).await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Shares made to `recipient_id` whose subject is live (trashed folders and
/// files are left out), newest first, with the owner's names.
pub async fn shared_with(pool: &PgPool, recipient_id: Uuid) -> Result<Vec<ShareRecord>, DbError> {
    let sql = format!(
        "SELECT {COLUMNS}, u.username AS person_username, u.display_name AS person_display_name \
           FROM shares s JOIN users u ON u.id = s.owner_id \
           LEFT JOIN nodes n ON n.id = s.node_id \
          WHERE s.recipient_id = $1 AND (s.album_id IS NOT NULL OR n.deleted_at IS NULL) \
            AND u.disabled_at IS NULL \
          ORDER BY s.created_at DESC, s.id DESC LIMIT $2"
    );
    Ok(sqlx::query_as::<_, ShareRecord>(&sql)
        .bind(recipient_id)
        .bind(MAX_SHARES)
        .fetch_all(pool)
        .await?)
}

/// Of `node_ids`, those `owner_id` shares with someone (to badge them).
pub async fn shared_nodes(
    pool: &PgPool,
    owner_id: Uuid,
    node_ids: &[Uuid],
) -> Result<Vec<Uuid>, DbError> {
    if node_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT node_id FROM shares WHERE owner_id = $1 AND node_id = ANY($2)",
    )
    .bind(owner_id)
    .bind(node_ids)
    .fetch_all(pool)
    .await?)
}
