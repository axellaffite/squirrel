//! Albums: collections of timeline photos.
//!
//! `album_items.owner_id` is the photo's owner: an album shared for writing
//! holds photos of several people (each can remove only theirs; the album's
//! owner can remove any). Functions taking the album's `owner_id` filter on it;
//! the others expect the caller to have checked access ([`crate::access`]).
//! Trashed photos are hidden from albums (joins on live nodes) and come back
//! on restore.
//!
//! A dynamic album ([`crate::album_rules`]) has no items: its photos are its
//! owner's photos matching its rule, so adding, removing and contributing do
//! not apply to it.

use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use squirrel_core::AlbumName;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::album_rules::{self, AlbumRules, RULE_MATCHES};
use crate::timeline::{AssetSummaryRecord, FROM, SUMMARY_COLUMNS, TimelineCursor};
use crate::{DbError, Page};

/// Albums listed at most.
pub const MAX_ALBUMS: i64 = 1000;

/// Live, visible items of album `al`, joined to their assets.
const LIVE_ITEMS: &str = "album_items i \
     JOIN assets a ON a.node_id = i.node_id AND a.owner_id = i.owner_id \
     JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL AND a.hidden_at IS NULL \
     WHERE i.album_id = al.id";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AlbumRecord {
    pub id: Uuid,
    pub name: String,
    /// The chosen cover, if any (it may be trashed: see `cover_id`).
    pub cover_node_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// Whether its photos come from a rule ([`crate::album_rules`]).
    pub dynamic: bool,
    /// Live items (matching photos for a dynamic album).
    pub item_count: i64,
    /// The cover to show: the chosen one while it is a live item, else the
    /// newest live item; `None` for an empty album.
    pub cover_id: Option<Uuid>,
}

fn album_select() -> String {
    let matches = format!("{FROM} WHERE {RULE_MATCHES}");
    format!(
        "SELECT al.id, al.name, al.cover_node_id, al.created_at, al.updated_at, \
                (r.album_id IS NOT NULL) AS dynamic, \
                CASE WHEN r.album_id IS NULL THEN (SELECT count(*) FROM {LIVE_ITEMS}) \
                     ELSE (SELECT count(*) {matches}) END AS item_count, \
                CASE WHEN r.album_id IS NULL THEN COALESCE( \
                  (SELECT a.node_id FROM {LIVE_ITEMS} AND a.node_id = al.cover_node_id), \
                  (SELECT a.node_id FROM {LIVE_ITEMS} \
                    ORDER BY a.taken_at DESC, a.node_id DESC LIMIT 1)) \
                ELSE COALESCE( \
                  (SELECT a.node_id {matches} AND a.node_id = al.cover_node_id), \
                  (SELECT a.node_id {matches} \
                    ORDER BY a.taken_at DESC, a.node_id DESC LIMIT 1)) END AS cover_id \
           FROM albums al LEFT JOIN album_rules r ON r.album_id = al.id"
    )
}

/// `owner_id`'s albums, most recently changed first (at most [`MAX_ALBUMS`]).
pub async fn list(pool: &PgPool, owner_id: Uuid) -> Result<Vec<AlbumRecord>, DbError> {
    let sql = format!(
        "{} WHERE al.owner_id = $1 ORDER BY al.updated_at DESC, al.id DESC LIMIT $2",
        album_select()
    );
    Ok(sqlx::query_as::<_, AlbumRecord>(&sql)
        .bind(owner_id)
        .bind(MAX_ALBUMS)
        .fetch_all(pool)
        .await?)
}

/// One of `owner_id`'s albums.
pub async fn find(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<Option<AlbumRecord>, DbError> {
    let sql = format!("{} WHERE al.owner_id = $1 AND al.id = $2", album_select());
    Ok(sqlx::query_as::<_, AlbumRecord>(&sql)
        .bind(owner_id)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Album `id` whoever owns it: the caller checked access.
pub async fn find_any(pool: &PgPool, id: Uuid) -> Result<Option<AlbumRecord>, DbError> {
    let sql = format!("{} WHERE al.id = $1", album_select());
    Ok(sqlx::query_as::<_, AlbumRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Albums `ids` whoever owns them (the caller checked access), in no order.
pub async fn find_many(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<AlbumRecord>, DbError> {
    let sql = format!("{} WHERE al.id = ANY($1)", album_select());
    Ok(sqlx::query_as::<_, AlbumRecord>(&sql)
        .bind(ids)
        .fetch_all(pool)
        .await?)
}

/// Whether `id` is one of `owner_id`'s albums (cheaper than [`find`]).
pub async fn exists(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<bool, DbError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM albums WHERE id = $1 AND owner_id = $2)")
            .bind(id)
            .bind(owner_id)
            .fetch_one(pool)
            .await?,
    )
}

/// Creates an album holding those of `node_ids` that are the owner's live photos.
pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    name: &AlbumName,
    node_ids: &[Uuid],
) -> Result<Uuid, DbError> {
    let id = Uuid::now_v7();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO albums (id, owner_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(owner_id)
        .bind(name.as_str())
        .execute(&mut *tx)
        .await?;
    insert_items(&mut tx, id, owner_id, node_ids).await?;
    tx.commit().await?;
    Ok(id)
}

/// Creates a dynamic album holding the owner's photos matching `rules`.
pub async fn create_dynamic(
    pool: &PgPool,
    owner_id: Uuid,
    name: &AlbumName,
    rules: &AlbumRules,
) -> Result<Uuid, DbError> {
    let id = Uuid::now_v7();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO albums (id, owner_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(owner_id)
        .bind(name.as_str())
        .execute(&mut *tx)
        .await?;
    album_rules::save(&mut tx, id, owner_id, rules).await?;
    tx.commit().await?;
    Ok(id)
}

/// Locks the album row for the transaction; `NotFound` unless it is the owner's.
async fn lock(conn: &mut PgConnection, id: Uuid, owner_id: Uuid) -> Result<(), DbError> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM albums WHERE id = $1 AND owner_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(conn)
    .await?
    .map(|_| ())
    .ok_or(DbError::NotFound)
}

/// Locks album `id` whoever owns it; `NotFound` if it does not exist.
async fn lock_any(conn: &mut PgConnection, id: Uuid) -> Result<(), DbError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM albums WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .map(|_| ())
        .ok_or(DbError::NotFound)
}

async fn insert_items(
    conn: &mut PgConnection,
    album_id: Uuid,
    owner_id: Uuid,
    node_ids: &[Uuid],
) -> Result<u64, DbError> {
    if node_ids.is_empty() {
        return Ok(0);
    }
    let added = sqlx::query(
        "INSERT INTO album_items (album_id, node_id, owner_id) \
         SELECT $1, a.node_id, a.owner_id FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.owner_id = $2 AND a.node_id = ANY($3) \
         ON CONFLICT DO NOTHING",
    )
    .bind(album_id)
    .bind(owner_id)
    .bind(node_ids)
    .execute(conn)
    .await?;
    Ok(added.rows_affected())
}

async fn touch(conn: &mut PgConnection, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE albums SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Adds those of `node_ids` that are `adder_id`'s live photos and not in the
/// album yet; returns how many were added. The caller checked that `adder_id`
/// owns the album or may write to it.
pub async fn add_items(
    pool: &PgPool,
    album_id: Uuid,
    adder_id: Uuid,
    node_ids: &[Uuid],
) -> Result<u64, DbError> {
    let mut tx = pool.begin().await?;
    lock_any(&mut tx, album_id).await?;
    if album_rules::is_dynamic(&mut tx, album_id).await? {
        return Err(DbError::DynamicAlbum);
    }
    let added = insert_items(&mut tx, album_id, adder_id, node_ids).await?;
    if added > 0 {
        touch(&mut tx, album_id).await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// Who removes photos from an album.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remover {
    /// The album's owner: any photo.
    AlbumOwner,
    /// Someone it is shared with for writing: only the photos they own.
    Contributor(Uuid),
}

/// Removes `node_ids` from the album (the photos stay); returns how many were
/// removed. A removed chosen cover falls back to the newest item. The caller
/// checked access.
pub async fn remove_items(
    pool: &PgPool,
    album_id: Uuid,
    remover: Remover,
    node_ids: &[Uuid],
) -> Result<u64, DbError> {
    let mut tx = pool.begin().await?;
    lock_any(&mut tx, album_id).await?;
    if album_rules::is_dynamic(&mut tx, album_id).await? {
        return Err(DbError::DynamicAlbum);
    }
    let only_owner = match remover {
        Remover::AlbumOwner => None,
        Remover::Contributor(id) => Some(id),
    };
    let removed = sqlx::query(
        "DELETE FROM album_items WHERE album_id = $1 AND node_id = ANY($3) \
            AND ($2::uuid IS NULL OR owner_id = $2)",
    )
    .bind(album_id)
    .bind(only_owner)
    .bind(node_ids)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if removed > 0 {
        sqlx::query(
            "UPDATE albums SET updated_at = now(), \
                    cover_node_id = CASE WHEN cover_node_id = ANY($2) THEN NULL \
                                         ELSE cover_node_id END \
              WHERE id = $1",
        )
        .bind(album_id)
        .bind(node_ids)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(removed)
}

/// What [`update`] does to the chosen cover.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoverChange {
    #[default]
    Keep,
    /// No chosen cover: the newest photo is shown.
    Clear,
    /// Must be a live item of the album.
    Set(Uuid),
}

/// What [`update`] changes; `None` / `Keep` fields stay as they are.
#[derive(Debug, Clone, Default)]
pub struct AlbumUpdate {
    pub name: Option<AlbumName>,
    pub cover: CoverChange,
    /// A dynamic album's new rule.
    pub rules: Option<AlbumRules>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated,
    NotFound,
    /// The requested cover is not a live item of the album.
    CoverNotInAlbum,
    /// New rules for an album that is not dynamic.
    NotDynamic,
}

pub async fn update(
    pool: &PgPool,
    album_id: Uuid,
    owner_id: Uuid,
    changes: &AlbumUpdate,
) -> Result<UpdateOutcome, DbError> {
    let mut tx = pool.begin().await?;
    match lock(&mut tx, album_id, owner_id).await {
        Err(DbError::NotFound) => return Ok(UpdateOutcome::NotFound),
        other => other?,
    }
    let dynamic = album_rules::is_dynamic(&mut tx, album_id).await?;
    if let Some(rules) = &changes.rules {
        if !dynamic {
            return Ok(UpdateOutcome::NotDynamic);
        }
        album_rules::save(&mut tx, album_id, owner_id, rules).await?;
    }
    if let CoverChange::Set(cover) = changes.cover {
        // Checked after the rule changed: the cover must match the new one.
        let sql = if dynamic {
            format!(
                "SELECT EXISTS (SELECT 1 {FROM} JOIN album_rules r ON r.album_id = $1 \
                                 WHERE a.owner_id = $2 AND a.node_id = $3 AND {RULE_MATCHES})"
            )
        } else {
            "SELECT EXISTS (SELECT 1 FROM album_items i \
                              JOIN nodes n ON n.id = i.node_id AND n.deleted_at IS NULL \
                             WHERE i.album_id = $1 AND i.owner_id = $2 AND i.node_id = $3)"
                .to_owned()
        };
        let is_item: bool = sqlx::query_scalar(&sql)
            .bind(album_id)
            .bind(owner_id)
            .bind(cover)
            .fetch_one(&mut *tx)
            .await?;
        if !is_item {
            return Ok(UpdateOutcome::CoverNotInAlbum);
        }
    }
    let (change_cover, cover) = match changes.cover {
        CoverChange::Keep => (false, None),
        CoverChange::Clear => (true, None),
        CoverChange::Set(id) => (true, Some(id)),
    };
    sqlx::query(
        "UPDATE albums SET name = COALESCE($2, name), \
                cover_node_id = CASE WHEN $4 THEN $3 ELSE cover_node_id END, \
                updated_at = now() \
          WHERE id = $1",
    )
    .bind(album_id)
    .bind(changes.name.as_ref().map(AlbumName::as_str))
    .bind(cover)
    .bind(change_cover)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(UpdateOutcome::Updated)
}

/// Deletes the album (its photos stay). `false`: not one of the owner's albums.
pub async fn delete(pool: &PgPool, album_id: Uuid, owner_id: Uuid) -> Result<bool, DbError> {
    let deleted = sqlx::query("DELETE FROM albums WHERE id = $1 AND owner_id = $2")
        .bind(album_id)
        .bind(owner_id)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

/// A page of the album's live photos (of every contributor), newest capture
/// first. Callers check access to the album first ([`crate::access::album`]).
pub async fn items(
    pool: &PgPool,
    album_id: Uuid,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<AssetSummaryRecord>, DbError> {
    let mut query = match album_rules::owner_of(pool, album_id).await? {
        // The owner is bound too, so the keyset stays on the timeline index.
        Some(owner_id) => {
            let mut query = QueryBuilder::<Postgres>::new(format!(
                "SELECT {SUMMARY_COLUMNS} {FROM} JOIN album_rules r ON r.album_id = "
            ));
            query
                .push_bind(album_id)
                .push(" WHERE a.owner_id = ")
                .push_bind(owner_id)
                .push(format!(" AND {RULE_MATCHES}"));
            query
        }
        None => {
            let mut query = QueryBuilder::<Postgres>::new(format!(
                "SELECT {SUMMARY_COLUMNS} {FROM} \
                   JOIN album_items i ON i.node_id = a.node_id AND i.owner_id = a.owner_id \
                  WHERE i.album_id = "
            ));
            query.push_bind(album_id);
            query
        }
    };
    if let Some(cursor) = after {
        query
            .push(" AND (a.taken_at, a.node_id) < (")
            .push_bind(cursor.taken_at)
            .push(", ")
            .push_bind(cursor.node_id)
            .push(")");
    }
    query
        .push(" ORDER BY a.taken_at DESC, a.node_id DESC LIMIT ")
        .push_bind(i64::from(limit) + 1);
    let rows = query
        .build_query_as::<AssetSummaryRecord>()
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AlbumRefRecord {
    pub id: Uuid,
    pub name: String,
}

/// The owner's albums holding `node_id` (dynamic ones it matches included),
/// by name.
pub async fn albums_for_node(
    pool: &PgPool,
    owner_id: Uuid,
    node_id: Uuid,
) -> Result<Vec<AlbumRefRecord>, DbError> {
    let sql = format!(
        "SELECT al.id, al.name FROM album_items i \
           JOIN albums al ON al.id = i.album_id AND al.owner_id = i.owner_id \
          WHERE i.owner_id = $1 AND i.node_id = $2 \
         UNION \
         SELECT al.id, al.name {FROM} \
           JOIN album_rules r ON r.owner_id = a.owner_id \
           JOIN albums al ON al.id = r.album_id \
          WHERE a.owner_id = $1 AND a.node_id = $2 AND {RULE_MATCHES} \
          ORDER BY name, id"
    );
    Ok(sqlx::query_as::<_, AlbumRefRecord>(&sql)
        .bind(owner_id)
        .bind(node_id)
        .fetch_all(pool)
        .await?)
}
