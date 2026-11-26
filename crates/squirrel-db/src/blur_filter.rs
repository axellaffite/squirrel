//! A user's blur filter: a rule, as a dynamic album's ([`crate::album_rules`]),
//! whose matching photos (the user's own) clients show blurred until the user
//! reveals them. One per user; it can be turned off without being forgotten.

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::DbError;
use crate::album_rules::{AlbumRules, RULE_FIELDS, RULE_MATCHES, RuleRow, SearchMatch, dedup};
use crate::assets::AssetKind;
use crate::timeline::FROM;

/// Photos listed as blurred, at most: more than any home library holds.
pub const MAX_BLURRED: i64 = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub struct BlurFilter {
    /// Whether clients blur the matching photos.
    pub enabled: bool,
    pub rules: AlbumRules,
}

#[derive(FromRow)]
struct FilterRow {
    enabled: bool,
    #[sqlx(flatten)]
    rule: RuleRow,
}

/// `owner_id`'s blur filter, if they have one (its search match is not read back).
pub async fn find(pool: &PgPool, owner_id: Uuid) -> Result<Option<BlurFilter>, DbError> {
    let sql = format!("SELECT enabled, {RULE_FIELDS} FROM blur_filters WHERE owner_id = $1");
    let row = sqlx::query_as::<_, FilterRow>(&sql)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|row| BlurFilter {
        enabled: row.enabled,
        rules: AlbumRules::from(row.rule),
    }))
}

/// Inserts or replaces `owner_id`'s blur filter. A search text needs its
/// [`SearchMatch`] (a check of the table), as for album rules.
pub async fn save(pool: &PgPool, owner_id: Uuid, filter: &BlurFilter) -> Result<(), DbError> {
    let rules = &filter.rules;
    let search = rules.search_match.as_ref();
    sqlx::query(
        "INSERT INTO blur_filters (owner_id, enabled, people, people_match, tags, tags_match, \
                                   country, region, city, camera_make, camera_model, kind, \
                                   taken_from, taken_to, search, search_model, search_embedding, \
                                   search_rivals, search_max_distance, search_rival_margin) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                 $15, $16, $17, COALESCE($18, '{}'), $19, $20) \
         ON CONFLICT (owner_id) DO UPDATE SET \
             enabled = EXCLUDED.enabled, \
             people = EXCLUDED.people, people_match = EXCLUDED.people_match, \
             tags = EXCLUDED.tags, tags_match = EXCLUDED.tags_match, \
             country = EXCLUDED.country, region = EXCLUDED.region, city = EXCLUDED.city, \
             camera_make = EXCLUDED.camera_make, camera_model = EXCLUDED.camera_model, \
             kind = EXCLUDED.kind, taken_from = EXCLUDED.taken_from, \
             taken_to = EXCLUDED.taken_to, search = EXCLUDED.search, \
             search_model = EXCLUDED.search_model, search_embedding = EXCLUDED.search_embedding, \
             search_rivals = EXCLUDED.search_rivals, \
             search_max_distance = EXCLUDED.search_max_distance, \
             search_rival_margin = EXCLUDED.search_rival_margin, \
             updated_at = now()",
    )
    .bind(owner_id)
    .bind(filter.enabled)
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
    .bind(&rules.search)
    .bind(search.map(|s| s.model.clone()))
    .bind(search.map(SearchMatch::embedding))
    .bind(search.map(SearchMatch::rivals))
    .bind(search.map(|s| s.max_distance))
    .bind(search.map(|s| s.rival_margin))
    .execute(pool)
    .await?;
    Ok(())
}

/// Forgets `owner_id`'s blur filter (nothing is blurred any more).
pub async fn delete(pool: &PgPool, owner_id: Uuid) -> Result<(), DbError> {
    sqlx::query("DELETE FROM blur_filters WHERE owner_id = $1")
        .bind(owner_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The node ids of `owner_id`'s live, visible photos their enabled blur filter
/// matches, newest first; none without one. Under the search timeout
/// ([`DbError::Busy`] past it).
pub async fn blurred_ids(pool: &PgPool, owner_id: Uuid) -> Result<Vec<Uuid>, DbError> {
    let mut tx = crate::bounded::begin(pool).await?;
    let ids = sqlx::query_scalar(&format!(
        "SELECT a.node_id {FROM} \
           JOIN blur_filters r ON r.owner_id = a.owner_id AND r.enabled \
          WHERE a.owner_id = $1 AND {RULE_MATCHES} \
          ORDER BY a.taken_at DESC, a.node_id DESC LIMIT $2"
    ))
    .bind(owner_id)
    .bind(MAX_BLURRED)
    .fetch_all(&mut *tx)
    .await
    .map_err(crate::documents::timed_out_is_busy)?;
    tx.commit().await?;
    Ok(ids)
}
