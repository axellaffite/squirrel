//! Duplicates: per-owner groups of near-identical photos. A photo whose
//! perceptual hash (`media.phash`) is within [`MAX_DISTANCE`] bits of a
//! group's anchor, taken within [`WINDOW_HOURS`] of it, belongs to that group.
//! Groups never mix users; a photo is in at most one group.

pub mod review;

use sqlx::{FromRow, PgConnection, PgExecutor};
use time::PrimitiveDateTime;
use uuid::Uuid;

use crate::DbError;

/// Largest Hamming distance (of 64 bits) between a photo's hash and its
/// group's anchor. Provisional: set on generated pictures, to be calibrated on
/// real photos.
pub const MAX_DISTANCE: i64 = 6;
/// How far apart in capture time two photos of one group may be.
pub const WINDOW_HOURS: i32 = 24;

/// Hamming distance between `m.phash` and the bound hash `$n`, in SQL.
fn distance_sql(column: &str, bind: &str) -> String {
    format!("bit_count(({column} # {bind})::bit(64))")
}

/// The hash bits as stored (BIGINT).
pub fn to_db(phash: u64) -> i64 {
    phash as i64
}

/// The hash bits as computed.
pub fn from_db(phash: i64) -> u64 {
    phash as u64
}

/// Records `sha256`'s perceptual hash; `false` if the blob is gone.
pub async fn set_phash<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
    phash: u64,
) -> Result<bool, DbError> {
    let updated = sqlx::query("UPDATE media SET phash = $2, updated_at = now() WHERE sha256 = $1")
        .bind(sha256)
        .bind(to_db(phash))
        .execute(executor)
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// What every grouping query requires of a photo: live, hashed, and with its
/// capture time settled (metadata read).
const GROUPABLE: &str = "FROM assets a \
     JOIN nodes n ON n.id = a.node_id AND n.owner_id = a.owner_id AND n.deleted_at IS NULL \
     JOIN media m ON m.sha256 = a.sha256 \
                 AND m.phash IS NOT NULL AND m.metadata_state <> 'pending'";

/// Owners with a photo of `sha256` the grouping job has not looked at yet.
pub async fn owners_to_group(conn: &mut PgConnection, sha256: &str) -> Result<Vec<Uuid>, DbError> {
    let sql = format!(
        "SELECT DISTINCT a.owner_id {GROUPABLE} WHERE a.sha256 = $1 AND NOT a.duplicates_checked"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(sha256)
        .fetch_all(conn)
        .await?)
}

/// Whether `owner` has photos the grouping job has not looked at yet.
pub async fn needs_grouping(conn: &mut PgConnection, owner: Uuid) -> Result<bool, DbError> {
    let sql = format!(
        "SELECT EXISTS (SELECT 1 {GROUPABLE} WHERE a.owner_id = $1 AND NOT a.duplicates_checked)"
    );
    Ok(sqlx::query_scalar(&sql).bind(owner).fetch_one(conn).await?)
}

/// Owners with photos waiting for the grouping job (worker start).
pub async fn owners_waiting(conn: &mut PgConnection) -> Result<Vec<Uuid>, DbError> {
    let sql = format!("SELECT DISTINCT a.owner_id {GROUPABLE} WHERE NOT a.duplicates_checked");
    Ok(sqlx::query_scalar(&sql).fetch_all(conn).await?)
}

/// Serialises grouping runs of one owner for the transaction.
pub async fn lock_owner(conn: &mut PgConnection, owner: Uuid) -> Result<(), DbError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('duplicates:' || $1::text, 0))")
        .bind(owner)
        .execute(conn)
        .await?;
    Ok(())
}

/// A photo the grouping job considers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct Candidate {
    pub node_id: Uuid,
    pub taken_at: PrimitiveDateTime,
    phash: i64,
    /// Already in a group.
    pub grouped: bool,
}

impl Candidate {
    pub fn new(node_id: Uuid, taken_at: PrimitiveDateTime, phash: u64) -> Self {
        Self {
            node_id,
            taken_at,
            phash: to_db(phash),
            grouped: false,
        }
    }

    pub fn phash(&self) -> u64 {
        from_db(self.phash)
    }
}

const CANDIDATE_COLUMNS: &str = "a.node_id, a.taken_at, m.phash, \
     EXISTS (SELECT 1 FROM duplicate_members d WHERE d.node_id = a.node_id) AS grouped";

/// `owner`'s photos not looked at yet, oldest capture first.
pub async fn unchecked_page(
    conn: &mut PgConnection,
    owner: Uuid,
    limit: i64,
) -> Result<Vec<Candidate>, DbError> {
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} {GROUPABLE} \
          WHERE a.owner_id = $1 AND NOT a.duplicates_checked \
          ORDER BY a.taken_at, a.node_id LIMIT $2"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(limit)
        .fetch_all(conn)
        .await?)
}

/// `owner`'s group whose anchor is nearest to `photo` within the limits.
pub async fn nearest_group(
    conn: &mut PgConnection,
    owner: Uuid,
    photo: &Candidate,
) -> Result<Option<(Uuid, i16)>, DbError> {
    let distance = distance_sql("g.anchor_phash", "$2");
    let sql = format!(
        "SELECT g.id, {distance}::int2 AS distance FROM duplicate_groups g \
          WHERE g.owner_id = $1 \
            AND g.anchor_taken_at BETWEEN $3 - make_interval(hours => $4) \
                                      AND $3 + make_interval(hours => $4) \
            AND {distance} <= $5 \
          ORDER BY distance, abs(extract(epoch FROM g.anchor_taken_at - $3)), g.id LIMIT 1"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(photo.phash)
        .bind(photo.taken_at)
        .bind(WINDOW_HOURS)
        .bind(MAX_DISTANCE)
        .fetch_optional(conn)
        .await?)
}

/// `owner`'s other photos, in no group, within the limits of `photo`.
pub async fn ungrouped_matches(
    conn: &mut PgConnection,
    owner: Uuid,
    photo: &Candidate,
) -> Result<Vec<Candidate>, DbError> {
    let distance = distance_sql("m.phash", "$3");
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} {GROUPABLE} \
          WHERE a.owner_id = $1 AND a.node_id <> $2 \
            AND a.taken_at BETWEEN $4 - make_interval(hours => $5) \
                               AND $4 + make_interval(hours => $5) \
            AND {distance} <= $6 \
            AND NOT EXISTS (SELECT 1 FROM duplicate_members d WHERE d.node_id = a.node_id) \
          ORDER BY a.taken_at, a.node_id"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(photo.node_id)
        .bind(photo.phash)
        .bind(photo.taken_at)
        .bind(WINDOW_HOURS)
        .bind(MAX_DISTANCE)
        .fetch_all(conn)
        .await?)
}

/// Adds `node` to `group`. A resolved group is reopened: the new photo stays
/// visible until the user decides again (photos hidden before stay hidden).
pub async fn join_group(
    conn: &mut PgConnection,
    owner: Uuid,
    group: Uuid,
    node: Uuid,
    distance: i16,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO duplicate_members (node_id, owner_id, group_id, distance) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (node_id) DO NOTHING",
    )
    .bind(node)
    .bind(owner)
    .bind(group)
    .bind(distance)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE duplicate_groups SET resolved_at = NULL, updated_at = now() \
          WHERE id = $1 AND owner_id = $2",
    )
    .bind(group)
    .bind(owner)
    .execute(conn)
    .await?;
    Ok(())
}

/// Creates a group anchored at `anchor` holding `members` (with their
/// distance to the anchor).
pub async fn create_group(
    conn: &mut PgConnection,
    owner: Uuid,
    anchor: &Candidate,
    members: &[(Uuid, i16)],
) -> Result<Uuid, DbError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO duplicate_groups (id, owner_id, anchor_phash, anchor_taken_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(owner)
    .bind(anchor.phash)
    .bind(anchor.taken_at)
    .execute(&mut *conn)
    .await?;
    let (nodes, distances): (Vec<Uuid>, Vec<i16>) = members.iter().copied().unzip();
    sqlx::query(
        "INSERT INTO duplicate_members (node_id, owner_id, group_id, distance) \
         SELECT node, $2, $3, distance FROM unnest($1::uuid[], $4::int2[]) AS x (node, distance) \
         ON CONFLICT (node_id) DO NOTHING",
    )
    .bind(&nodes)
    .bind(owner)
    .bind(id)
    .bind(&distances)
    .execute(conn)
    .await?;
    Ok(id)
}

/// Marks `nodes` as looked at.
pub async fn mark_checked(
    conn: &mut PgConnection,
    owner: Uuid,
    nodes: &[Uuid],
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE assets SET duplicates_checked = TRUE WHERE owner_id = $1 AND node_id = ANY($2)",
    )
    .bind(owner)
    .bind(nodes)
    .execute(conn)
    .await?;
    Ok(())
}
