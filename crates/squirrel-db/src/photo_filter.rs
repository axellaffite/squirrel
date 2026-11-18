//! Filters shared by the timeline, its months and photo search: each one is
//! a column predicate or an `EXISTS` on the caller's own assets (`a`, joined
//! to `n` and `m` by [`crate::timeline`]'s `FROM`), built from fixed SQL
//! fragments and binds only.

use sqlx::{Postgres, QueryBuilder};

use uuid::Uuid;

use crate::assets::AssetKind;

/// One value per filter, combined with AND. Checked by the API (place parts
/// need their parents, a camera model its make).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhotoFilter {
    /// One of the owner's people: photos with one of their faces.
    pub person: Option<Uuid>,
    /// A tag slug of any source.
    pub tag: Option<String>,
    /// ISO 3166 country code, as geocoded.
    pub country: Option<String>,
    /// First-level region (`place_admin1`), within `country`. Empty: places
    /// without a region (`place_admin1 IS NULL`), as Explore lists them.
    pub region: Option<String>,
    /// Place name, within `country` (and `region` when given).
    pub city: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub kind: Option<AssetKind>,
}

impl PhotoFilter {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Appends ` AND …` for each filter set.
    pub fn push(&self, query: &mut QueryBuilder<'_, Postgres>) {
        if let Some(person) = self.person {
            // Faces are the owner's own: another user's person matches nothing.
            query
                .push(
                    " AND EXISTS (SELECT 1 FROM faces f \
                       WHERE f.owner_id = a.owner_id AND f.sha256 = a.sha256 AND f.person_id = ",
                )
                .push_bind(person)
                .push(")");
        }
        if let Some(tag) = &self.tag {
            query
                .push(
                    " AND EXISTS (SELECT 1 FROM asset_tags t \
                       WHERE t.sha256 = a.sha256 AND t.tag = ",
                )
                .push_bind(tag.clone())
                .push(")");
        }
        if self.region.as_deref() == Some("") {
            query.push(" AND m.place_admin1 IS NULL");
        }
        let region = self.region.as_ref().filter(|r| !r.is_empty());
        let columns = [
            ("m.place_country_code", &self.country),
            ("m.place_admin1", &region.cloned()),
            ("m.place_name", &self.city),
            ("m.camera_make", &self.camera_make),
            ("m.camera_model", &self.camera_model),
        ];
        for (column, value) in columns {
            if let Some(value) = value {
                query
                    .push(" AND ")
                    .push(column)
                    .push(" = ")
                    .push_bind(value.clone());
            }
        }
        if let Some(kind) = self.kind {
            query.push(" AND a.kind = ").push_bind(kind.as_str());
        }
    }
}
