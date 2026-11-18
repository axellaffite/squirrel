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

/// The `created_at` of `owner`'s newest face: a clustering run's watermark.
pub async fn newest_face(
    conn: &mut PgConnection,
    owner: Uuid,
) -> Result<Option<OffsetDateTime>, DbError> {
    Ok(
        sqlx::query_scalar("SELECT max(created_at) FROM faces WHERE owner_id = $1")
            .bind(owner)
            .fetch_one(conn)
            .await?,
    )
}

/// Up to `limit` of `owner`'s unassigned automatic faces to decide, oldest
/// first, after `after` (`created_at`, `id`): those never tried, and those
/// left waiting before `newest` (a newer face may now make them core).
pub async fn unclustered_page(
    conn: &mut PgConnection,
    owner: Uuid,
    newest: OffsetDateTime,
    after: Option<(OffsetDateTime, Uuid)>,
    limit: i64,
) -> Result<Vec<Unclustered>, DbError> {
    let (at, id) = after.unzip();
    Ok(sqlx::query_as(
        "SELECT id, score, created_at FROM faces \
          WHERE owner_id = $1 AND person_id IS NULL AND assigned_by = 'auto' \
            AND (cluster_tried_at IS NULL OR cluster_tried_at < $2) \
            AND ($3::timestamptz IS NULL OR (created_at, id) > ($3, $4)) \
          ORDER BY created_at, id LIMIT $5",
    )
    .bind(owner)
    .bind(newest)
    .bind(at)
    .bind(id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// Records that `faces` were left waiting by a run whose watermark is
/// `newest`.
pub async fn mark_waiting(
    conn: &mut PgConnection,
    owner: Uuid,
    faces: &[Uuid],
    newest: OffsetDateTime,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE faces SET cluster_tried_at = $3 \
          WHERE owner_id = $1 AND id = ANY($2) AND person_id IS NULL AND assigned_by = 'auto'",
    )
    .bind(owner)
    .bind(faces)
    .bind(newest)
    .execute(conn)
    .await?;
    Ok(())
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

/// Deletes `owner`'s unnamed, unhidden people left without a face (a named or
/// hidden person stays, so a rename or a hide survives a re-scan); returns
/// how many.
pub async fn delete_empty_people(conn: &mut PgConnection, owner: Uuid) -> Result<u64, DbError> {
    Ok(sqlx::query(
        "DELETE FROM people p WHERE p.owner_id = $1 AND p.name IS NULL AND NOT p.hidden \
           AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.owner_id = $1 AND f.person_id = p.id)",
    )
    .bind(owner)
    .execute(conn)
    .await?
    .rows_affected())
}
