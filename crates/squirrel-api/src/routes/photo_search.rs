//! Photo search by text (SigLIP2, any language): the query is encoded by the
//! server's text child, then the caller's own photos are ranked by an exact
//! cosine scan. A query close to a grouped label of the bank ("dog") must
//! also beat that label's siblings ("cat") on each photo.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use squirrel_db::album_rules::SearchMatch;
use squirrel_db::photo_search::{self, PhotoQuery};
use squirrel_db::timeline::TimelineCursor;
use squirrel_ml::clip::{self, labels};
use utoipa::{IntoParams, ToSchema};

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::filter_dto::{PhotoFilterQuery, with_filters};
use crate::input::{ApiQuery, known_params};
use crate::ml::EncodeError;
use crate::photos_dto::AssetSummary;
use crate::state::AppState;

/// Results reachable through `offset` and `limit`, at most.
pub const MAX_RESULTS: u32 = 200;
pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;

/// How results are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchOrder {
    /// Best match first, paged by `offset`.
    #[default]
    Relevance,
    /// Newest capture first (`taken_at`, as the timeline), paged by `cursor`.
    Date,
}

impl SearchOrder {
    /// An unknown value is a validation error (422), not a malformed query.
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw {
            None | Some("relevance") => Ok(Self::Relevance),
            Some("date") => Ok(Self::Date),
            Some(_) => Err(ApiError::Validation(
                "order must be relevance or date".into(),
            )),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PhotoSearchQuery {
    /// What to look for, in any language (1–200 characters), e.g. `dog on the beach`
    /// or `chien sur la plage`.
    pub q: Option<String>,
    /// `relevance` (default): best match first. `date`: the same results,
    /// newest capture first.
    #[param(value_type = Option<SearchOrder>)]
    pub order: Option<String>,
    /// Results to skip; `offset + limit` is at most 200. `order=relevance` only.
    #[param(minimum = 0, maximum = 199)]
    pub offset: Option<u32>,
    /// `next_cursor` of the previous page. `order=date` only.
    pub cursor: Option<String>,
    /// Page size, 1–100, default 50.
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
}

/// The caller's photos that look like the query, in the requested order, at
/// most 200 of them.
#[derive(Debug, Serialize, ToSchema)]
pub struct PhotoSearchResults {
    pub items: Vec<AssetSummary>,
    /// More results within the first 200: ask again with `offset + limit`
    /// (`order=relevance`) or `cursor` (`order=date`).
    pub has_more: bool,
    /// `order=date`: pass back as `cursor` for the next page; `null` on the
    /// last page and for `order=relevance`.
    pub next_cursor: Option<String>,
}

/// Where a checked page starts.
#[derive(Debug, PartialEq)]
enum Start {
    Offset(u32),
    After(Option<TimelineCursor>),
}

/// `(start, limit)` once checked: `offset` belongs to relevance, `cursor` to date.
fn page_bounds(query: &PhotoSearchQuery, order: SearchOrder) -> Result<(Start, u32), ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::Validation(format!("limit must be 1–{MAX_LIMIT}")));
    }
    if order == SearchOrder::Date {
        if query.offset.is_some() {
            return Err(ApiError::Validation(
                "order=date pages with cursor, not offset".into(),
            ));
        }
        let after = query
            .cursor
            .as_deref()
            .map(cursor::decode_timeline)
            .transpose()?;
        return Ok((Start::After(after), limit));
    }
    if query.cursor.is_some() {
        return Err(ApiError::Validation(
            "cursor needs order=date (relevance pages with offset)".into(),
        ));
    }
    let offset = query.offset.unwrap_or(0);
    if offset.saturating_add(limit) > MAX_RESULTS {
        return Err(ApiError::Validation(format!(
            "offset + limit must be at most {MAX_RESULTS}"
        )));
    }
    Ok((Start::Offset(offset), limit))
}

/// Search the caller's photos by text (any language).
///
/// A photo is a result when the model's match probability with the query is
/// high enough and, for a query naming a label of the tag bank (`dog`,
/// `chien`), when it beats that label's easily confused siblings (`cat`) on
/// that photo. At most the 200 best results can be reached. With
/// `order=relevance` (the default) they come best first, paged by offset
/// (`offset + limit ≤ 200`). With `order=date` the same results come newest
/// capture first (`taken_at DESC`, then node id, as the timeline: a photo
/// without a capture date sits at its upload time), paged by `cursor`. Only
/// the caller's own live photos are searched.
#[utoipa::path(get, path = "/api/search/photos", security(("device_bearer" = []), ("session_cookie" = [])), tag = "search", params(PhotoSearchQuery, PhotoFilterQuery),
    responses((status = 200, body = PhotoSearchResults),
              (status = 400, body = ErrorBody, description = "Invalid cursor"),
              (status = 422, body = ErrorBody, description = "Invalid q, order, offset, cursor, limit or filter"),
              (status = 429, body = ErrorBody, description = "Too many searches from this client"),
              (status = 503, body = ErrorBody, description = "`ml_unavailable`: no text model on this server; `busy`: try again")))]
pub async fn search_photos(
    State(state): State<AppState>,
    caller: Caller,
    RawQuery(raw): RawQuery,
    ApiQuery(query): ApiQuery<PhotoSearchQuery>,
    ApiQuery(filters): ApiQuery<PhotoFilterQuery>,
) -> Result<Json<PhotoSearchResults>, ApiError> {
    known_params(
        raw.as_deref(),
        &with_filters(&["q", "order", "offset", "cursor", "limit"]),
    )?;
    let order = SearchOrder::parse(query.order.as_deref())?;
    let (start, limit) = page_bounds(&query, order)?;
    let filter = filters.filter()?;
    let embedding = encode_query(&state, query.q.as_deref().unwrap_or_default()).await?;
    let rivals = rivals(&state.pool, &embedding).await?;
    let photo_query = PhotoQuery {
        model: squirrel_ml::CLIP_MODEL_ID,
        embedding: &embedding,
        max_distance: max_distance(),
        relative_cutoff: f64::from(clip::logits_as_cosine(clip::SEARCH_RELATIVE_CUTOFF)),
        rivals: &rivals,
        rival_margin: rival_margin(),
        filter: &filter,
    };
    let user = caller.user.id;
    let (page, has_more) = match start {
        Start::Offset(offset) => {
            let page = photo_search::search(&state.pool, user, &photo_query, offset, limit).await?;
            let has_more = page.has_more && offset + limit < MAX_RESULTS;
            (page, has_more)
        }
        Start::After(after) => {
            let page = photo_search::search_by_date(
                &state.pool,
                user,
                &photo_query,
                MAX_RESULTS,
                after.as_ref(),
                limit,
            )
            .await?;
            let has_more = page.has_more;
            (page, has_more)
        }
    };
    let next_cursor = match (order, has_more) {
        (SearchOrder::Date, true) => page
            .items
            .last()
            .map(|hit| cursor::encode_timeline(&hit.summary.cursor())),
        _ => None,
    };
    Ok(Json(PhotoSearchResults {
        items: page
            .items
            .iter()
            .map(|hit| AssetSummary::from(&hit.summary))
            .collect(),
        has_more,
        next_cursor,
    }))
}

/// `text`'s unit embedding: 422 for a bad length, 503 without the text model.
async fn encode_query(state: &AppState, text: &str) -> Result<Vec<f32>, ApiError> {
    let encoder = state.text_encoder.as_ref().ok_or(ApiError::MlUnavailable)?;
    encoder.encode(text).await.map_err(|err| match err {
        EncodeError::Length => ApiError::Validation(err.to_string()),
        EncodeError::Unavailable(why) => {
            tracing::warn!(error = %why, "photo search could not encode its query");
            ApiError::Busy
        }
    })
}

/// Results farther than this (cosine distance) are dropped.
fn max_distance() -> f64 {
    f64::from(1.0 - clip::search_min_cosine())
}

/// How much farther than the query every rival must be (cosine distance).
fn rival_margin() -> f64 {
    f64::from(clip::logits_as_cosine(clip::SEARCH_CONTRAST))
}

/// How a dynamic album's search text matches photos: as photo search, now.
pub(crate) async fn search_match(state: &AppState, text: &str) -> Result<SearchMatch, ApiError> {
    let embedding = encode_query(state, text).await?;
    // The rivals are kept with the rule: saved before the label bank exists
    // (a fresh install computes it first), the rule would keep none for good.
    let bank = label_bank(&state.pool).await?;
    if bank.is_empty() {
        return Err(ApiError::Busy);
    }
    Ok(SearchMatch {
        model: squirrel_ml::CLIP_MODEL_ID.to_owned(),
        rivals: rivals_in(&bank, &embedding),
        embedding,
        max_distance: max_distance(),
        rival_margin: rival_margin(),
    })
}

/// The current label bank's embeddings; empty while it is not computed yet.
async fn label_bank(pool: &sqlx::PgPool) -> Result<Vec<(String, Vec<f32>)>, ApiError> {
    Ok(squirrel_db::clip::label_embeddings(
        pool,
        squirrel_ml::CLIP_MODEL_ID,
        labels::LABELS_VERSION,
        &labels::bank_digest(),
    )
    .await?)
}

/// The label embeddings of `bank` the query must beat on each photo.
fn rivals_in(bank: &[(String, Vec<f32>)], query: &[f32]) -> Vec<Vec<f32>> {
    labels::search_rivals(query, bank, &labels::bank(), clip::QUERY_LABEL_MATCH)
        .into_iter()
        .map(|(_, embedding)| embedding.to_vec())
        .collect()
}

/// The label embeddings the query must beat on each photo (none while the
/// label bank is not computed yet).
async fn rivals(pool: &sqlx::PgPool, query: &[f32]) -> Result<Vec<Vec<f32>>, ApiError> {
    Ok(rivals_in(&label_bank(pool).await?, query))
}

#[utoipa::path(post, path = "/api/search/warmup", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "search",
    responses((status = 204, description = "The text model is loading (or loaded): the next search is fast"),
              (status = 429, body = ErrorBody, description = "Too many warm-ups from this client"),
              (status = 503, body = ErrorBody, description = "`ml_unavailable`: no text model on this server")))]
pub async fn warm_up_search(
    State(state): State<AppState>,
    _caller: Caller,
) -> Result<StatusCode, ApiError> {
    let encoder = state.text_encoder.as_ref().ok_or(ApiError::MlUnavailable)?;
    encoder.warm_up();
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(offset: Option<u32>, limit: Option<u32>) -> PhotoSearchQuery {
        PhotoSearchQuery {
            q: Some("dog".into()),
            order: None,
            offset,
            cursor: None,
            limit,
        }
    }

    #[test]
    fn pages_are_bounded() {
        let relevance = SearchOrder::Relevance;
        assert_eq!(
            page_bounds(&query(None, None), relevance).unwrap(),
            (Start::Offset(0), 50)
        );
        assert_eq!(
            page_bounds(&query(Some(100), Some(100)), relevance).unwrap(),
            (Start::Offset(100), 100)
        );
        for bad in [
            query(None, Some(0)),
            query(None, Some(101)),
            query(Some(150), Some(51)),
        ] {
            assert!(page_bounds(&bad, relevance).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn date_order_pages_by_cursor_only() {
        let date = SearchOrder::Date;
        assert_eq!(
            page_bounds(&query(None, Some(100)), date).unwrap(),
            (Start::After(None), 100)
        );
        assert!(page_bounds(&query(Some(0), None), date).is_err());
        assert!(page_bounds(&query(None, Some(101)), date).is_err());
        let with_cursor = PhotoSearchQuery {
            cursor: Some("x".into()),
            ..query(None, None)
        };
        assert!(page_bounds(&with_cursor, SearchOrder::Relevance).is_err());
    }

    #[test]
    fn orders_are_checked() {
        assert_eq!(SearchOrder::parse(None).unwrap(), SearchOrder::Relevance);
        assert_eq!(
            SearchOrder::parse(Some("relevance")).unwrap(),
            SearchOrder::Relevance
        );
        assert_eq!(SearchOrder::parse(Some("date")).unwrap(), SearchOrder::Date);
        for bad in ["", "Date", "taken_at", "newest"] {
            assert!(matches!(
                SearchOrder::parse(Some(bad)),
                Err(ApiError::Validation(_))
            ));
        }
    }
}
