//! People (groups of one owner's faces) and the user's corrections. Every
//! correction pins the faces it touches (`assigned_by = 'user'`), so
//! clustering never moves them again. Edits share the owner's row lock, like
//! storing a scan, so they never interleave with turning faces off.

use sqlx::{FromRow, PgConnection, PgPool};
use time::PrimitiveDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::faces::lock_owner;

/// A person with its face count and cover face.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PersonRecord {
    pub id: Uuid,
    pub name: Option<String>,
    pub hidden: bool,
    pub face_count: i64,
    #[sqlx(flatten)]
    pub cover: CoverColumns,
}

/// The face shown for a person: the one the user chose, else the best-scoring
/// face on a live photo with a ready thumbnail (else any on a live photo).
/// All `None` for a person without such a face.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct CoverColumns {
    pub cover_face_id: Option<Uuid>,
    pub cover_photo_id: Option<Uuid>,
    pub cover_box_x: Option<f32>,
    pub cover_box_y: Option<f32>,
    pub cover_box_w: Option<f32>,
    pub cover_box_h: Option<f32>,
    pub cover_thumbs_version: Option<i16>,
}

const PERSON_SELECT: &str = "SELECT p.id, p.name, p.hidden, \
       (SELECT count(*) FROM faces f WHERE f.owner_id = p.owner_id AND f.person_id = p.id) AS face_count, \
       cv.id AS cover_face_id, cv.node_id AS cover_photo_id, cv.box_x AS cover_box_x, \
       cv.box_y AS cover_box_y, cv.box_w AS cover_box_w, cv.box_h AS cover_box_h, \
       cv.thumbs_version AS cover_thumbs_version \
  FROM people p \
  LEFT JOIN LATERAL ( \
    SELECT f.id, c.node_id, f.box_x, f.box_y, f.box_w, f.box_h, \
           CASE WHEN m.thumbs_state = 'ready' THEN m.thumbs_version END AS thumbs_version \
      FROM faces f \
      JOIN media m ON m.sha256 = f.sha256 \
      CROSS JOIN LATERAL (SELECT a.node_id FROM assets a \
           JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.owner_id = f.owner_id AND a.sha256 = f.sha256 \
          ORDER BY a.taken_at DESC, a.node_id DESC LIMIT 1) c \
     WHERE f.owner_id = p.owner_id AND f.person_id = p.id \
     ORDER BY f.id = p.cover_face_id DESC NULLS LAST, m.thumbs_state = 'ready' DESC, \
              f.score DESC, f.id \
     LIMIT 1) cv ON true";

/// Most people listed at once.
pub const MAX_PEOPLE: i64 = 5_000;

/// `owner`'s people, named first, then by face count.
pub async fn list(
    pool: &PgPool,
    owner: Uuid,
    include_hidden: bool,
) -> Result<Vec<PersonRecord>, DbError> {
    let sql = format!(
        "SELECT * FROM ({PERSON_SELECT} WHERE p.owner_id = $1 AND ($2 OR NOT p.hidden)) x \
          ORDER BY x.name IS NULL, x.face_count DESC, lower(x.name), x.id LIMIT $3"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(include_hidden)
        .bind(MAX_PEOPLE)
        .fetch_all(pool)
        .await?)
}

pub async fn find<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    owner: Uuid,
    id: Uuid,
) -> Result<Option<PersonRecord>, DbError> {
    let sql = format!("{PERSON_SELECT} WHERE p.owner_id = $1 AND p.id = $2");
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(id)
        .fetch_optional(executor)
        .await?)
}

/// Changes to a person; `None` keeps a field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersonUpdate {
    /// `Some(None)` clears the name.
    pub name: Option<Option<String>>,
    pub hidden: Option<bool>,
    /// `Some(None)` goes back to the best-scoring face.
    pub cover_face_id: Option<Option<Uuid>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Edit<T> {
    Done(T),
    NotFound,
    /// A face given is not one of the person's (or of the owner's).
    NotTheirs,
}

pub async fn update(
    pool: &PgPool,
    owner: Uuid,
    id: Uuid,
    changes: &PersonUpdate,
) -> Result<Edit<PersonRecord>, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner, false).await?;
    if !exists(&mut tx, owner, id).await? {
        return Ok(Edit::NotFound);
    }
    if let Some(Some(face)) = changes.cover_face_id
        && !faces_of(&mut tx, owner, id, &[face]).await?
    {
        return Ok(Edit::NotTheirs);
    }
    sqlx::query(
        "UPDATE people SET \
           name = CASE WHEN $3 THEN $4 ELSE name END, \
           hidden = COALESCE($5, hidden), \
           cover_face_id = CASE WHEN $6 THEN $7 ELSE cover_face_id END, \
           updated_at = now() \
         WHERE owner_id = $1 AND id = $2",
    )
    .bind(owner)
    .bind(id)
    .bind(changes.name.is_some())
    .bind(changes.name.clone().flatten())
    .bind(changes.hidden)
    .bind(changes.cover_face_id.is_some())
    .bind(changes.cover_face_id.flatten())
    .execute(&mut *tx)
    .await?;
    let person = find(&mut *tx, owner, id).await?.ok_or(DbError::NotFound)?;
    tx.commit().await?;
    Ok(Edit::Done(person))
}

/// Moves every face of `from` to `into`, pinned, and deletes `from`.
/// `NotFound` when any of them is not the owner's.
pub async fn merge(
    pool: &PgPool,
    owner: Uuid,
    into: Uuid,
    from: &[Uuid],
) -> Result<Edit<PersonRecord>, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner, false).await?;
    let all: Vec<Uuid> = from.iter().copied().chain([into]).collect();
    let found: i64 =
        sqlx::query_scalar("SELECT count(*) FROM people WHERE owner_id = $1 AND id = ANY($2)")
            .bind(owner)
            .bind(&all)
            .fetch_one(&mut *tx)
            .await?;
    let distinct = all.iter().collect::<std::collections::HashSet<_>>().len();
    if usize::try_from(found).ok() != Some(distinct) {
        return Ok(Edit::NotFound);
    }
    sqlx::query(
        "UPDATE faces SET person_id = $2, assigned_by = 'user' \
          WHERE owner_id = $1 AND person_id = ANY($3)",
    )
    .bind(owner)
    .bind(into)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM people WHERE owner_id = $1 AND id = ANY($2)")
        .bind(owner)
        .bind(from)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE people SET updated_at = now() WHERE owner_id = $1 AND id = $2")
        .bind(owner)
        .bind(into)
        .execute(&mut *tx)
        .await?;
    let person = find(&mut *tx, owner, into)
        .await?
        .ok_or(DbError::NotFound)?;
    tx.commit().await?;
    Ok(Edit::Done(person))
}

/// Moves `faces` (all of them `person`'s) to a new person named `name`,
/// pinned.
pub async fn split(
    pool: &PgPool,
    owner: Uuid,
    person: Uuid,
    faces: &[Uuid],
    name: Option<&str>,
) -> Result<Edit<PersonRecord>, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner, false).await?;
    if !exists(&mut tx, owner, person).await? {
        return Ok(Edit::NotFound);
    }
    if !faces_of(&mut tx, owner, person, faces).await? {
        return Ok(Edit::NotTheirs);
    }
    let new = create(&mut tx, owner, name).await?;
    pin(&mut tx, owner, faces, Some(new)).await?;
    let record = find(&mut *tx, owner, new).await?.ok_or(DbError::NotFound)?;
    tx.commit().await?;
    Ok(Edit::Done(record))
}

/// Where a face goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Assignment {
    Person(Uuid),
    NewPerson {
        name: Option<String>,
    },
    /// "Not this person": pinned to nobody, never clustered again.
    Nobody,
}

/// Pins `face` to a person, a new person or nobody. Returns the face's
/// person afterwards. An unnamed person left without faces is deleted.
pub async fn assign_face(
    pool: &PgPool,
    owner: Uuid,
    face: Uuid,
    to: &Assignment,
) -> Result<Edit<Option<Uuid>>, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner, false).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM faces WHERE owner_id = $1 AND id = $2)")
            .bind(owner)
            .bind(face)
            .fetch_one(&mut *tx)
            .await?;
    if !exists {
        return Ok(Edit::NotFound);
    }
    let person = match to {
        Assignment::Person(id) => {
            if !self::exists(&mut tx, owner, *id).await? {
                return Ok(Edit::NotTheirs);
            }
            Some(*id)
        }
        Assignment::NewPerson { name } => Some(create(&mut tx, owner, name.as_deref()).await?),
        Assignment::Nobody => None,
    };
    pin(&mut tx, owner, &[face], person).await?;
    tx.commit().await?;
    Ok(Edit::Done(person))
}

/// A face on one of the owner's photos.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct FaceRow {
    pub id: Uuid,
    /// The newest live copy of the photo.
    pub photo_id: Uuid,
    pub sha256: String,
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub score: f32,
    pub person_id: Option<Uuid>,
    pub person_name: Option<String>,
    pub assigned_by: String,
    pub taken_at: PrimitiveDateTime,
    /// The thumbnail version of the photo, once its thumbnails are ready.
    pub thumbs_version: Option<i16>,
}

const FACE_SELECT: &str = "SELECT f.id, c.node_id AS photo_id, f.sha256, f.box_x, f.box_y, \
       f.box_w, f.box_h, f.score, f.person_id, p.name AS person_name, f.assigned_by, c.taken_at, \
       CASE WHEN m.thumbs_state = 'ready' THEN m.thumbs_version END AS thumbs_version \
  FROM faces f \
  JOIN media m ON m.sha256 = f.sha256 \
  CROSS JOIN LATERAL (SELECT a.node_id, a.taken_at FROM assets a \
       JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
      WHERE a.owner_id = f.owner_id AND a.sha256 = f.sha256 \
      ORDER BY a.taken_at DESC, a.node_id DESC LIMIT 1) c \
  LEFT JOIN people p ON p.id = f.person_id AND p.owner_id = f.owner_id";

/// Faces on the owner's photo `node` (any copy of its blob), left to right.
/// `None` when the node is not one of the owner's live photos.
pub async fn faces_of_photo(
    pool: &PgPool,
    owner: Uuid,
    node: Uuid,
) -> Result<Option<Vec<FaceRow>>, DbError> {
    let sha: Option<String> = sqlx::query_scalar(
        "SELECT a.sha256 FROM assets a JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
          WHERE a.owner_id = $1 AND a.node_id = $2",
    )
    .bind(owner)
    .bind(node)
    .fetch_optional(pool)
    .await?;
    let Some(sha) = sha else {
        return Ok(None);
    };
    let sql =
        format!("{FACE_SELECT} WHERE f.owner_id = $1 AND f.sha256 = $2 ORDER BY f.box_x, f.id");
    let mut faces: Vec<FaceRow> = sqlx::query_as(&sql)
        .bind(owner)
        .bind(&sha)
        .fetch_all(pool)
        .await?;
    // On this photo, not the newest copy of it.
    for face in &mut faces {
        face.photo_id = node;
    }
    Ok(Some(faces))
}

/// Keyset position in a person's faces: newest photo first.
pub type FaceCursor = (PrimitiveDateTime, Uuid);

/// A page of `person`'s faces on live photos, newest photo first.
pub async fn faces_of_person(
    pool: &PgPool,
    owner: Uuid,
    person: Uuid,
    after: Option<FaceCursor>,
    limit: i64,
) -> Result<Vec<FaceRow>, DbError> {
    let (at, id) = after.unzip();
    let sql = format!(
        "{FACE_SELECT} WHERE f.owner_id = $1 AND f.person_id = $2 \
           AND ($3::timestamp IS NULL OR (c.taken_at, f.id) < ($3, $4)) \
         ORDER BY c.taken_at DESC, f.id DESC LIMIT $5"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(person)
        .bind(at)
        .bind(id)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// The sha and box of the owner's face `id`, for its crop.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct FaceBox {
    pub sha256: String,
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
}

pub async fn face_box(pool: &PgPool, owner: Uuid, id: Uuid) -> Result<Option<FaceBox>, DbError> {
    Ok(sqlx::query_as(
        "SELECT sha256, box_x, box_y, box_w, box_h FROM faces WHERE owner_id = $1 AND id = $2",
    )
    .bind(owner)
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// People on the owner's photo `node` (not hidden ones): named first, by name.
pub async fn people_on(
    pool: &PgPool,
    owner: Uuid,
    node: Uuid,
) -> Result<Vec<(Uuid, Option<String>)>, DbError> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT p.id, p.name, p.name IS NULL AS unnamed FROM faces f \
           JOIN people p ON p.id = f.person_id AND p.owner_id = f.owner_id AND NOT p.hidden \
          WHERE f.owner_id = $1 \
            AND f.sha256 = (SELECT a.sha256 FROM assets a WHERE a.owner_id = $1 AND a.node_id = $2) \
          ORDER BY unnamed, p.name, p.id",
    )
    .bind(owner)
    .bind(node)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, name, _): (Uuid, Option<String>, bool)| (id, name))
    .collect())
}

async fn exists(conn: &mut PgConnection, owner: Uuid, person: Uuid) -> Result<bool, DbError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM people WHERE owner_id = $1 AND id = $2)")
            .bind(owner)
            .bind(person)
            .fetch_one(conn)
            .await?,
    )
}

/// Whether every one of `faces` is `person`'s.
async fn faces_of(
    conn: &mut PgConnection,
    owner: Uuid,
    person: Uuid,
    faces: &[Uuid],
) -> Result<bool, DbError> {
    let theirs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM faces WHERE owner_id = $1 AND person_id = $2 AND id = ANY($3)",
    )
    .bind(owner)
    .bind(person)
    .bind(faces)
    .fetch_one(conn)
    .await?;
    let distinct = faces.iter().collect::<std::collections::HashSet<_>>().len();
    Ok(usize::try_from(theirs).ok() == Some(distinct))
}

async fn create(conn: &mut PgConnection, owner: Uuid, name: Option<&str>) -> Result<Uuid, DbError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO people (id, owner_id, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(owner)
    .bind(name)
    .fetch_one(conn)
    .await?)
}

async fn pin(
    conn: &mut PgConnection,
    owner: Uuid,
    faces: &[Uuid],
    person: Option<Uuid>,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE faces SET person_id = $3, assigned_by = 'user' WHERE owner_id = $1 AND id = ANY($2)",
    )
    .bind(owner)
    .bind(faces)
    .bind(person)
    .execute(&mut *conn)
    .await?;
    // A cover that left its person is no longer that person's cover.
    sqlx::query(
        "UPDATE people SET cover_face_id = NULL \
          WHERE owner_id = $1 AND cover_face_id = ANY($2) AND id IS DISTINCT FROM $3",
    )
    .bind(owner)
    .bind(faces)
    .bind(person)
    .execute(&mut *conn)
    .await?;
    // People a correction emptied go, unless named (as after clustering).
    crate::clustering::delete_empty_people(conn, owner).await?;
    Ok(())
}
