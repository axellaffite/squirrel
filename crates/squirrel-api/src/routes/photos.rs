//! The photo timeline: every live image file of the caller, by capture time.

use axum::Json;
use axum::extract::State;
use squirrel_db::timeline::{self, Filter, month_bounds};

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiQuery;
use crate::photos_dto::{AssetSummary, MonthCountDto, TimelinePage, TimelineQuery};
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

#[utoipa::path(get, path = "/api/photos", tag = "photos", params(TimelineQuery),
    responses(
        (status = 200, body = TimelinePage),
        (status = 400, body = ErrorBody, description = "Invalid cursor"),
        (status = 422, body = ErrorBody, description = "Invalid month")))]
pub async fn list_photos(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(query): ApiQuery<TimelineQuery>,
) -> Result<Json<TimelinePage>, ApiError> {
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
        current.user.id,
        filter,
        after.as_ref(),
        photo_page_size(query.limit),
    )
    .await?;
    Ok(Json(timeline_page(page)))
}

#[utoipa::path(get, path = "/api/photos/months", tag = "photos",
    responses((status = 200, body = Vec<MonthCountDto>, description = "Newest month first")))]
pub async fn photo_months(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<MonthCountDto>>, ApiError> {
    let months = timeline::months(&state.pool, current.user.id).await?;
    Ok(Json(months.into_iter().map(MonthCountDto::from).collect()))
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
