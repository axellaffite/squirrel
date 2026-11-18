//! Search over the caller's documents: file names and extracted text.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use squirrel_db::documents;
use squirrel_db::media;

use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::files_dto::NodeDto;
use crate::input::ApiQuery;
use crate::search_dto::{DocumentHit, DocumentResults, SearchQuery, snippet_parts};
use crate::state::AppState;

pub const MAX_QUERY_CHARS: usize = 200;
/// Results reachable through `offset` and `limit`, at most.
pub const MAX_RESULTS: u32 = 200;
pub const DEFAULT_LIMIT: u32 = 20;
pub const MAX_LIMIT: u32 = 50;

/// `(query, offset, limit)` once checked.
fn validate(query: &SearchQuery) -> Result<(&str, u32, u32), ApiError> {
    let q = query.q.as_deref().unwrap_or_default().trim();
    let chars = q.chars().count();
    if chars == 0 || chars > MAX_QUERY_CHARS {
        return Err(ApiError::Validation(format!(
            "q must be 1–{MAX_QUERY_CHARS} characters"
        )));
    }
    let offset = query.offset.unwrap_or(0);
    if offset > MAX_RESULTS {
        return Err(ApiError::Validation(format!(
            "offset must be at most {MAX_RESULTS}"
        )));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::Validation(format!("limit must be 1–{MAX_LIMIT}")));
    }
    if offset + limit > MAX_RESULTS {
        return Err(ApiError::Validation(format!(
            "offset + limit must be at most {MAX_RESULTS}"
        )));
    }
    Ok((q, offset, limit))
}

#[utoipa::path(get, path = "/api/search/documents", security(("device_bearer" = []), ("session_cookie" = [])), tag = "search", params(SearchQuery),
    responses((status = 200, body = DocumentResults),
              (status = 422, body = ErrorBody, description = "Invalid q, offset or limit"),
              (status = 429, body = ErrorBody, description = "Too many searches from this client")))]
pub async fn search_documents(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<SearchQuery>,
) -> Result<Json<DocumentResults>, ApiError> {
    let (q, offset, limit) = validate(&query)?;
    let page = documents::search(&state.pool, caller.user.id, q, offset, limit).await?;
    let hashes: Vec<String> = page
        .items
        .iter()
        .filter_map(|hit| hit.node.blob_sha256.clone())
        .collect();
    let thumbnails = if hashes.is_empty() {
        HashMap::new()
    } else {
        media::thumbnails_for(&state.pool, &hashes).await?
    };
    let items = page
        .items
        .iter()
        .map(|hit| DocumentHit {
            node: NodeDto::with_thumbnails(&hit.node, &thumbnails),
            page_count: hit.page_count,
            snippet: hit
                .snippet
                .as_deref()
                .map(snippet_parts)
                .unwrap_or_default(),
        })
        .collect();
    Ok(Json(DocumentResults {
        items,
        has_more: page.has_more && offset + limit < MAX_RESULTS,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(q: Option<&str>, offset: Option<u32>, limit: Option<u32>) -> SearchQuery {
        SearchQuery {
            q: q.map(str::to_owned),
            offset,
            limit,
        }
    }

    #[test]
    fn queries_and_pages_are_bounded() {
        assert_eq!(
            validate(&query(Some(" ecole "), None, None)).unwrap(),
            ("ecole", 0, 20)
        );
        for bad in [
            query(None, None, None),
            query(Some("   "), None, None),
            query(Some(&"x".repeat(201)), None, None),
            query(Some("ok"), Some(201), None),
            query(Some("ok"), None, Some(0)),
            query(Some("ok"), None, Some(51)),
            query(Some("ok"), Some(180), Some(21)),
        ] {
            assert!(validate(&bad).is_err(), "{bad:?}");
        }
        assert!(validate(&query(Some(&"é".repeat(200)), Some(150), Some(50))).is_ok());
    }
}
