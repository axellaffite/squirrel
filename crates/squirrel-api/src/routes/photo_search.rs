//! Photo search by text (SigLIP2, any language): the query is encoded by the
//! server's text child, then the caller's own photos are ranked by an exact
//! cosine scan. A query close to a grouped label of the bank ("dog") must
//! also beat that label's siblings ("cat") on each photo.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use squirrel_db::photo_search::{self, PhotoQuery};
use squirrel_ml::clip::{self, labels};
use utoipa::{IntoParams, ToSchema};

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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PhotoSearchQuery {
    /// What to look for, in any language (1–200 characters), e.g. `dog on the beach`
    /// or `chien sur la plage`.
    pub q: Option<String>,
    /// Results to skip; `offset + limit` is at most 200.
    #[param(minimum = 0, maximum = 199)]
    pub offset: Option<u32>,
    /// Page size, 1–100, default 50.
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<u32>,
}

/// The caller's photos that look like the query, best match first. Pages are
/// offset-based and stop at 200 results.
#[derive(Debug, Serialize, ToSchema)]
pub struct PhotoSearchResults {
    pub items: Vec<AssetSummary>,
    /// More results within the first 200: ask again with `offset + limit`.
    pub has_more: bool,
}

/// `(offset, limit)` once checked.
fn page_bounds(query: &PhotoSearchQuery) -> Result<(u32, u32), ApiError> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::Validation(format!("limit must be 1–{MAX_LIMIT}")));
    }
    if offset.saturating_add(limit) > MAX_RESULTS {
        return Err(ApiError::Validation(format!(
            "offset + limit must be at most {MAX_RESULTS}"
        )));
    }
    Ok((offset, limit))
}

/// Search the caller's photos by text (any language).
///
/// A photo is a result when the model's match probability with the query is
/// high enough and, for a query naming a label of the tag bank (`dog`,
/// `chien`), when it beats that label's easily confused siblings (`cat`) on
/// that photo. Pagination is by offset, not by cursor: results are ranked, at most 200
/// can be reached (`offset + limit ≤ 200`), and `has_more` is false past
/// them. Only the caller's own live photos are searched.
#[utoipa::path(get, path = "/api/search/photos", security(("device_bearer" = []), ("session_cookie" = [])), tag = "search", params(PhotoSearchQuery, PhotoFilterQuery),
    responses((status = 200, body = PhotoSearchResults),
              (status = 422, body = ErrorBody, description = "Invalid q, offset, limit or filter"),
              (status = 429, body = ErrorBody, description = "Too many searches from this client"),
              (status = 503, body = ErrorBody, description = "`ml_unavailable`: no text model on this server; `busy`: try again")))]
pub async fn search_photos(
    State(state): State<AppState>,
    caller: Caller,
    RawQuery(raw): RawQuery,
    ApiQuery(query): ApiQuery<PhotoSearchQuery>,
    ApiQuery(filters): ApiQuery<PhotoFilterQuery>,
) -> Result<Json<PhotoSearchResults>, ApiError> {
    known_params(raw.as_deref(), &with_filters(&["q", "offset", "limit"]))?;
    let (offset, limit) = page_bounds(&query)?;
    let filter = filters.filter()?;
    let encoder = state.text_encoder.as_ref().ok_or(ApiError::MlUnavailable)?;
    let embedding = encoder
        .encode(query.q.as_deref().unwrap_or_default())
        .await
        .map_err(|err| match err {
            EncodeError::Length => ApiError::Validation(err.to_string()),
            EncodeError::Unavailable(why) => {
                tracing::warn!(error = %why, "photo search could not encode its query");
                ApiError::Busy
            }
        })?;
    let rivals = rivals(&state.pool, &embedding).await?;
    let page = photo_search::search(
        &state.pool,
        caller.user.id,
        &PhotoQuery {
            model: squirrel_ml::CLIP_MODEL_ID,
            embedding: &embedding,
            max_distance: f64::from(1.0 - clip::search_min_cosine()),
            relative_cutoff: f64::from(clip::logits_as_cosine(clip::SEARCH_RELATIVE_CUTOFF)),
            rivals: &rivals,
            rival_margin: f64::from(clip::logits_as_cosine(clip::SEARCH_CONTRAST)),
            filter: &filter,
        },
        offset,
        limit,
    )
    .await?;
    Ok(Json(PhotoSearchResults {
        items: page
            .items
            .iter()
            .map(|hit| AssetSummary::from(&hit.summary))
            .collect(),
        has_more: page.has_more && offset + limit < MAX_RESULTS,
    }))
}

/// The label embeddings the query must beat on each photo (none while the
/// label bank is not computed yet).
async fn rivals(pool: &sqlx::PgPool, query: &[f32]) -> Result<Vec<Vec<f32>>, ApiError> {
    let bank = squirrel_db::clip::label_embeddings(
        pool,
        squirrel_ml::CLIP_MODEL_ID,
        labels::LABELS_VERSION,
        &labels::bank_digest(),
    )
    .await?;
    Ok(
        labels::search_rivals(query, &bank, &labels::bank(), clip::QUERY_LABEL_MATCH)
            .into_iter()
            .map(|(_, embedding)| embedding.to_vec())
            .collect(),
    )
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
            offset,
            limit,
        }
    }

    #[test]
    fn pages_are_bounded() {
        assert_eq!(page_bounds(&query(None, None)).unwrap(), (0, 50));
        assert_eq!(
            page_bounds(&query(Some(100), Some(100))).unwrap(),
            (100, 100)
        );
        for bad in [
            query(None, Some(0)),
            query(None, Some(101)),
            query(Some(150), Some(51)),
        ] {
            assert!(page_bounds(&bad).is_err(), "{bad:?}");
        }
    }
}
