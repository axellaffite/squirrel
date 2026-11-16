//! A user's photo timeline: their live image files, newest capture first.
//!
//! Every query starts from `assets` filtered on the caller's `owner_id` and
//! joins the node (live only) before the shared, content-derived `media` row,
//! so another user's photos, shared dedup blobs and trashed files never leak.

use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use time::{Date, Month, PrimitiveDateTime, Time};
use uuid::Uuid;

use crate::media::MediaState;
use crate::{DbError, Page};

/// Columns of [`AssetSummaryRecord`], shared by the timeline, albums and detail.
pub(crate) const SUMMARY_COLUMNS: &str = "a.node_id, n.name, n.mime, n.size, a.kind, a.taken_at, \
     a.taken_offset_min, m.width, m.height, m.blurhash, m.thumbs_state, m.thumbs_version";
/// Live nodes only; `n.owner_id = a.owner_id` is also guaranteed by the foreign key.
pub(crate) const FROM: &str = "FROM assets a \
     JOIN nodes n ON n.id = a.node_id AND n.owner_id = a.owner_id AND n.deleted_at IS NULL \
     JOIN media m ON m.sha256 = a.sha256";

/// One photo of the timeline.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct AssetSummaryRecord {
    pub node_id: Uuid,
    pub name: String,
    pub mime: Option<String>,
    pub size: i64,
    /// `photo` or `screenshot`.
    pub kind: String,
    /// Wall-clock capture time.
    pub taken_at: PrimitiveDateTime,
    pub taken_offset_min: Option<i16>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub blurhash: Option<String>,
    thumbs_state: String,
    pub thumbs_version: i16,
}

impl AssetSummaryRecord {
    pub fn thumbs_state(&self) -> MediaState {
        MediaState::parse(&self.thumbs_state)
    }

    /// Where a listing after this item resumes.
    pub fn cursor(&self) -> TimelineCursor {
        TimelineCursor {
            taken_at: self.taken_at,
            node_id: self.node_id,
        }
    }
}

/// Where a timeline listing resumes (`taken_at DESC, node_id DESC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineCursor {
    pub taken_at: PrimitiveDateTime,
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    /// A calendar month of wall-clock capture time: `(year, 1..=12)`.
    Month(i32, u8),
}

/// `[first instant of the month, first instant of the next)`.
pub fn month_bounds(year: i32, month: u8) -> Option<(PrimitiveDateTime, PrimitiveDateTime)> {
    let month = Month::try_from(month).ok()?;
    let start = Date::from_calendar_date(year, month, 1).ok()?;
    let (next_year, next_month) = if month == Month::December {
        (year.checked_add(1)?, Month::January)
    } else {
        (year, month.next())
    };
    let end = Date::from_calendar_date(next_year, next_month, 1).ok()?;
    Some((
        PrimitiveDateTime::new(start, Time::MIDNIGHT),
        PrimitiveDateTime::new(end, Time::MIDNIGHT),
    ))
}

/// The query of [`page`], after `prefix` (`""`, or `EXPLAIN ` in tests).
/// Built from fixed fragments and binds only, so the keyset row comparison
/// stays an Index Cond on `assets_timeline_idx`.
fn page_query<'a>(
    prefix: &str,
    owner_id: Uuid,
    filter: Filter,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<QueryBuilder<'a, Postgres>, DbError> {
    let mut query = QueryBuilder::<Postgres>::new(format!(
        "{prefix}SELECT {SUMMARY_COLUMNS} {FROM} WHERE a.owner_id = "
    ));
    query.push_bind(owner_id);
    if let Some(cursor) = after {
        query
            .push(" AND (a.taken_at, a.node_id) < (")
            .push_bind(cursor.taken_at)
            .push(", ")
            .push_bind(cursor.node_id)
            .push(")");
    }
    if let Filter::Month(year, month) = filter {
        let (from, to) =
            month_bounds(year, month).ok_or(DbError::InvalidInput("month out of range"))?;
        query
            .push(" AND a.taken_at >= ")
            .push_bind(from)
            .push(" AND a.taken_at < ")
            .push_bind(to);
    }
    query
        .push(" ORDER BY a.taken_at DESC, a.node_id DESC LIMIT ")
        .push_bind(i64::from(limit) + 1);
    Ok(query)
}

/// One page of `owner_id`'s live photos, newest capture first.
pub async fn page(
    pool: &PgPool,
    owner_id: Uuid,
    filter: Filter,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<AssetSummaryRecord>, DbError> {
    let mut query = page_query("", owner_id, filter, after, limit)?;
    let rows = query
        .build_query_as::<AssetSummaryRecord>()
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// The plan of [`page`]'s query (tests check it uses the index).
#[doc(hidden)]
pub async fn explain_page(
    conn: &mut sqlx::PgConnection,
    owner_id: Uuid,
    filter: Filter,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<String, DbError> {
    let mut query = page_query("EXPLAIN ", owner_id, filter, after, limit)?;
    let lines: Vec<(String,)> = query.build_query_as().fetch_all(conn).await?;
    Ok(lines
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n"))
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct MonthCount {
    /// `YYYY-MM`.
    pub month: String,
    pub count: i64,
}

/// How many live photos `owner_id` has per month, newest month first.
pub async fn months(pool: &PgPool, owner_id: Uuid) -> Result<Vec<MonthCount>, DbError> {
    let sql = format!(
        "SELECT to_char(a.taken_at, 'YYYY-MM') AS month, count(*) AS count {FROM} \
          WHERE a.owner_id = $1 GROUP BY 1 ORDER BY 1 DESC"
    );
    Ok(sqlx::query_as::<_, MonthCount>(&sql)
        .bind(owner_id)
        .fetch_all(pool)
        .await?)
}

/// Everything the photo viewer shows about one photo.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct AssetDetailRecord {
    #[sqlx(flatten)]
    pub summary: AssetSummaryRecord,
    pub parent_id: Option<Uuid>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f32>,
    pub iso: Option<i32>,
    pub focal_length_mm: Option<f32>,
    pub place_name: Option<String>,
    pub place_admin1: Option<String>,
    pub place_country: Option<String>,
    pub place_country_code: Option<String>,
}

/// One of `owner_id`'s live photos; `None` for anything else (another
/// user's node, a trashed file, a file that is not a photo).
pub async fn detail(
    pool: &PgPool,
    owner_id: Uuid,
    node_id: Uuid,
) -> Result<Option<AssetDetailRecord>, DbError> {
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS}, n.parent_id, m.latitude, m.longitude, m.altitude, \
                m.camera_make, m.camera_model, m.lens_model, m.exposure_time, m.f_number, \
                m.iso, m.focal_length_mm, m.place_name, m.place_admin1, m.place_country, \
                m.place_country_code \
         {FROM} WHERE a.owner_id = $1 AND a.node_id = $2"
    );
    Ok(sqlx::query_as::<_, AssetDetailRecord>(&sql)
        .bind(owner_id)
        .bind(node_id)
        .fetch_optional(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn months_are_half_open_and_december_rolls_over() {
        assert_eq!(
            month_bounds(2024, 7),
            Some((datetime!(2024-07-01 00:00), datetime!(2024-08-01 00:00)))
        );
        assert_eq!(
            month_bounds(2024, 12),
            Some((datetime!(2024-12-01 00:00), datetime!(2025-01-01 00:00)))
        );
        assert_eq!(month_bounds(2024, 0), None);
        assert_eq!(month_bounds(2024, 13), None);
        assert_eq!(month_bounds(9999, 12), None);
    }
}
