//! The queries of `cluster_faces`: an owner's unassigned automatic faces, their
//! nearest faces (exact cosine scans over that owner's faces only), and the
//! assignments the clustering decides. Faces pinned by the user
//! (`assigned_by = 'user'`) are never written here.

use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::faces::lock_owner;

/// An unassigned automatic face, in clustering order.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct Unclustered {
    pub id: Uuid,
    pub score: f32,
    pub created_at: OffsetDateTime,
}

/// One neighbour of `face`.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct NeighbourRow {
    pub face: Uuid,
    pub neighbour: Uuid,
    pub person_id: Option<Uuid>,
    pub distance: f32,
}

/// Shares `owner`'s row lock (turning faces off waits for the batch) and
/// tells whether faces are still on.
pub async fn lock_enabled(conn: &mut PgConnection, owner: Uuid) -> Result<bool, DbError> {
    lock_owner(conn, owner, false).await?;
    crate::ml_settings::faces_enabled(&mut *conn, owner).await
}

/// Up to `limit` of `owner`'s unassigned automatic faces, oldest first, after
/// `after` (`created_at`, `id`).
pub async fn unclustered_page(
    conn: &mut PgConnection,
    owner: Uuid,
    after: Option<(OffsetDateTime, Uuid)>,
    limit: i64,
) -> Result<Vec<Unclustered>, DbError> {
    let (at, id) = after.unzip();
    Ok(sqlx::query_as(
        "SELECT id, score, created_at FROM faces \
          WHERE owner_id = $1 AND person_id IS NULL AND assigned_by = 'auto' \
            AND ($2::timestamptz IS NULL OR (created_at, id) > ($2, $3)) \
          ORDER BY created_at, id LIMIT $4",
    )
    .bind(owner)
    .bind(at)
    .bind(id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// `owner`'s unassigned automatic faces created at or after `since` (a
/// clustering run compares them with the faces it went through).
pub async fn unclustered_since(
    conn: &mut PgConnection,
    owner: Uuid,
    since: OffsetDateTime,
) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM faces WHERE owner_id = $1 AND person_id IS NULL \
           AND assigned_by = 'auto' AND created_at >= $2",
    )
    .bind(owner)
    .bind(since)
    .fetch_all(conn)
    .await?)
}

/// The `k` nearest faces of each of `faces` among `owner`'s faces, leaving
/// out faces pinned to nobody. Exact scans: only this owner's rows are read.
pub async fn neighbours(
    conn: &mut PgConnection,
    owner: Uuid,
    faces: &[Uuid],
    k: i64,
) -> Result<Vec<NeighbourRow>, DbError> {
    Ok(sqlx::query_as(
        "SELECT q.id AS face, n.id AS neighbour, n.person_id, n.distance \
           FROM faces q \
           CROSS JOIN LATERAL ( \
             SELECT o.id, o.person_id, (o.embedding <=> q.embedding)::real AS distance \
               FROM faces o \
              WHERE o.owner_id = $1 AND o.id <> q.id \
                AND (o.assigned_by = 'auto' OR o.person_id IS NOT NULL) \
              ORDER BY o.embedding <=> q.embedding LIMIT $3) n \
          WHERE q.owner_id = $1 AND q.id = ANY($2) \
          ORDER BY q.id, n.distance",
    )
    .bind(owner)
    .bind(faces)
    .bind(k)
    .fetch_all(conn)
    .await?)
}

/// A new unnamed person of `owner`.
pub async fn create_person(conn: &mut PgConnection, owner: Uuid) -> Result<Uuid, DbError> {
    Ok(
        sqlx::query_scalar("INSERT INTO people (id, owner_id) VALUES ($1, $2) RETURNING id")
            .bind(Uuid::now_v7())
            .bind(owner)
            .fetch_one(conn)
            .await?,
    )
}

/// Gives `person` those of `faces` that are still unassigned and automatic;
/// returns how many.
pub async fn assign(
    conn: &mut PgConnection,
    owner: Uuid,
    faces: &[Uuid],
    person: Uuid,
) -> Result<u64, DbError> {
    Ok(sqlx::query(
        "UPDATE faces SET person_id = $3 \
          WHERE owner_id = $1 AND id = ANY($2) AND person_id IS NULL AND assigned_by = 'auto'",
    )
    .bind(owner)
    .bind(faces)
    .bind(person)
    .execute(conn)
    .await?
    .rows_affected())
}

/// Deletes `owner`'s unnamed people left without a face (a named person
/// stays, so a rename survives a re-scan); returns how many.
pub async fn delete_empty_people(conn: &mut PgConnection, owner: Uuid) -> Result<u64, DbError> {
    Ok(sqlx::query(
        "DELETE FROM people p WHERE p.owner_id = $1 AND p.name IS NULL \
           AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.owner_id = $1 AND f.person_id = p.id)",
    )
    .bind(owner)
    .execute(conn)
    .await?
    .rows_affected())
}
