//! In-app notifications: a share made to you, or a comment on something you
//! can see. Rows cascade away with their share, comment or subject; access
//! that changed otherwise (an ancestor share revoked, a node trashed) is
//! re-checked by the caller when listing.

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shares::Subject;
use crate::{DbError, Page};

/// Ancestor walks stop here, so a corrupted cycle cannot loop forever.
const MAX_DEPTH: i32 = 1000;

/// Why someone is notified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Share,
    Comment,
}

#[derive(Debug, Clone, FromRow)]
pub struct NotificationRecord {
    pub id: Uuid,
    kind: String,
    pub node_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
    pub share_id: Option<Uuid>,
    pub comment_id: Option<Uuid>,
    /// `None` once the actor's account was deleted.
    pub actor_username: Option<String>,
    pub actor_display_name: Option<String>,
    /// The folder, file or album name.
    pub subject_name: String,
    /// `true` for a folder; `false` for a file or an album.
    pub subject_is_folder: bool,
    pub subject_mime: Option<String>,
    /// For a share: `read` or `write`.
    pub permission: Option<String>,
    /// For a comment: its current text.
    pub comment_body: Option<String>,
    pub created_at: OffsetDateTime,
    pub read_at: Option<OffsetDateTime>,
}

impl NotificationRecord {
    pub fn kind(&self) -> Kind {
        if self.kind == "share" {
            Kind::Share
        } else {
            Kind::Comment
        }
    }

    pub fn subject(&self) -> Subject {
        match (self.node_id, self.album_id) {
            (Some(node), _) => Subject::Node(node),
            (None, Some(album)) => Subject::Album(album),
            // The schema's CHECK rules this out.
            (None, None) => Subject::Album(Uuid::nil()),
        }
    }
}

/// Where an inbox page resumes: after this notification, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationCursor {
    pub created_at: OffsetDateTime,
    pub id: Uuid,
}

/// Tells the recipient of share `share_id` about it, once per share (a later
/// permission change adds nothing).
pub async fn for_share(pool: &PgPool, share_id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO notifications (id, user_id, kind, actor_id, node_id, album_id, share_id) \
         SELECT $2, s.recipient_id, 'share', s.owner_id, s.node_id, s.album_id, s.id \
           FROM shares s WHERE s.id = $1 \
         ON CONFLICT (share_id) WHERE share_id IS NOT NULL DO NOTHING",
    )
    .bind(share_id)
    .bind(Uuid::now_v7())
    .execute(pool)
    .await?;
    Ok(())
}

/// Tells everyone who can see the subject of comment `comment_id`, except its
/// author: the owner, recipients of a share on the subject or an ancestor
/// folder, and for a photo, the owners and recipients of albums holding it.
/// Disabled accounts are skipped. Returns how many were notified.
pub async fn for_comment(pool: &PgPool, comment_id: Uuid) -> Result<u64, DbError> {
    let inserted = sqlx::query(
        "WITH RECURSIVE c AS ( \
             SELECT id, owner_id, node_id, album_id, author_id FROM comments WHERE id = $1 \
         ), chain (id, parent_id, depth) AS ( \
             SELECT n.id, n.parent_id, 0 FROM nodes n JOIN c ON n.id = c.node_id \
           UNION ALL \
             SELECT n.id, n.parent_id, ch.depth + 1 \
               FROM nodes n JOIN chain ch ON n.id = ch.parent_id WHERE ch.depth < $2 \
         ), albums_of AS ( \
             SELECT c.album_id AS id FROM c WHERE c.album_id IS NOT NULL \
           UNION \
             SELECT i.album_id FROM album_items i JOIN c ON i.node_id = c.node_id \
               JOIN assets a ON a.node_id = i.node_id AND a.owner_id = i.owner_id \
                            AND a.hidden_at IS NULL \
         ), watchers (user_id) AS ( \
             SELECT owner_id FROM c \
           UNION SELECT s.recipient_id FROM shares s JOIN chain ch ON s.node_id = ch.id \
           UNION SELECT al.owner_id FROM albums al JOIN albums_of ao ON al.id = ao.id \
           UNION SELECT s.recipient_id FROM shares s JOIN albums_of ao ON s.album_id = ao.id \
         ) \
         INSERT INTO notifications (id, user_id, kind, actor_id, node_id, album_id, comment_id) \
         SELECT gen_random_uuid(), w.user_id, 'comment', c.author_id, c.node_id, \
                c.album_id, c.id \
           FROM watchers w CROSS JOIN c JOIN users u ON u.id = w.user_id \
          WHERE w.user_id IS DISTINCT FROM c.author_id AND u.disabled_at IS NULL \
         ON CONFLICT (comment_id, user_id) WHERE comment_id IS NOT NULL DO NOTHING",
    )
    .bind(comment_id)
    .bind(MAX_DEPTH)
    .execute(pool)
    .await?;
    Ok(inserted.rows_affected())
}

const SELECT: &str = "SELECT n.id, n.kind, n.node_id, n.album_id, n.share_id, n.comment_id, \
            a.username AS actor_username, a.display_name AS actor_display_name, \
            COALESCE(nd.name, al.name) AS subject_name, \
            COALESCE(nd.kind = 'folder', false) AS subject_is_folder, nd.mime AS subject_mime, \
            s.permission, c.body AS comment_body, n.created_at, n.read_at \
       FROM notifications n \
       LEFT JOIN users a ON a.id = n.actor_id \
       LEFT JOIN nodes nd ON nd.id = n.node_id \
       LEFT JOIN albums al ON al.id = n.album_id \
       LEFT JOIN shares s ON s.id = n.share_id \
       LEFT JOIN comments c ON c.id = n.comment_id";

/// A page of `user_id`'s notifications, newest first.
pub async fn list(
    pool: &PgPool,
    user_id: Uuid,
    after: Option<NotificationCursor>,
    limit: u32,
) -> Result<Page<NotificationRecord>, DbError> {
    let sql = format!(
        "{SELECT} WHERE n.user_id = $1 \
           AND ($2::timestamptz IS NULL OR (n.created_at, n.id) < ($2, $3)) \
         ORDER BY n.created_at DESC, n.id DESC LIMIT $4"
    );
    let rows = sqlx::query_as::<_, NotificationRecord>(&sql)
        .bind(user_id)
        .bind(after.map(|c| c.created_at))
        .bind(after.map(|c| c.id))
        .bind(i64::from(limit) + 1)
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// How many of `user_id`'s notifications are unread.
pub async fn unread_count(pool: &PgPool, user_id: Uuid) -> Result<i64, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?)
}

/// Marks `ids` (or, with `None`, all) of `user_id`'s notifications read.
/// Returns how many changed.
pub async fn mark_read(pool: &PgPool, user_id: Uuid, ids: Option<&[Uuid]>) -> Result<u64, DbError> {
    let updated = sqlx::query(
        "UPDATE notifications SET read_at = now() \
          WHERE user_id = $1 AND read_at IS NULL AND ($2::uuid[] IS NULL OR id = ANY($2))",
    )
    .bind(user_id)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected())
}

/// Deletes `ids` of `user_id`'s notifications (whose subject they can no
/// longer see).
pub async fn delete(pool: &PgPool, user_id: Uuid, ids: &[Uuid]) -> Result<(), DbError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("DELETE FROM notifications WHERE user_id = $1 AND id = ANY($2)")
        .bind(user_id)
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(())
}
