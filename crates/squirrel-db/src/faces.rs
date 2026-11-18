//! Faces found on a user's photos (`faces`, `face_scans`, `people`). Biometric
//! data is per owner, never shared between users even for a deduplicated
//! blob, and only ever stored for a user who has faces on. A user's row in
//! `users` is the lock that orders storing a scan against turning faces off.

use pgvector::HalfVector;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::DbError;

/// Dimensions of a face embedding (SFace).
pub const FACE_DIM: usize = 128;

/// One detected face, as the worker stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFace {
    /// Relative to the displayed image (0..1): left, top, width, height.
    pub bbox: [f32; 4],
    /// Five (x, y) points, relative like the box.
    pub landmarks: [f32; 10],
    pub score: f32,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct FaceRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub sha256: String,
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub landmarks: Vec<f32>,
    pub score: f32,
    pub person_id: Option<Uuid>,
    pub assigned_by: String,
}

/// Owners of a live asset of `sha256` who have faces on and no scan of it
/// with `model` yet: the users a `faces` run writes rows for.
pub async fn owners_to_scan(
    pool: &PgPool,
    sha256: &str,
    model: &str,
) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT a.owner_id FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.sha256 = $1 \
            AND COALESCE((SELECT s.faces_enabled FROM user_ml_settings s \
                           WHERE s.user_id = a.owner_id), true) \
            AND NOT EXISTS (SELECT 1 FROM face_scans f \
                             WHERE f.owner_id = a.owner_id AND f.sha256 = a.sha256 \
                               AND f.model = $2) \
          ORDER BY 1",
    )
    .bind(sha256)
    .bind(model)
    .fetch_all(pool)
    .await?)
}

/// Whether any owner of a live asset of `sha256` has faces on (the
/// thumbnails step queues `faces` only then).
pub async fn wanted<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    sha256: &str,
) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.sha256 = $1 \
            AND COALESCE((SELECT s.faces_enabled FROM user_ml_settings s \
                           WHERE s.user_id = a.owner_id), true))",
    )
    .bind(sha256)
    .fetch_one(executor)
    .await?)
}

/// Whether `owner` has faces on, a live copy of `sha256` and no scan of it
/// by `model` (a copy that appears after the thumbnails step queued `faces`).
pub async fn owner_needs_scan(
    conn: &mut PgConnection,
    owner: Uuid,
    sha256: &str,
    model: &str,
) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.owner_id = $1 AND a.sha256 = $2 \
            AND COALESCE((SELECT s.faces_enabled FROM user_ml_settings s \
                           WHERE s.user_id = a.owner_id), true) \
            AND NOT EXISTS (SELECT 1 FROM face_scans f \
                             WHERE f.owner_id = a.owner_id AND f.sha256 = a.sha256 \
                               AND f.model = $3))",
    )
    .bind(owner)
    .bind(sha256)
    .bind(model)
    .fetch_one(&mut *conn)
    .await?)
}

/// Rendered photos of trash entry `trash_root` (a batch about to be
/// restored) that `owner`, with faces on, has no scan of by `model`.
pub async fn unscanned_in_batch(
    conn: &mut PgConnection,
    owner: Uuid,
    trash_root: Uuid,
    model: &str,
) -> Result<Vec<String>, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT a.sha256 FROM nodes n \
           JOIN assets a ON a.node_id = n.id \
           JOIN media m ON m.sha256 = a.sha256 AND m.thumbs_state = 'ready' \
          WHERE n.owner_id = $1 AND n.trash_root = $2 \
            AND COALESCE((SELECT s.faces_enabled FROM user_ml_settings s \
                           WHERE s.user_id = $1), true) \
            AND NOT EXISTS (SELECT 1 FROM face_scans f \
                             WHERE f.owner_id = $1 AND f.sha256 = a.sha256 AND f.model = $3) \
          ORDER BY 1",
    )
    .bind(owner)
    .bind(trash_root)
    .bind(model)
    .fetch_all(&mut *conn)
    .await?)
}

/// Locks `owner`'s row: storing a scan shares it, turning faces off takes it
/// exclusively, so neither interleaves with the other. Deleting an asset holds
/// it too (`quota::lock_owner`), which the `assets_drop_faces` trigger relies on.
pub(crate) async fn lock_owner(
    conn: &mut PgConnection,
    owner: Uuid,
    exclusive: bool,
) -> Result<(), DbError> {
    let sql = if exclusive {
        "SELECT 1 FROM users WHERE id = $1 FOR UPDATE"
    } else {
        "SELECT 1 FROM users WHERE id = $1 FOR SHARE"
    };
    sqlx::query(sql).bind(owner).execute(&mut *conn).await?;
    Ok(())
}

/// Stores `owner`'s scan of `sha256` (and its faces) in one transaction.
/// `false` when nothing was written: faces are off for the owner by now, the
/// owner no longer has any copy of the photo (deleted while it was being
/// analysed: no biometric data may outlive it), or the scan already exists.
pub async fn store_scan(
    pool: &PgPool,
    owner: Uuid,
    sha256: &str,
    model: &str,
    faces: &[NewFace],
) -> Result<bool, DbError> {
    let mut tx = pool.begin().await?;
    let stored = store_scan_in(&mut tx, owner, sha256, model, faces).await?;
    tx.commit().await?;
    Ok(stored)
}

/// [`store_scan`] in the caller's transaction (whatever else the caller
/// queues with the scan commits with it, or not at all).
pub async fn store_scan_in(
    tx: &mut PgConnection,
    owner: Uuid,
    sha256: &str,
    model: &str,
    faces: &[NewFace],
) -> Result<bool, DbError> {
    lock_owner(tx, owner, false).await?;
    let enabled = crate::ml_settings::faces_enabled(&mut *tx, owner).await?;
    if !enabled {
        return Ok(false);
    }
    let still_owned: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM assets WHERE owner_id = $1 AND sha256 = $2)",
    )
    .bind(owner)
    .bind(sha256)
    .fetch_one(&mut *tx)
    .await?;
    if !still_owned {
        return Ok(false);
    }
    let count = i16::try_from(faces.len()).map_err(|_| DbError::InvalidInput("too many faces"))?;
    let inserted = sqlx::query(
        "INSERT INTO face_scans (owner_id, sha256, model, face_count) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (owner_id, sha256) DO NOTHING",
    )
    .bind(owner)
    .bind(sha256)
    .bind(model)
    .bind(count)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(false);
    }
    for face in faces {
        sqlx::query(
            "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, \
                                landmarks, score, embedding) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(Uuid::now_v7())
        .bind(owner)
        .bind(sha256)
        .bind(model)
        .bind(face.bbox[0])
        .bind(face.bbox[1])
        .bind(face.bbox[2])
        .bind(face.bbox[3])
        .bind(face.landmarks.to_vec())
        .bind(face.score)
        .bind(HalfVector::from_f32_slice(&face.embedding))
        .execute(&mut *tx)
        .await?;
    }
    Ok(true)
}

/// `owner`'s faces on `sha256`, left to right.
pub async fn faces_of(
    pool: &PgPool,
    owner: Uuid,
    sha256: &str,
) -> Result<Vec<FaceRecord>, DbError> {
    Ok(sqlx::query_as(
        "SELECT id, owner_id, sha256, box_x, box_y, box_w, box_h, landmarks, score, \
                person_id, assigned_by \
           FROM faces WHERE owner_id = $1 AND sha256 = $2 ORDER BY box_x, id",
    )
    .bind(owner)
    .bind(sha256)
    .fetch_all(pool)
    .await?)
}

/// `owner`'s scan of `sha256`: how many faces it found, if it exists.
pub async fn scan_count(pool: &PgPool, owner: Uuid, sha256: &str) -> Result<Option<i16>, DbError> {
    Ok(
        sqlx::query_scalar("SELECT face_count FROM face_scans WHERE owner_id = $1 AND sha256 = $2")
            .bind(owner)
            .bind(sha256)
            .fetch_optional(pool)
            .await?,
    )
}

/// Turns faces off for `owner` and deletes all their face data (faces,
/// people, scans) and their queued clustering and face backfill jobs, in one
/// transaction. Returns what was deleted. Other users are untouched.
pub async fn turn_off(pool: &PgPool, owner: Uuid) -> Result<FaceCounts, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner, true).await?;
    crate::ml_settings::set_faces_enabled(&mut *tx, owner, false).await?;
    let faces = sqlx::query("DELETE FROM faces WHERE owner_id = $1")
        .bind(owner)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let people = sqlx::query("DELETE FROM people WHERE owner_id = $1")
        .bind(owner)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query("DELETE FROM face_scans WHERE owner_id = $1")
        .bind(owner)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM jobs WHERE status = 'queued' \
            AND ((kind = 'cluster_faces' AND dedupe_key IN ('cluster_faces:' || $1::text, \
                                                       'cluster_faces:' || $1::text || ':next')) \
              OR (kind = 'face_backfill' AND (dedupe_key = 'face_backfill:' || $1::text \
                                           OR dedupe_key LIKE 'face_backfill:' || $1::text || ':%')))",
    )
    .bind(owner)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(FaceCounts {
        faces: i64::try_from(faces).unwrap_or(i64::MAX),
        people: i64::try_from(people).unwrap_or(i64::MAX),
    })
}

/// `owner`'s next page of photos without a scan with `model` (ready
/// previews, live copies), after node `after`: (node id, sha256).
pub async fn unscanned_page(
    pool: &PgPool,
    owner: Uuid,
    model: &str,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<(Uuid, String)>, DbError> {
    Ok(sqlx::query_as(
        "SELECT a.node_id, a.sha256 FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
           JOIN media m ON m.sha256 = a.sha256 AND m.kind = 'image' AND m.thumbs_state = 'ready' \
          WHERE a.owner_id = $1 AND a.node_id > $2 \
            AND NOT EXISTS (SELECT 1 FROM face_scans f \
                             WHERE f.owner_id = a.owner_id AND f.sha256 = a.sha256 \
                               AND f.model = $3) \
          ORDER BY a.node_id LIMIT $4",
    )
    .bind(owner)
    .bind(after.unwrap_or(Uuid::nil()))
    .bind(model)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// How much face data `owner` has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, FromRow)]
pub struct FaceCounts {
    pub faces: i64,
    pub people: i64,
}

pub async fn counts(pool: &PgPool, owner: Uuid) -> Result<FaceCounts, DbError> {
    Ok(sqlx::query_as(
        "SELECT (SELECT count(*) FROM faces WHERE owner_id = $1) AS faces, \
                (SELECT count(*) FROM people WHERE owner_id = $1) AS people",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?)
}
