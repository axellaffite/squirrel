//! Albums: per-user collections of timeline photos.
//!
//! `album_items` references both `albums (id, owner_id)` and
//! `assets (node_id, owner_id)` with the same `owner_id`, so the database
//! itself refuses an album holding another user's photo. Every function also
//! filters on the caller's `owner_id`. Trashed photos are hidden from albums
//! (joins on live nodes) and come back on restore.

use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use squirrel_core::AlbumName;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::timeline::{AssetSummaryRecord, FROM, SUMMARY_COLUMNS, TimelineCursor};
use crate::{DbError, Page};

/// Albums listed at most.
pub const MAX_ALBUMS: i64 = 1000;

/// Live items of album `al`, joined to their assets.
const LIVE_ITEMS: &str = "album_items i \
     JOIN assets a ON a.node_id = i.node_id AND a.owner_id = i.owner_id \
     JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
     WHERE i.album_id = al.id";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AlbumRecord {
    pub id: Uuid,
    pub name: String,
    /// The chosen cover, if any (it may be trashed: see `cover_id`).
    pub cover_node_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// Live items.
    pub item_count: i64,
    /// The cover to show: the chosen one while it is a live item, else the
    /// newest live item; `None` for an empty album.
    pub cover_id: Option<Uuid>,
}

fn album_select() -> String {
    format!(
        "SELECT al.id, al.name, al.cover_node_id, al.created_at, al.updated_at, \
                (SELECT count(*) FROM {LIVE_ITEMS}) AS item_count, \
                COALESCE( \
                  (SELECT a.node_id FROM {LIVE_ITEMS} AND a.node_id = al.cover_node_id), \
                  (SELECT a.node_id FROM {LIVE_ITEMS} \
                    ORDER BY a.taken_at DESC, a.node_id DESC LIMIT 1)) AS cover_id \
           FROM albums al"
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

/// Adds those of `node_ids` that are the owner's live photos and not in the
/// album yet; returns how many were added. `NotFound` for another user's album.
pub async fn add_items(
    pool: &PgPool,
    album_id: Uuid,
    owner_id: Uuid,
    node_ids: &[Uuid],
) -> Result<u64, DbError> {
    let mut tx = pool.begin().await?;
    lock(&mut tx, album_id, owner_id).await?;
    let added = insert_items(&mut tx, album_id, owner_id, node_ids).await?;
    if added > 0 {
        touch(&mut tx, album_id).await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// Removes `node_ids` from the album (the photos stay); returns how many were
/// removed. A removed chosen cover falls back to the newest item.
pub async fn remove_items(
    pool: &PgPool,
    album_id: Uuid,
    owner_id: Uuid,
    node_ids: &[Uuid],
) -> Result<u64, DbError> {
    let mut tx = pool.begin().await?;
    lock(&mut tx, album_id, owner_id).await?;
    let removed = sqlx::query(
        "DELETE FROM album_items WHERE album_id = $1 AND owner_id = $2 AND node_id = ANY($3)",
    )
    .bind(album_id)
    .bind(owner_id)
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

/// What [`update`] changes; `None` fields stay as they are.
#[derive(Debug, Clone, Default)]
pub struct AlbumUpdate {
    pub name: Option<AlbumName>,
    /// Must be a live item of the album.
    pub cover_node_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated,
    NotFound,
    /// The requested cover is not a live item of the album.
    CoverNotInAlbum,
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
    if let Some(cover) = changes.cover_node_id {
        let is_item: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM album_items i \
                              JOIN nodes n ON n.id = i.node_id AND n.deleted_at IS NULL \
                             WHERE i.album_id = $1 AND i.owner_id = $2 AND i.node_id = $3)",
        )
        .bind(album_id)
        .bind(owner_id)
        .bind(cover)
        .fetch_one(&mut *tx)
        .await?;
        if !is_item {
            return Ok(UpdateOutcome::CoverNotInAlbum);
        }
    }
    sqlx::query(
        "UPDATE albums SET name = COALESCE($2, name), \
                cover_node_id = COALESCE($3, cover_node_id), updated_at = now() \
          WHERE id = $1",
    )
    .bind(album_id)
    .bind(changes.name.as_ref().map(AlbumName::as_str))
    .bind(changes.cover_node_id)
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

/// A page of the album's live photos, newest capture first. Callers check
/// the album is the owner's first ([`find`]); another user's album is empty.
pub async fn items(
    pool: &PgPool,
    owner_id: Uuid,
    album_id: Uuid,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<AssetSummaryRecord>, DbError> {
    let mut query = QueryBuilder::<Postgres>::new(format!(
        "SELECT {SUMMARY_COLUMNS} {FROM} \
           JOIN album_items i ON i.node_id = a.node_id AND i.owner_id = a.owner_id \
          WHERE a.owner_id = "
    ));
    query.push_bind(owner_id);
    query.push(" AND i.album_id = ").push_bind(album_id);
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

/// The owner's albums holding `node_id`, by name.
pub async fn albums_for_node(
    pool: &PgPool,
    owner_id: Uuid,
    node_id: Uuid,
) -> Result<Vec<AlbumRefRecord>, DbError> {
    Ok(sqlx::query_as::<_, AlbumRefRecord>(
        "SELECT al.id, al.name FROM album_items i \
           JOIN albums al ON al.id = i.album_id AND al.owner_id = i.owner_id \
          WHERE i.owner_id = $1 AND i.node_id = $2 \
          ORDER BY al.name, al.id",
    )
    .bind(owner_id)
    .bind(node_id)
    .fetch_all(pool)
    .await?)
}
