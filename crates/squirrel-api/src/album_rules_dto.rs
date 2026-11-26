//! The rules of a dynamic album as JSON, checked into [`AlbumRules`].

use serde::{Deserialize, Serialize};
use squirrel_db::album_rules::{AlbumRules, MAX_VALUES, Match};
use squirrel_db::assets::AssetKind;
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::filter_dto::{PhotoFilterQuery, PhotoKindFilter};

time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// How a group of people or tags matches a photo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// At least one of them is on the photo.
    #[default]
    Any,
    /// Every one of them is on the photo.
    All,
}

impl From<MatchMode> for Match {
    fn from(mode: MatchMode) -> Self {
        match mode {
            MatchMode::Any => Self::Any,
            MatchMode::All => Self::All,
        }
    }
}

impl From<Match> for MatchMode {
    fn from(mode: Match) -> Self {
        match mode {
            Match::Any => Self::Any,
            Match::All => Self::All,
        }
    }
}

/// Which photos a dynamic album holds. Every condition given must hold (AND);
/// an empty list or an absent field does not constrain. At least one
/// condition is required.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AlbumRulesDto {
    /// The caller's people (`GET /api/people`), at most 20.
    #[serde(default)]
    pub people: Vec<Uuid>,
    /// Whether a photo needs any or all of `people`.
    #[serde(default)]
    pub people_match: MatchMode,
    /// Tags (`GET /api/tags`), at most 20.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether a photo needs any or all of `tags`.
    #[serde(default)]
    pub tags_match: MatchMode,
    /// A country code (`GET /api/places`), e.g. `FR`.
    #[serde(default)]
    pub country: Option<String>,
    /// A region of `country`; needs `country`. Empty: places without a region.
    #[serde(default)]
    pub region: Option<String>,
    /// A place of `country`; needs `country`.
    #[serde(default)]
    pub city: Option<String>,
    /// A camera make (`GET /api/cameras`).
    #[serde(default)]
    pub camera_make: Option<String>,
    /// A camera model; needs `camera_make`.
    #[serde(default)]
    pub camera_model: Option<String>,
    #[serde(default)]
    pub kind: Option<PhotoKindFilter>,
    /// First capture day (wall clock), inclusive: `YYYY-MM-DD`.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub taken_from: Option<Date>,
    /// Last capture day, inclusive: `YYYY-MM-DD`.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub taken_to: Option<Date>,
}

impl AlbumRulesDto {
    /// The checked rules. Whether `people` are the caller's is checked by the
    /// route (it needs the database).
    pub fn rules(&self) -> Result<AlbumRules, ApiError> {
        if self.people.len() > MAX_VALUES || self.tags.len() > MAX_VALUES {
            return Err(ApiError::Validation(format!(
                "a rule names at most {MAX_VALUES} people and {MAX_VALUES} tags"
            )));
        }
        let tags: Vec<String> = self.tags.iter().map(|t| t.trim().to_owned()).collect();
        if !tags.iter().all(|t| squirrel_ml::clip::labels::is_slug(t)) {
            return Err(ApiError::Validation("tags must be tag slugs".into()));
        }
        // Place, camera and kind are checked as the photo filters are.
        let filter = PhotoFilterQuery {
            country: self.country.clone(),
            region: self.region.clone(),
            city: self.city.clone(),
            camera_make: self.camera_make.clone(),
            camera_model: self.camera_model.clone(),
            kind: self.kind,
            ..PhotoFilterQuery::default()
        }
        .filter()?;
        if let (Some(from), Some(to)) = (self.taken_from, self.taken_to)
            && from > to
        {
            return Err(ApiError::Validation(
                "taken_from must not be after taken_to".into(),
            ));
        }
        let rules = AlbumRules {
            people: self.people.clone(),
            people_match: self.people_match.into(),
            tags,
            tags_match: self.tags_match.into(),
            country: filter.country,
            region: filter.region,
            city: filter.city,
            camera_make: filter.camera_make,
            camera_model: filter.camera_model,
            kind: filter.kind,
            taken_from: self.taken_from,
            taken_to: self.taken_to,
        };
        if rules.is_empty() {
            return Err(ApiError::Validation(
                "a dynamic album needs at least one condition".into(),
            ));
        }
        Ok(rules)
    }
}

impl From<&AlbumRules> for AlbumRulesDto {
    fn from(rules: &AlbumRules) -> Self {
        Self {
            people: rules.people.clone(),
            people_match: rules.people_match.into(),
            tags: rules.tags.clone(),
            tags_match: rules.tags_match.into(),
            country: rules.country.clone(),
            region: rules.region.clone(),
            city: rules.city.clone(),
            camera_make: rules.camera_make.clone(),
            camera_model: rules.camera_model.clone(),
            kind: rules.kind.map(|kind| match kind {
                AssetKind::Photo => PhotoKindFilter::Photo,
                AssetKind::Screenshot => PhotoKindFilter::Screenshot,
            }),
            taken_from: rules.taken_from,
            taken_to: rules.taken_to,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn parse(json: &str) -> Result<AlbumRules, String> {
        let dto: AlbumRulesDto = serde_json::from_str(json).map_err(|e| e.to_string())?;
        dto.rules().map_err(|e| e.to_string())
    }

    #[test]
    fn a_rule_needs_a_condition() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"people_match": "all"}"#).is_err());
        let rules = parse(r#"{"tags": [" beach "], "tags_match": "all"}"#).unwrap();
        assert_eq!(rules.tags, ["beach"]);
        assert_eq!(rules.tags_match, Match::All);
    }

    #[test]
    fn dates_are_days_in_order() {
        let rules = parse(r#"{"taken_from": "2024-07-01", "taken_to": "2024-07-01"}"#).unwrap();
        assert_eq!(rules.taken_from, Some(date!(2024 - 07 - 01)));
        assert!(parse(r#"{"taken_from": "2024-07-02", "taken_to": "2024-07-01"}"#).is_err());
        assert!(parse(r#"{"taken_from": "2024-07-01T10:00:00Z"}"#).is_err());
    }

    #[test]
    fn values_are_checked_like_photo_filters() {
        assert!(parse(r#"{"tags": ["Not A Slug"]}"#).is_err());
        assert!(parse(r#"{"city": "Paris"}"#).is_err(), "city needs country");
        assert!(parse(r#"{"camera_model": "X100"}"#).is_err());
        assert!(parse(r#"{"colour": "red"}"#).is_err(), "unknown field");
        let many: Vec<String> = (0..21).map(|i| format!("\"t{i}\"")).collect();
        assert!(parse(&format!(r#"{{"tags": [{}]}}"#, many.join(","))).is_err());
        let region = parse(r#"{"country": "CH", "region": ""}"#).unwrap();
        assert_eq!(region.region.as_deref(), Some(""));
    }

    #[test]
    fn rules_round_trip_through_json() {
        let dto: AlbumRulesDto =
            serde_json::from_str(r#"{"tags": ["sea"], "kind": "photo", "taken_to": "2024-12-31"}"#)
                .unwrap();
        let back = AlbumRulesDto::from(&dto.rules().unwrap());
        assert_eq!(back, dto);
        let json = serde_json::to_value(&back).unwrap();
        assert_eq!(json["taken_to"], "2024-12-31");
        assert_eq!(json["people_match"], "any");
    }
}
