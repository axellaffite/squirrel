//! The photo filters of the timeline, its months and photo search, as query
//! parameters, checked into a [`PhotoFilter`].

use serde::Deserialize;
use squirrel_db::assets::AssetKind;
use squirrel_db::photo_filter::PhotoFilter;
use utoipa::{IntoParams, ToSchema};

use crate::error::ApiError;

/// Longest filter value accepted (place and camera names are shorter).
const MAX_VALUE_CHARS: usize = 200;

/// What a photo is, for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PhotoKindFilter {
    Photo,
    Screenshot,
}

/// One value per filter, combined with AND. An unknown value matches nothing
/// (filters are queries, not lookups).
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PhotoFilterQuery {
    /// A tag (`GET /api/tags`), e.g. `beach`.
    pub tag: Option<String>,
    /// A country code (`GET /api/places`), e.g. `FR`.
    pub country: Option<String>,
    /// A region of `country`; needs `country`.
    pub region: Option<String>,
    /// A place of `country` (and `region` when given); needs `country`.
    pub city: Option<String>,
    /// A camera make (`GET /api/cameras`).
    pub camera_make: Option<String>,
    /// A camera model; needs `camera_make`.
    pub camera_model: Option<String>,
    pub kind: Option<PhotoKindFilter>,
}

impl PhotoFilterQuery {
    /// The checked filter: dependent parts need their parent, tags are slugs.
    pub fn filter(&self) -> Result<PhotoFilter, ApiError> {
        let value = |name: &str, raw: &Option<String>| -> Result<Option<String>, ApiError> {
            match raw.as_deref().map(str::trim) {
                None => Ok(None),
                Some(v) if v.is_empty() || v.chars().count() > MAX_VALUE_CHARS => Err(
                    ApiError::Validation(format!("{name} must be 1–{MAX_VALUE_CHARS} characters")),
                ),
                Some(v) => Ok(Some(v.to_owned())),
            }
        };
        let tag = value("tag", &self.tag)?;
        if tag
            .as_deref()
            .is_some_and(|t| !squirrel_ml::clip::labels::is_slug(t))
        {
            return Err(ApiError::Validation("tag must be a tag slug".into()));
        }
        let filter = PhotoFilter {
            tag,
            country: value("country", &self.country)?,
            region: value("region", &self.region)?,
            city: value("city", &self.city)?,
            camera_make: value("camera_make", &self.camera_make)?,
            camera_model: value("camera_model", &self.camera_model)?,
            kind: self.kind.map(|kind| match kind {
                PhotoKindFilter::Photo => AssetKind::Photo,
                PhotoKindFilter::Screenshot => AssetKind::Screenshot,
            }),
        };
        if (filter.region.is_some() || filter.city.is_some()) && filter.country.is_none() {
            return Err(ApiError::Validation("region and city need country".into()));
        }
        if filter.camera_model.is_some() && filter.camera_make.is_none() {
            return Err(ApiError::Validation(
                "camera_model needs camera_make".into(),
            ));
        }
        Ok(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> PhotoFilterQuery {
        PhotoFilterQuery::default()
    }

    #[test]
    fn dependent_filters_need_their_parent() {
        assert!(query().filter().unwrap().is_empty());
        let city = PhotoFilterQuery {
            city: Some("Paris".into()),
            ..query()
        };
        assert!(city.filter().is_err());
        let model = PhotoFilterQuery {
            camera_model: Some("X100".into()),
            ..query()
        };
        assert!(model.filter().is_err());
        let ok = PhotoFilterQuery {
            country: Some(" FR ".into()),
            city: Some("Paris".into()),
            camera_make: Some("Fujifilm".into()),
            camera_model: Some("X100".into()),
            kind: Some(PhotoKindFilter::Photo),
            ..query()
        };
        let filter = ok.filter().unwrap();
        assert_eq!(filter.country.as_deref(), Some("FR"));
        assert_eq!(filter.kind, Some(AssetKind::Photo));
    }

    #[test]
    fn tags_are_slugs_and_values_bounded() {
        for bad in ["Dog", "dog cat", ""] {
            let q = PhotoFilterQuery {
                tag: Some(bad.into()),
                ..query()
            };
            assert!(q.filter().is_err(), "{bad:?}");
        }
        let long = PhotoFilterQuery {
            country: Some("x".repeat(201)),
            ..query()
        };
        assert!(long.filter().is_err());
    }
}
