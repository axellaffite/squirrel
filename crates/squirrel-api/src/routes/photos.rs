//! The photo timeline: every live image file of the caller, by capture time.

use axum::Json;
use axum::extract::{RawQuery, State};
use squirrel_db::albums;
use squirrel_db::timeline::{self, Filter, month_bounds};
use uuid::Uuid;

use crate::access;
use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::filter_dto::{PhotoFilterQuery, with_filters};
use crate::input::{ApiPath, ApiQuery, known_params};
use crate::people_dto::PersonRef;
use crate::photos_dto::{
    AlbumRef, AssetDetails, AssetSummary, MediaStatus, MlStatusDto, MonthCountDto, TimelinePage,
    TimelineQuery,
};
use crate::state::AppState;

pub const DEFAULT_PHOTO_PAGE: u32 = 200;
pub const MAX_PHOTO_PAGE: u32 = 500;

pub fn photo_page_size(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_PHOTO_PAGE)
        .clamp(1, MAX_PHOTO_PAGE)
}

/// `YYYY-MM` (exactly), month 01–12.
pub fn parse_month(raw: &str) -> Option<(i32, u8)> {
    let (year, month) = raw.split_once('-')?;
    if year.len() != 4 || month.len() != 2 {
        return None;
    }
    if !year
        .bytes()
        .chain(month.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let parsed = (year.parse().ok()?, month.parse().ok()?);
    month_bounds(parsed.0, parsed.1).map(|_| parsed)
}

/// A page of [`timeline::page`]-like results, as returned to clients.
pub fn timeline_page(page: squirrel_db::Page<timeline::AssetSummaryRecord>) -> TimelinePage {
    let next_cursor = page
        .items
        .last()
        .filter(|_| page.has_more)
        .map(|last| cursor::encode_timeline(&last.cursor()));
    TimelinePage {
        items: page.items.iter().map(AssetSummary::from).collect(),
        next_cursor,
    }
}

#[utoipa::path(get, path = "/api/photos", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos", params(TimelineQuery, PhotoFilterQuery),
    responses(
        (status = 200, body = TimelinePage),
        (status = 400, body = ErrorBody, description = "Invalid cursor"),
        (status = 422, body = ErrorBody, description = "Invalid month or filter")))]
pub async fn list_photos(
    State(state): State<AppState>,
    caller: Caller,
    RawQuery(raw): RawQuery,
    ApiQuery(query): ApiQuery<TimelineQuery>,
    ApiQuery(filters): ApiQuery<PhotoFilterQuery>,
) -> Result<Json<TimelinePage>, ApiError> {
    known_params(raw.as_deref(), &with_filters(&["month", "cursor", "limit"]))?;
    let photos = filters.filter()?;
    let filter = match query.month.as_deref() {
        None => Filter::All,
        Some(raw) => {
            let (year, month) = parse_month(raw)
                .ok_or_else(|| ApiError::Validation("month must be YYYY-MM".into()))?;
            Filter::Month(year, month)
        }
    };
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?;
    let page = timeline::page(
        &state.pool,
        caller.user.id,
        filter,
        &photos,
        after.as_ref(),
        photo_page_size(query.limit),
    )
    .await?;
    Ok(Json(timeline_page(page)))
}

#[utoipa::path(get, path = "/api/photos/months", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos", params(PhotoFilterQuery),
    responses((status = 200, body = Vec<MonthCountDto>, description = "Newest month first"),
              (status = 422, body = ErrorBody, description = "Invalid filter")))]
pub async fn photo_months(
    State(state): State<AppState>,
    caller: Caller,
    RawQuery(raw): RawQuery,
    ApiQuery(filters): ApiQuery<PhotoFilterQuery>,
) -> Result<Json<Vec<MonthCountDto>>, ApiError> {
    known_params(raw.as_deref(), &with_filters(&[]))?;
    let months = timeline::months(&state.pool, caller.user.id, &filters.filter()?).await?;
    Ok(Json(months.into_iter().map(MonthCountDto::from).collect()))
}

#[utoipa::path(get, path = "/api/photos/{id}", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos",
    params(("id" = Uuid, Path, description = "The photo's file node id")),
    responses((status = 200, body = AssetDetails), (status = 404, body = ErrorBody)))]
pub async fn get_photo(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<AssetDetails>, ApiError> {
    let found = access::file(&state, caller.user.id, id).await?;
    let mut record = timeline::detail(&state.pool, found.node.owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !found.access.is_owner() {
        // Someone else's photo: its own facts only. The owner's albums, tags,
        // people and folders stay private.
        record.parent_id = None;
        let details = AssetDetails::new(&record, Vec::new(), Vec::new(), Vec::new());
        return Ok(Json(details.seen_with(found.access.into())));
    }
    let albums = albums::albums_for_node(&state.pool, caller.user.id, id)
        .await?
        .into_iter()
        .map(|album| AlbumRef {
            id: album.id,
            name: album.name,
        })
        .collect();
    let tags = squirrel_db::explore::tags_of_photo(&state.pool, caller.user.id, id).await?;
    let people = squirrel_db::people::people_on(&state.pool, caller.user.id, id)
        .await?
        .into_iter()
        .map(|(id, name)| PersonRef { id, name })
        .collect();
    Ok(Json(AssetDetails::new(&record, albums, tags, people)))
}

#[utoipa::path(get, path = "/api/media/status", security(("device_bearer" = []), ("session_cookie" = [])), tag = "photos",
    responses((status = 200, body = MediaStatus)))]
pub async fn media_status(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<MediaStatus>, ApiError> {
    let counts = timeline::status_counts(&state.pool, caller.user.id).await?;
    let ml = squirrel_db::ml_status::current(&state.pool).await?;
    let ml_counts = squirrel_db::explore::ml_counts(&state.pool, caller.user.id).await?;
    let ml = MlStatusDto::new(ml, ml_counts, state.models_installed);
    Ok(Json(MediaStatus::new(counts, ml)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_are_strictly_yyyy_mm() {
        assert_eq!(parse_month("2024-07"), Some((2024, 7)));
        assert_eq!(parse_month("1999-12"), Some((1999, 12)));
        for bad in [
            "2024-7",
            "2024-00",
            "2024-13",
            "abcd-ef",
            "24-07",
            "2024-07-01",
            "+024-07",
            "",
            "-",
        ] {
            assert_eq!(parse_month(bad), None, "{bad}");
        }
    }

    #[test]
    fn page_sizes_are_clamped() {
        assert_eq!(photo_page_size(None), 200);
        assert_eq!(photo_page_size(Some(0)), 1);
        assert_eq!(photo_page_size(Some(10_000)), 500);
    }
}
