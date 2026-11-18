//! Explore: the caller's tags, places and cameras, each with a count and
//! (tags, places) the newest photo as cover.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use squirrel_db::explore;
use squirrel_db::timeline::{self, AssetSummaryRecord};
use uuid::Uuid;

use crate::error::ApiError;
use crate::explore_dto::{CameraGroupDto, PlaceGroupDto, PlaceLevel, PlacesQuery, TagGroupDto};
use crate::extract::Caller;
use crate::input::ApiQuery;
use crate::photos_dto::AssetSummary;
use crate::state::AppState;

/// The caller's summaries of `covers`, by node id.
async fn covers(
    state: &AppState,
    owner: Uuid,
    covers: Vec<Uuid>,
) -> Result<HashMap<Uuid, AssetSummaryRecord>, ApiError> {
    Ok(timeline::summaries(&state.pool, owner, &covers)
        .await?
        .into_iter()
        .map(|record| (record.node_id, record))
        .collect())
}

#[utoipa::path(get, path = "/api/tags", security(("device_bearer" = []), ("session_cookie" = [])), tag = "explore",
    responses((status = 200, body = Vec<TagGroupDto>, description = "Most used first, at most 200")))]
pub async fn list_tags(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<TagGroupDto>>, ApiError> {
    let tags = explore::tags(&state.pool, caller.user.id).await?;
    let covers = covers(
        &state,
        caller.user.id,
        tags.iter().map(|t| t.cover).collect(),
    )
    .await?;
    Ok(Json(
        tags.into_iter()
            .filter_map(|tag| {
                let cover = covers.get(&tag.cover)?;
                Some(TagGroupDto {
                    tag: tag.tag,
                    count: tag.count,
                    cover: AssetSummary::from(cover),
                })
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/api/places", security(("device_bearer" = []), ("session_cookie" = [])), tag = "explore", params(PlacesQuery),
    responses((status = 200, body = Vec<PlaceGroupDto>, description = "Most photographed first, at most 200")))]
pub async fn list_places(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<PlacesQuery>,
) -> Result<Json<Vec<PlaceGroupDto>>, ApiError> {
    let level = match query.level.unwrap_or_default() {
        PlaceLevel::City => explore::PlaceLevel::City,
        PlaceLevel::Country => explore::PlaceLevel::Country,
    };
    let places = explore::places(&state.pool, caller.user.id, level).await?;
    let covers = covers(
        &state,
        caller.user.id,
        places.iter().map(|p| p.cover).collect(),
    )
    .await?;
    Ok(Json(
        places
            .into_iter()
            .filter_map(|place| {
                let cover = covers.get(&place.cover)?;
                Some(PlaceGroupDto {
                    country_code: place.country_code,
                    country: place.country,
                    region: place.region,
                    city: place.city,
                    count: place.count,
                    cover: AssetSummary::from(cover),
                })
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/api/cameras", security(("device_bearer" = []), ("session_cookie" = [])), tag = "explore",
    responses((status = 200, body = Vec<CameraGroupDto>, description = "Most used first, at most 200")))]
pub async fn list_cameras(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<Vec<CameraGroupDto>>, ApiError> {
    let cameras = explore::cameras(&state.pool, caller.user.id).await?;
    Ok(Json(
        cameras
            .into_iter()
            .map(|camera| CameraGroupDto {
                make: camera.make,
                model: camera.model,
                count: camera.count,
            })
            .collect(),
    ))
}
