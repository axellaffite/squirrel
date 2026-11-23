//! Comments: one flat thread per folder, file or album, visible to everyone
//! who can see the subject. Callers check access with [`crate::access`]
//! before calling these functions.

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shares::Subject;
use crate::{DbError, Page};

/// Longest comment, in characters (also a CHECK in the schema).
pub const MAX_BODY_CHARS: usize = 4000;

#[derive(Debug, Clone, FromRow)]
pub struct CommentRecord {
    pub id: Uuid,
    /// The subject's owner.
    pub owner_id: Uuid,
    /// `None` once the author's account was deleted.
    pub author_id: Option<Uuid>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub body: String,
    pub created_at: OffsetDateTime,
    pub edited_at: Option<OffsetDateTime>,
}

/// Where a thread page resumes: after this comment, in posting order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentCursor {
    pub created_at: OffsetDateTime,
    pub id: Uuid,
}

const SELECT: &str = "SELECT c.id, c.owner_id, c.author_id, u.username AS author_username, \
            u.display_name AS author_display_name, c.body, c.created_at, c.edited_at \
       FROM comments c LEFT JOIN users u ON u.id = c.author_id";

fn subject_ids(subject: Subject) -> (Option<Uuid>, Option<Uuid>) {
    match subject {
        Subject::Node(id) => (Some(id), None),
        Subject::Album(id) => (None, Some(id)),
    }
}

/// A page of the thread, oldest first.
pub async fn list(
    pool: &PgPool,
    subject: Subject,
    after: Option<CommentCursor>,
    limit: u32,
) -> Result<Page<CommentRecord>, DbError> {
    let (node_id, album_id) = subject_ids(subject);
    let subject_filter = if node_id.is_some() {
        "c.node_id = $1 AND $2::uuid IS NULL"
    } else {
        "c.album_id = $2 AND $1::uuid IS NULL"
    };
    let sql = format!(
        "{SELECT} WHERE {subject_filter} \
           AND ($3::timestamptz IS NULL OR (c.created_at, c.id) > ($3, $4)) \
         ORDER BY c.created_at, c.id LIMIT $5"
    );
    let rows = sqlx::query_as::<_, CommentRecord>(&sql)
        .bind(node_id)
        .bind(album_id)
        .bind(after.map(|c| c.created_at))
        .bind(after.map(|c| c.id))
        .bind(i64::from(limit) + 1)
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// Adds `author_id`'s comment to the thread of `subject`, owned by `owner_id`.
pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    subject: Subject,
    author_id: Uuid,
    body: &str,
) -> Result<CommentRecord, DbError> {
    let (node_id, album_id) = subject_ids(subject);
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO comments (id, owner_id, node_id, album_id, author_id, body) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(owner_id)
    .bind(node_id)
    .bind(album_id)
    .bind(author_id)
    .bind(body)
    .execute(pool)
    .await?;
    find(pool, id).await?.ok_or(DbError::NotFound)
}

pub async fn find(pool: &PgPool, id: Uuid) -> Result<Option<CommentRecord>, DbError> {
    let sql = format!("{SELECT} WHERE c.id = $1");
    Ok(sqlx::query_as::<_, CommentRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// The subject a comment belongs to.
pub async fn subject_of(pool: &PgPool, id: Uuid) -> Result<Option<Subject>, DbError> {
    let row: Option<(Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as("SELECT node_id, album_id FROM comments WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|ids| match ids {
        (Some(node), _) => Some(Subject::Node(node)),
        (None, Some(album)) => Some(Subject::Album(album)),
        (None, None) => None,
    }))
}

/// Replaces the body of `author_id`'s own comment. `false`: not theirs.
pub async fn edit(pool: &PgPool, author_id: Uuid, id: Uuid, body: &str) -> Result<bool, DbError> {
    let updated = sqlx::query(
        "UPDATE comments SET body = $3, edited_at = now() WHERE id = $1 AND author_id = $2",
    )
    .bind(id)
    .bind(author_id)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Deletes a comment written by `user_id`, or any comment on `user_id`'s own
/// subject. `false`: neither.
pub async fn delete(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, DbError> {
    let deleted =
        sqlx::query("DELETE FROM comments WHERE id = $1 AND (author_id = $2 OR owner_id = $2)")
            .bind(id)
            .bind(user_id)
            .execute(pool)
            .await?;
    Ok(deleted.rows_affected() == 1)
}

/// Number of comments on each of `node_ids` that has any.
pub async fn counts_for_nodes(
    pool: &PgPool,
    node_ids: &[Uuid],
) -> Result<Vec<(Uuid, i64)>, DbError> {
    if node_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as(
        "SELECT node_id, count(*) FROM comments WHERE node_id = ANY($1) GROUP BY node_id",
    )
    .bind(node_ids)
    .fetch_all(pool)
    .await?)
}
