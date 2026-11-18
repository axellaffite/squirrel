//! Response bodies of Explore: tags, places and cameras.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::photos_dto::{AssetSummary, LocationDto};

/// A tag of the caller's photos.
#[derive(Debug, Serialize, ToSchema)]
pub struct TagGroupDto {
    /// Stable English slug, e.g. `beach` (display names are the UI's).
    pub tag: String,
    pub count: i64,
    /// The newest photo with this tag.
    pub cover: AssetSummary,
}

/// How places are grouped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaceLevel {
    Country,
    #[default]
    City,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlacesQuery {
    /// `city` (default): one entry per place; `country`: per country.
    pub level: Option<PlaceLevel>,
}

/// A place the caller's photos were taken (filter with `country`, `region`
/// and `city` on `GET /api/photos`).
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceGroupDto {
    pub country_code: String,
    pub country: Option<String>,
    /// `null` at country level, or for a place without a region.
    pub region: Option<String>,
    /// `null` at country level.
    pub city: Option<String>,
    pub count: i64,
    /// The newest photo taken there.
    pub cover: AssetSummary,
    /// Where the cover photo was taken, for a locator dot.
    pub location: Option<LocationDto>,
}

/// A camera the caller's photos were taken with.
#[derive(Debug, Serialize, ToSchema)]
pub struct CameraGroupDto {
    pub make: String,
    pub model: Option<String>,
    pub count: i64,
}
