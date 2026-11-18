//! People and faces as the API shows them.

use serde::{Deserialize, Serialize};
use squirrel_db::people::{FaceRow, PersonRecord};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::albums_dto::present;
use crate::error::ApiError;
use crate::photos_dto::format_taken_at;

/// Most people merged at once.
pub const MAX_MERGE: usize = 50;
/// Most faces split off at once.
pub const MAX_SPLIT: usize = 500;
/// Longest person name, in characters.
pub const MAX_NAME_CHARS: usize = 100;

/// A group of the caller's faces, named or not.
#[derive(Debug, Serialize, ToSchema)]
pub struct PersonDto {
    pub id: Uuid,
    pub name: Option<String>,
    /// Left out of lists unless asked for; still clustered.
    pub hidden: bool,
    /// Faces on photos outside the trash.
    pub face_count: i64,
    /// The face to show for this person: the one chosen with `cover_face_id`,
    /// else the best-scoring face on a photo with a ready thumbnail. `null`
    /// for a person without a face on a live photo.
    pub cover: Option<FaceCrop>,
}

impl From<PersonRecord> for PersonDto {
    fn from(record: PersonRecord) -> Self {
        let c = record.cover;
        let cover = match (c.cover_face_id, c.cover_photo_id) {
            (Some(face_id), Some(photo_id)) => Some(FaceCrop {
                face_id,
                photo_id,
                thumbnail_version: c.cover_thumbs_version,
                bbox: FaceBoxDto {
                    x: c.cover_box_x.unwrap_or_default(),
                    y: c.cover_box_y.unwrap_or_default(),
                    width: c.cover_box_w.unwrap_or_default(),
                    height: c.cover_box_h.unwrap_or_default(),
                },
            }),
            _ => None,
        };
        Self {
            id: record.id,
            name: record.name,
            hidden: record.hidden,
            face_count: record.face_count,
            cover,
        }
    }
}

/// How to show a face. There is no face image on the server: a client crops
/// the photo's thumbnail itself, `GET /api/files/{photo_id}/thumbnail?size=thumb&v={thumbnail_version}`,
/// to `box` (e.g. CSS `object-fit`/`object-position`, or a Compose crop). The
/// thumb is 360 px tall: for a face smaller than about 120 px on the 1440 px
/// preview (`box.height` under about 0.08), crop `size=preview` instead.
#[derive(Debug, Serialize, ToSchema)]
pub struct FaceCrop {
    pub face_id: Uuid,
    /// The photo (file node) whose thumbnail holds the face.
    pub photo_id: Uuid,
    /// The thumbnail's `v`; `null` while the photo has no thumbnail yet.
    pub thumbnail_version: Option<i16>,
    #[serde(rename = "box")]
    pub bbox: FaceBoxDto,
}

/// A person as other objects refer to it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonRef {
    pub id: Uuid,
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PeopleQuery {
    /// Also list hidden people (default `false`).
    pub include_hidden: Option<bool>,
}

/// Rename (or clear the name), hide or choose the cover. At least one change
/// is required.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePersonRequest {
    /// A new name, 1–100 characters once trimmed.
    #[serde(default)]
    pub name: Option<String>,
    /// `true` removes the name (not with `name`).
    #[serde(default)]
    pub clear_name: bool,
    #[serde(default)]
    pub hidden: Option<bool>,
    /// One of this person's faces; `null` goes back to the best one.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub cover_face_id: Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MergePeopleRequest {
    /// People (1–50) whose faces move to this one; they are deleted.
    pub from: Vec<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SplitPersonRequest {
    /// Faces of this person (1–500) that become a new person.
    pub face_ids: Vec<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NewPersonDto {
    #[serde(default)]
    pub name: Option<String>,
}

/// Where a face goes: an existing person (`person_id`) or a new one
/// (`new_person`). Exactly one.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignFaceRequest {
    #[serde(default)]
    pub person_id: Option<Uuid>,
    #[serde(default)]
    pub new_person: Option<NewPersonDto>,
}

/// Who decided a face's person.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FaceAssignedBy {
    /// Clustering. Once grouped, only an operator's
    /// `squirrel admin faces recluster` (after a threshold change) moves it.
    Auto,
    /// The user; clustering never moves it (a `null` person: "not this
    /// person", never grouped again). Deleting the photo for good drops the
    /// face and its pin.
    User,
}

/// A face's box, 0–1 from the top left, in displayed coordinates: the photo as
/// shown after its EXIF orientation, which is also the frame of every thumbnail
/// size (`thumb`, `preview`). A client crops a thumbnail with it as is.
#[derive(Debug, Serialize, ToSchema)]
pub struct FaceBoxDto {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A face. Show it by cropping the photo's thumbnail to `box` (see
/// [`FaceCrop`]): the server stores no face images.
#[derive(Debug, Serialize, ToSchema)]
pub struct FaceDto {
    pub id: Uuid,
    /// The photo it is on (`GET /api/photos/{id}`).
    pub photo_id: Uuid,
    /// The photo thumbnail's `v`; `null` while it has none.
    pub thumbnail_version: Option<i16>,
    /// When the photo was taken (wall clock, no offset), e.g. `2024-07-14T18:30:05`.
    pub taken_at: String,
    #[serde(rename = "box")]
    pub bbox: FaceBoxDto,
    /// Detector confidence, 0–1.
    pub score: f32,
    pub person: Option<PersonRef>,
    pub assigned_by: FaceAssignedBy,
}

impl From<FaceRow> for FaceDto {
    fn from(row: FaceRow) -> Self {
        Self {
            id: row.id,
            photo_id: row.photo_id,
            thumbnail_version: row.thumbs_version,
            taken_at: format_taken_at(row.taken_at),
            bbox: FaceBoxDto {
                x: row.box_x,
                y: row.box_y,
                width: row.box_w,
                height: row.box_h,
            },
            score: row.score,
            person: row.person_id.map(|id| PersonRef {
                id,
                name: row.person_name,
            }),
            assigned_by: if row.assigned_by == "user" {
                FaceAssignedBy::User
            } else {
                FaceAssignedBy::Auto
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FacesPage {
    /// Newest photo first.
    pub items: Vec<FaceDto>,
    /// Pass back as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FacesQuery {
    pub cursor: Option<String>,
    /// 1–500, default 100.
    pub limit: Option<u32>,
}

/// A checked person name: trimmed, 1–100 characters, no control character.
pub fn person_name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim();
    let chars = name.chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(ApiError::Validation(format!(
            "name must be 1–{MAX_NAME_CHARS} characters, without control characters"
        )));
    }
    Ok(name.to_owned())
}

/// Between 1 and `max` distinct ids.
pub fn check_ids(field: &str, ids: &[Uuid], max: usize) -> Result<(), ApiError> {
    let distinct = ids.iter().collect::<std::collections::HashSet<_>>().len();
    if ids.is_empty() || ids.len() > max || distinct != ids.len() {
        return Err(ApiError::Validation(format!(
            "{field} must hold 1–{max} distinct ids"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(person_name("  Anna ").unwrap(), "Anna");
        assert!(person_name("   ").is_err());
        assert!(person_name(&"é".repeat(101)).is_err());
        assert!(person_name(&"é".repeat(100)).is_ok());
        assert!(person_name("A\u{7}").is_err());
    }

    #[test]
    fn a_name_is_kept_cleared_or_set() {
        let parse = |json: &str| serde_json::from_str::<UpdatePersonRequest>(json).unwrap();
        let kept = parse(r#"{"hidden": true}"#);
        assert_eq!((kept.name, kept.clear_name), (None, false));
        let cleared = parse(r#"{"clear_name": true}"#);
        assert_eq!((cleared.name, cleared.clear_name), (None, true));
        assert_eq!(parse(r#"{"name": "Anna"}"#).name.as_deref(), Some("Anna"));
    }

    #[test]
    fn id_lists_are_bounded_and_distinct() {
        let id = Uuid::now_v7();
        assert!(check_ids("from", &[], 50).is_err());
        assert!(check_ids("from", &[id, id], 50).is_err());
        assert!(check_ids("from", &[id], 50).is_ok());
    }
}
