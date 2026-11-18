//! What a user's photos hold, grouped for Explore: tags, places, cameras.
//! Every count is over the caller's own live assets (one per node: two
//! copies of a photo count twice, as in the timeline); covers are the newest
//! asset of each group.

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::documents::timed_out_is_busy;
use crate::timeline::FROM;
use crate::{DbError, bounded};

/// Groups listed at most.
pub const MAX_GROUPS: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TagGroup {
    pub tag: String,
    pub count: i64,
    /// The node id of the newest photo with this tag.
    pub cover: Uuid,
}

/// The caller's tags, most used first.
pub async fn tags(pool: &PgPool, owner_id: Uuid) -> Result<Vec<TagGroup>, DbError> {
    let sql = format!(
        "SELECT t.tag, count(DISTINCT a.node_id) AS count, \
                (array_agg(a.node_id ORDER BY a.taken_at DESC, a.node_id DESC))[1] AS cover \
         {FROM} JOIN asset_tags t ON t.sha256 = a.sha256 \
          WHERE a.owner_id = $1 GROUP BY t.tag ORDER BY count DESC, t.tag LIMIT $2"
    );
    let mut tx = bounded::begin(pool).await?;
    let rows = sqlx::query_as(&sql)
        .bind(owner_id)
        .bind(MAX_GROUPS)
        .fetch_all(&mut *tx)
        .await
        .map_err(timed_out_is_busy)?;
    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceLevel {
    Country,
    City,
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PlaceGroup {
    pub country_code: String,
    pub country: Option<String>,
    /// `None` at country level, or for places without a region.
    pub region: Option<String>,
    /// `None` at country level.
    pub city: Option<String>,
    pub count: i64,
    pub cover: Uuid,
    /// Where the cover photo was taken.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// The caller's geocoded places, most photographed first: one row per
/// `(country, region, city)`, or per country.
pub async fn places(
    pool: &PgPool,
    owner_id: Uuid,
    level: PlaceLevel,
) -> Result<Vec<PlaceGroup>, DbError> {
    let (columns, group) = match level {
        PlaceLevel::City => (
            "m.place_country_code AS country_code, m.place_country AS country, \
             m.place_admin1 AS region, m.place_name AS city",
            "m.place_country_code, m.place_country, m.place_admin1, m.place_name",
        ),
        PlaceLevel::Country => (
            "m.place_country_code AS country_code, min(m.place_country) AS country, \
             NULL::text AS region, NULL::text AS city",
            "m.place_country_code",
        ),
    };
    let sql = format!(
        "SELECT {columns}, count(*) AS count, \
                (array_agg(a.node_id ORDER BY a.taken_at DESC, a.node_id DESC))[1] AS cover, \
                (array_agg(m.latitude ORDER BY a.taken_at DESC, a.node_id DESC))[1] AS latitude, \
                (array_agg(m.longitude ORDER BY a.taken_at DESC, a.node_id DESC))[1] AS longitude \
         {FROM} WHERE a.owner_id = $1 AND m.place_country_code IS NOT NULL \
            AND ($3 OR m.place_name IS NOT NULL) \
          GROUP BY {group} ORDER BY count DESC, 1, 3, 4 LIMIT $2"
    );
    let mut tx = bounded::begin(pool).await?;
    let rows = sqlx::query_as(&sql)
        .bind(owner_id)
        .bind(MAX_GROUPS)
        .bind(level == PlaceLevel::Country)
        .fetch_all(&mut *tx)
        .await
        .map_err(timed_out_is_busy)?;
    tx.commit().await?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CameraGroup {
    pub make: String,
    pub model: Option<String>,
    pub count: i64,
}

/// The caller's cameras (make and model as recorded), most used first.
pub async fn cameras(pool: &PgPool, owner_id: Uuid) -> Result<Vec<CameraGroup>, DbError> {
    let sql = format!(
        "SELECT m.camera_make AS make, m.camera_model AS model, count(*) AS count \
         {FROM} WHERE a.owner_id = $1 AND m.camera_make IS NOT NULL \
          GROUP BY 1, 2 ORDER BY count DESC, 1, 2 LIMIT $2"
    );
    let mut tx = bounded::begin(pool).await?;
    let rows = sqlx::query_as(&sql)
        .bind(owner_id)
        .bind(MAX_GROUPS)
        .fetch_all(&mut *tx)
        .await
        .map_err(timed_out_is_busy)?;
    tx.commit().await?;
    Ok(rows)
}

/// The tags of one of the caller's live photos, best first (empty for
/// anything else).
pub async fn tags_of_photo(
    pool: &PgPool,
    owner_id: Uuid,
    node_id: Uuid,
) -> Result<Vec<String>, DbError> {
    let sql = format!(
        "SELECT t.tag {FROM} JOIN asset_tags t ON t.sha256 = a.sha256 \
          WHERE a.owner_id = $1 AND a.node_id = $2 ORDER BY t.score DESC, t.tag"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(owner_id)
        .bind(node_id)
        .fetch_all(pool)
        .await?)
}

/// The caller's live images whose CLIP step is pending or failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, FromRow)]
pub struct MlCounts {
    pub queued: i64,
    pub failed: i64,
}

pub async fn ml_counts(pool: &PgPool, owner_id: Uuid) -> Result<MlCounts, DbError> {
    let sql = format!(
        "SELECT count(*) FILTER (WHERE m.clip_state = 'pending') AS queued, \
                count(*) FILTER (WHERE m.clip_state = 'failed') AS failed \
         {FROM} WHERE a.owner_id = $1"
    );
    Ok(sqlx::query_as(&sql).bind(owner_id).fetch_one(pool).await?)
}
