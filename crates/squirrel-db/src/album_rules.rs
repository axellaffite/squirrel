//! The rules of dynamic albums: groups of conditions combined with AND, each
//! group of people or tags matching any or all of its values. A dynamic
//! album's photos are its owner's live, visible photos matching its rule,
//! computed on read.
//!
//! [`RULE_MATCHES`] is the single SQL definition of "matches": it reads the
//! rule from a row aliased `r` (an `album_rules` row, or the bound values of a
//! preview) and the photo from `a` and `m` ([`crate::timeline::FROM`]).

use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use time::Date;
use uuid::Uuid;

use crate::DbError;
use crate::assets::AssetKind;
use crate::timeline::{AssetSummaryRecord, FROM, SUMMARY_COLUMNS};

/// People or tags per rule, at most (also a check of the table).
pub const MAX_VALUES: usize = 20;

/// `a` (an asset of `r.owner_id`) matches rule `r`.
pub(crate) const RULE_MATCHES: &str = "a.owner_id = r.owner_id \
     AND (cardinality(r.people) = 0 \
          OR (SELECT count(DISTINCT f.person_id) FROM faces f \
               WHERE f.owner_id = a.owner_id AND f.sha256 = a.sha256 \
                 AND f.person_id = ANY (r.people)) \
             >= CASE r.people_match WHEN 'all' THEN cardinality(r.people) ELSE 1 END) \
     AND (cardinality(r.tags) = 0 \
          OR (SELECT count(DISTINCT t.tag) FROM asset_tags t \
               WHERE t.sha256 = a.sha256 AND t.tag = ANY (r.tags)) \
             >= CASE r.tags_match WHEN 'all' THEN cardinality(r.tags) ELSE 1 END) \
     AND (r.country IS NULL OR m.place_country_code = r.country) \
     AND (r.region IS NULL OR (r.region = '' AND m.place_admin1 IS NULL) \
          OR m.place_admin1 = r.region) \
     AND (r.city IS NULL OR m.place_name = r.city) \
     AND (r.camera_make IS NULL OR m.camera_make = r.camera_make) \
     AND (r.camera_model IS NULL OR m.camera_model = r.camera_model) \
     AND (r.kind IS NULL OR a.kind = r.kind) \
     AND (r.taken_from IS NULL OR a.taken_at >= r.taken_from) \
     AND (r.taken_to IS NULL OR a.taken_at < r.taken_to + 1)";

/// How a group of people or tags matches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Match {
    /// At least one of them is on the photo.
    #[default]
    Any,
    /// Every one of them is on the photo.
    All,
}

impl Match {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
        }
    }

    fn parse(raw: &str) -> Self {
        if raw == "all" { Self::All } else { Self::Any }
    }
}

/// A dynamic album's rule. Empty groups and `None` fields do not constrain.
/// Checked by the API: at least one condition, place parts need their
/// parents, a camera model its make, `taken_from <= taken_to`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlbumRules {
    /// The owner's people, without duplicates.
    pub people: Vec<Uuid>,
    pub people_match: Match,
    /// Tag slugs, without duplicates.
    pub tags: Vec<String>,
    pub tags_match: Match,
    /// ISO 3166 country code, as geocoded.
    pub country: Option<String>,
    /// Within `country`; empty: places without a region.
    pub region: Option<String>,
    pub city: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub kind: Option<AssetKind>,
    /// Capture dates, both inclusive.
    pub taken_from: Option<Date>,
    pub taken_to: Option<Date>,
}

impl AlbumRules {
    /// Whether no condition is set (it would match the whole timeline).
    pub fn is_empty(&self) -> bool {
        Self {
            people_match: Match::Any,
            tags_match: Match::Any,
            ..self.clone()
        } == Self::default()
    }
}

#[derive(FromRow)]
struct RulesRow {
    album_id: Uuid,
    people: Vec<Uuid>,
    people_match: String,
    tags: Vec<String>,
    tags_match: String,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    camera_make: Option<String>,
    camera_model: Option<String>,
    kind: Option<String>,
    taken_from: Option<Date>,
    taken_to: Option<Date>,
}

impl From<RulesRow> for AlbumRules {
    fn from(row: RulesRow) -> Self {
        Self {
            people: row.people,
            people_match: Match::parse(&row.people_match),
            tags: row.tags,
            tags_match: Match::parse(&row.tags_match),
            country: row.country,
            region: row.region,
            city: row.city,
            camera_make: row.camera_make,
            camera_model: row.camera_model,
            kind: row.kind.as_deref().map(|kind| match kind {
                "screenshot" => AssetKind::Screenshot,
                _ => AssetKind::Photo,
            }),
            taken_from: row.taken_from,
            taken_to: row.taken_to,
        }
    }
}

const RULE_COLUMNS: &str = "album_id, people, people_match, tags, tags_match, country, region, \
     city, camera_make, camera_model, kind, taken_from, taken_to";

/// The rules of those of `album_ids` that are dynamic, by album.
pub async fn find_many(
    pool: &PgPool,
    album_ids: &[Uuid],
) -> Result<Vec<(Uuid, AlbumRules)>, DbError> {
    if album_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!("SELECT {RULE_COLUMNS} FROM album_rules WHERE album_id = ANY($1)");
    let rows = sqlx::query_as::<_, RulesRow>(&sql)
        .bind(album_ids)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.album_id, AlbumRules::from(row)))
        .collect())
}

/// The owner of dynamic album `album_id`; `None` for a manual album (or none).
pub(crate) async fn owner_of(pool: &PgPool, album_id: Uuid) -> Result<Option<Uuid>, DbError> {
    Ok(
        sqlx::query_scalar("SELECT owner_id FROM album_rules WHERE album_id = $1")
            .bind(album_id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Whether `album_id` is a dynamic album.
pub(crate) async fn is_dynamic(conn: &mut PgConnection, album_id: Uuid) -> Result<bool, DbError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM album_rules WHERE album_id = $1)")
            .bind(album_id)
            .fetch_one(conn)
            .await?,
    )
}

fn dedup<T: PartialEq + Clone>(values: &[T]) -> Vec<T> {
    values.iter().fold(Vec::new(), |mut kept, value| {
        if !kept.contains(value) {
            kept.push(value.clone());
        }
        kept
    })
}

/// Inserts or replaces the rule of `album_id` (an album of `owner_id`).
pub(crate) async fn save(
    conn: &mut PgConnection,
    album_id: Uuid,
    owner_id: Uuid,
    rules: &AlbumRules,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO album_rules (album_id, owner_id, people, people_match, tags, tags_match, \
                                  country, region, city, camera_make, camera_model, kind, \
                                  taken_from, taken_to) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         ON CONFLICT (album_id) DO UPDATE SET \
             people = EXCLUDED.people, people_match = EXCLUDED.people_match, \
             tags = EXCLUDED.tags, tags_match = EXCLUDED.tags_match, \
             country = EXCLUDED.country, region = EXCLUDED.region, city = EXCLUDED.city, \
             camera_make = EXCLUDED.camera_make, camera_model = EXCLUDED.camera_model, \
             kind = EXCLUDED.kind, taken_from = EXCLUDED.taken_from, \
             taken_to = EXCLUDED.taken_to",
    )
    .bind(album_id)
    .bind(owner_id)
    .bind(dedup(&rules.people))
    .bind(rules.people_match.as_str())
    .bind(dedup(&rules.tags))
    .bind(rules.tags_match.as_str())
    .bind(&rules.country)
    .bind(&rules.region)
    .bind(&rules.city)
    .bind(&rules.camera_make)
    .bind(&rules.camera_model)
    .bind(rules.kind.map(AssetKind::as_str))
    .bind(rules.taken_from)
    .bind(rules.taken_to)
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether every one of `people` is one of `owner_id`'s people.
pub async fn are_people_of(
    pool: &PgPool,
    owner_id: Uuid,
    people: &[Uuid],
) -> Result<bool, DbError> {
    if people.is_empty() {
        return Ok(true);
    }
    let found: i64 =
        sqlx::query_scalar("SELECT count(*) FROM people WHERE owner_id = $1 AND id = ANY($2)")
            .bind(owner_id)
            .bind(dedup(people))
            .fetch_one(pool)
            .await?;
    Ok(usize::try_from(found).ok() == Some(dedup(people).len()))
}

/// What a rule would hold right now: how many photos, and the newest ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub count: i64,
    pub newest: Vec<AssetSummaryRecord>,
}

/// `WITH r AS (…)`: `rules` as the row `r` of [`RULE_MATCHES`].
fn push_rule_row(query: &mut QueryBuilder<'_, Postgres>, owner_id: Uuid, rules: &AlbumRules) {
    query
        .push("WITH r AS (SELECT ")
        .push_bind(owner_id)
        .push("::uuid AS owner_id, ")
        .push_bind(dedup(&rules.people))
        .push("::uuid[] AS people, ")
        .push_bind(rules.people_match.as_str())
        .push("::text AS people_match, ")
        .push_bind(dedup(&rules.tags))
        .push("::text[] AS tags, ")
        .push_bind(rules.tags_match.as_str())
        .push("::text AS tags_match, ")
        .push_bind(rules.country.clone())
        .push("::text AS country, ")
        .push_bind(rules.region.clone())
        .push("::text AS region, ")
        .push_bind(rules.city.clone())
        .push("::text AS city, ")
        .push_bind(rules.camera_make.clone())
        .push("::text AS camera_make, ")
        .push_bind(rules.camera_model.clone())
        .push("::text AS camera_model, ")
        .push_bind(rules.kind.map(AssetKind::as_str))
        .push("::text AS kind, ")
        .push_bind(rules.taken_from)
        .push("::date AS taken_from, ")
        .push_bind(rules.taken_to)
        .push("::date AS taken_to) ");
}

/// What `rules` would match among `owner_id`'s photos (before saving them).
pub async fn preview(
    pool: &PgPool,
    owner_id: Uuid,
    rules: &AlbumRules,
    newest: u32,
) -> Result<Preview, DbError> {
    let mut tx = crate::bounded::begin(pool).await?;
    let mut count_query = QueryBuilder::<Postgres>::new("");
    push_rule_row(&mut count_query, owner_id, rules);
    count_query.push(format!(
        "SELECT count(*) {FROM} CROSS JOIN r WHERE a.owner_id = r.owner_id AND {RULE_MATCHES}"
    ));
    let count: i64 = count_query
        .build_query_scalar()
        .fetch_one(&mut *tx)
        .await
        .map_err(crate::documents::timed_out_is_busy)?;
    let mut items_query = QueryBuilder::<Postgres>::new("");
    push_rule_row(&mut items_query, owner_id, rules);
    items_query
        .push(format!(
            "SELECT {SUMMARY_COLUMNS} {FROM} CROSS JOIN r \
              WHERE a.owner_id = r.owner_id AND {RULE_MATCHES} \
              ORDER BY a.taken_at DESC, a.node_id DESC LIMIT "
        ))
        .push_bind(i64::from(newest));
    let newest = items_query
        .build_query_as::<AssetSummaryRecord>()
        .fetch_all(&mut *tx)
        .await
        .map_err(crate::documents::timed_out_is_busy)?;
    tx.commit().await?;
    Ok(Preview { count, newest })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_modes_alone_do_not_make_a_rule() {
        let modes_only = AlbumRules {
            people_match: Match::All,
            tags_match: Match::All,
            ..AlbumRules::default()
        };
        assert!(modes_only.is_empty());
        let tagged = AlbumRules {
            tags: vec!["beach".into()],
            ..AlbumRules::default()
        };
        assert!(!tagged.is_empty());
    }

    #[test]
    fn dedup_keeps_first_occurrences_in_order() {
        assert_eq!(dedup(&["b", "a", "b", "c", "a"]), ["b", "a", "c"]);
    }

    #[test]
    fn unknown_match_modes_read_as_any() {
        assert_eq!(Match::parse("all"), Match::All);
        assert_eq!(Match::parse("any"), Match::Any);
        assert_eq!(Match::parse("?"), Match::Any);
    }
}
