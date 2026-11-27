//! Public links: one of an owner's folders, files or albums, readable by
//! anyone holding the link's token. Only the token's SHA-256 is stored.
//!
//! A link is active while it has not expired, its owner is not disabled and,
//! for a folder or a file, its node is live (not trashed). Visitors reach a
//! node only through [`node_in_link`].

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::album_rules::RULE_MATCHES;
use crate::nodes::{NODE_COLUMNS, NodeRecord};
use crate::shares::Subject;
use crate::timeline::FROM;

/// Links listed at most per item.
pub const MAX_LINKS: i64 = 200;
/// Ancestor walks stop here, so a corrupted cycle cannot loop forever.
const MAX_DEPTH: i32 = 1000;

const COLUMNS: &str = "l.id, l.owner_id, l.node_id, l.album_id, l.password_hash, l.expires_at, \
     l.created_at";

#[derive(Clone, FromRow)]
pub struct PublicLinkRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub node_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
    /// Argon2id PHC string; `None` without a password.
    pub password_hash: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

impl std::fmt::Debug for PublicLinkRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicLinkRecord")
            .field("id", &self.id)
            .field("owner_id", &self.owner_id)
            .field("node_id", &self.node_id)
            .field("album_id", &self.album_id)
            .field("has_password", &self.password_hash.is_some())
            .field("expires_at", &self.expires_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl PublicLinkRecord {
    pub fn subject(&self) -> Subject {
        match (self.node_id, self.album_id) {
            (Some(node), _) => Subject::Node(node),
            (None, Some(album)) => Subject::Album(album),
            // The table's check constraint makes exactly one of them set.
            (None, None) => unreachable!("public_links_one_subject"),
        }
    }

    pub fn has_password(&self) -> bool {
        self.password_hash.is_some()
    }
}

/// An active link and who shared it.
#[derive(Debug, Clone, FromRow)]
pub struct ActiveLink {
    #[sqlx(flatten)]
    pub link: PublicLinkRecord,
    pub owner_display_name: String,
}

pub struct NewPublicLink<'a> {
    pub owner_id: Uuid,
    pub subject: Subject,
    pub token_hash: &'a [u8],
    pub password_hash: Option<&'a str>,
    pub expires_at: Option<OffsetDateTime>,
}

/// Creates a link to `subject` (one of `owner_id`'s, checked by the caller).
pub async fn create(pool: &PgPool, new: &NewPublicLink<'_>) -> Result<PublicLinkRecord, DbError> {
    let (node_id, album_id) = match new.subject {
        Subject::Node(id) => (Some(id), None),
        Subject::Album(id) => (None, Some(id)),
    };
    let sql = format!(
        "INSERT INTO public_links AS l \
                (id, owner_id, node_id, album_id, token_hash, password_hash, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COLUMNS}"
    );
    Ok(sqlx::query_as::<_, PublicLinkRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.owner_id)
        .bind(node_id)
        .bind(album_id)
        .bind(new.token_hash)
        .bind(new.password_hash)
        .bind(new.expires_at)
        .fetch_one(pool)
        .await?)
}

/// The links of `subject`, newest first, expired ones included.
pub async fn for_subject(
    pool: &PgPool,
    subject: Subject,
) -> Result<Vec<PublicLinkRecord>, DbError> {
    let filter = match subject {
        Subject::Node(_) => "l.node_id = $1",
        Subject::Album(_) => "l.album_id = $1",
    };
    let id = match subject {
        Subject::Node(id) | Subject::Album(id) => id,
    };
    let sql = format!(
        "SELECT {COLUMNS} FROM public_links l WHERE {filter} \
         ORDER BY l.created_at DESC, l.id DESC LIMIT $2"
    );
    Ok(sqlx::query_as::<_, PublicLinkRecord>(&sql)
        .bind(id)
        .bind(MAX_LINKS)
        .fetch_all(pool)
        .await?)
}

/// How many links `subject` has, expired ones included.
pub async fn count_for_subject(pool: &PgPool, subject: Subject) -> Result<i64, DbError> {
    let (node_id, album_id) = match subject {
        Subject::Node(id) => (Some(id), None),
        Subject::Album(id) => (None, Some(id)),
    };
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM public_links WHERE node_id = $1 OR album_id = $2")
            .bind(node_id)
            .bind(album_id)
            .fetch_one(pool)
            .await?,
    )
}

/// `owner_id`'s link `id`, if it exists.
pub async fn find(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<PublicLinkRecord>, DbError> {
    let sql = format!("SELECT {COLUMNS} FROM public_links l WHERE l.id = $1 AND l.owner_id = $2");
    Ok(sqlx::query_as::<_, PublicLinkRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?)
}

/// Deletes `owner_id`'s link `id`; `false` when there is no such link.
pub async fn delete(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<bool, DbError> {
    let deleted = sqlx::query("DELETE FROM public_links WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(owner_id)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

/// The active link whose token hashes to `token_hash`, else `None`.
pub async fn find_active(pool: &PgPool, token_hash: &[u8]) -> Result<Option<ActiveLink>, DbError> {
    let sql = format!(
        "SELECT {COLUMNS}, u.display_name AS owner_display_name \
           FROM public_links l JOIN users u ON u.id = l.owner_id \
          WHERE l.token_hash = $1 \
            AND u.disabled_at IS NULL \
            AND (l.expires_at IS NULL OR l.expires_at > now()) \
            AND (l.node_id IS NULL OR EXISTS ( \
                  SELECT 1 FROM nodes n WHERE n.id = l.node_id AND n.deleted_at IS NULL))"
    );
    Ok(sqlx::query_as::<_, ActiveLink>(&sql)
        .bind(token_hash)
        .fetch_optional(pool)
        .await?)
}

/// The live node `id` if `link` opens it: the link's file, its folder or
/// anything below it, or a visible photo of its album. Else `None`.
pub async fn node_in_link(
    pool: &PgPool,
    link: &PublicLinkRecord,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    let reachable = match link.subject() {
        Subject::Node(root) => in_subtree(pool, link.owner_id, root, id).await?,
        Subject::Album(album) => in_album(pool, album, id).await?,
    };
    if !reachable {
        return Ok(None);
    }
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND deleted_at IS NULL");
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Whether `id` is `root` or below it in `owner_id`'s tree.
async fn in_subtree(pool: &PgPool, owner_id: Uuid, root: Uuid, id: Uuid) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE chain (id, parent_id, depth) AS ( \
             SELECT id, parent_id, 0 FROM nodes WHERE id = $1 AND owner_id = $2 \
           UNION ALL \
             SELECT n.id, n.parent_id, c.depth + 1 \
               FROM nodes n JOIN chain c ON n.id = c.parent_id \
              WHERE n.owner_id = $2 AND c.depth < $4 \
         ) \
         SELECT EXISTS (SELECT 1 FROM chain WHERE id = $3)",
    )
    .bind(id)
    .bind(owner_id)
    .bind(root)
    .bind(MAX_DEPTH)
    .fetch_one(pool)
    .await?)
}

/// Whether `node_id` is a visible photo of album `album_id` (an item, or a
/// photo matching its rule for a dynamic album).
async fn in_album(pool: &PgPool, album_id: Uuid, node_id: Uuid) -> Result<bool, DbError> {
    let sql = format!(
        "SELECT EXISTS ( \
           SELECT 1 FROM album_items i \
             JOIN assets a ON a.node_id = i.node_id AND a.owner_id = i.owner_id \
                          AND a.hidden_at IS NULL \
            WHERE i.album_id = $1 AND i.node_id = $2 \
              AND NOT EXISTS (SELECT 1 FROM album_rules r WHERE r.album_id = $1)) \
         OR EXISTS ( \
           SELECT 1 {FROM} \
             JOIN album_rules r ON r.album_id = $1 \
            WHERE a.node_id = $2 AND {RULE_MATCHES})"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(album_id)
        .bind(node_id)
        .fetch_one(pool)
        .await?)
}
