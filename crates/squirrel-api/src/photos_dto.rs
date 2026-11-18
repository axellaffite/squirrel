//! Response bodies of the photo timeline.

use serde::{Deserialize, Serialize};
use squirrel_db::explore::MlCounts;
use squirrel_db::media::MediaState;
use squirrel_db::ml_status::{MlState, MlStatus};
use squirrel_db::timeline::{AssetDetailRecord, AssetSummaryRecord, MonthCount, StatusCounts};
use time::PrimitiveDateTime;
use time::macros::format_description;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKindDto {
    Photo,
    Screenshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThumbnailStatus {
    /// Still being prepared.
    Pending,
    Ready,
    Failed,
    /// This format has no preview (in this build).
    Unsupported,
}

impl From<MediaState> for ThumbnailStatus {
    fn from(state: MediaState) -> Self {
        match state {
            MediaState::Pending => Self::Pending,
            MediaState::Ready => Self::Ready,
            MediaState::Failed => Self::Failed,
            MediaState::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ThumbnailState {
    pub state: ThumbnailStatus,
    /// Pass as `v` to `GET /api/files/{id}/thumbnail` once `ready`.
    pub version: i16,
}

/// One photo of the timeline.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetSummary {
    /// The file node's id (also used by `/api/files/{id}/…`).
    pub id: Uuid,
    pub name: String,
    pub mime: Option<String>,
    pub kind: AssetKindDto,
    /// Wall-clock capture time, no offset: `2024-07-14T18:30:05`.
    #[schema(example = "2024-07-14T18:30:05")]
    pub taken_at: String,
    /// `+02:00` when the camera recorded it.
    pub taken_offset: Option<String>,
    /// Displayed size, once known. The grid treats unknown as square.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub blurhash: Option<String>,
    pub thumbnail: ThumbnailState,
}

/// `2024-07-14T18:30:05` (no sub-seconds, no offset).
pub fn format_taken_at(at: PrimitiveDateTime) -> String {
    at.format(format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second]"
    ))
    .unwrap_or_default()
}

/// `+02:00`, `-09:30`, `+00:00`.
pub fn format_offset(minutes: i16) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let abs = minutes.unsigned_abs();
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

fn dimension(value: Option<i32>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

impl From<&AssetSummaryRecord> for AssetSummary {
    fn from(record: &AssetSummaryRecord) -> Self {
        Self {
            id: record.node_id,
            name: record.name.clone(),
            mime: record.mime.clone(),
            kind: if record.kind == "screenshot" {
                AssetKindDto::Screenshot
            } else {
                AssetKindDto::Photo
            },
            taken_at: format_taken_at(record.taken_at),
            taken_offset: record.taken_offset_min.map(format_offset),
            width: dimension(record.width),
            height: dimension(record.height),
            blurhash: record.blurhash.clone(),
            thumbnail: ThumbnailState {
                state: record.thumbs_state().into(),
                version: record.thumbs_version,
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TimelinePage {
    /// Newest capture first.
    pub items: Vec<AssetSummary>,
    /// Pass back as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MonthCountDto {
    /// `YYYY-MM`.
    pub month: String,
    pub count: i64,
}

impl From<MonthCount> for MonthCountDto {
    fn from(month: MonthCount) -> Self {
        Self {
            month: month.month,
            count: month.count,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TimelineQuery {
    /// Only this month (`YYYY-MM`, wall-clock capture time).
    pub month: Option<String>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Default 200, at most 500.
    pub limit: Option<u32>,
}

/// Where a photo was taken, named from the offline GeoNames dataset.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceDto {
    pub name: String,
    pub admin1: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LocationDto {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres above sea level.
    pub altitude: Option<f64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CameraDto {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// As recorded, e.g. `1/250`.
    pub exposure_time: Option<String>,
    pub f_number: Option<f32>,
    pub iso: Option<i32>,
    pub focal_length_mm: Option<f32>,
}

/// An album a photo belongs to.
#[derive(Debug, Serialize, ToSchema)]
pub struct AlbumRef {
    pub id: Uuid,
    pub name: String,
}

/// Everything the photo viewer shows about one photo.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetDetails {
    pub asset: AssetSummary,
    /// Bytes.
    pub size: i64,
    /// The folder holding the file.
    pub parent_id: Option<Uuid>,
    pub place: Option<PlaceDto>,
    pub location: Option<LocationDto>,
    pub camera: Option<CameraDto>,
    /// Albums of the caller holding this photo, by name.
    pub albums: Vec<AlbumRef>,
    /// Tags of this photo (slugs), best first.
    pub tags: Vec<String>,
}

impl AssetDetails {
    pub fn new(record: &AssetDetailRecord, albums: Vec<AlbumRef>, tags: Vec<String>) -> Self {
        let place = record.place_name.clone().map(|name| PlaceDto {
            name,
            admin1: record.place_admin1.clone(),
            country: record.place_country.clone(),
            country_code: record.place_country_code.clone(),
        });
        let location = record
            .latitude
            .zip(record.longitude)
            .map(|(latitude, longitude)| LocationDto {
                latitude,
                longitude,
                altitude: record.altitude,
            });
        let camera = CameraDto {
            make: record.camera_make.clone(),
            model: record.camera_model.clone(),
            lens: record.lens_model.clone(),
            exposure_time: record.exposure_time.clone(),
            f_number: record.f_number,
            iso: record.iso,
            focal_length_mm: record.focal_length_mm,
        };
        Self {
            asset: AssetSummary::from(&record.summary),
            size: record.summary.size,
            parent_id: record.parent_id,
            place,
            location,
            camera: (!camera.is_empty()).then_some(camera),
            albums,
            tags,
        }
    }
}

impl CameraDto {
    fn is_empty(&self) -> bool {
        self.make.is_none()
            && self.model.is_none()
            && self.lens.is_none()
            && self.exposure_time.is_none()
            && self.f_number.is_none()
            && self.iso.is_none()
            && self.focal_length_mm.is_none()
    }
}

/// The caller's files still being processed (drives "Preparing 12 photos…").
#[derive(Debug, Serialize, ToSchema)]
pub struct MediaStatus {
    /// Waiting for their first processing step.
    pub queued: i64,
    /// With a step in progress.
    pub processing: i64,
    /// With a step that failed.
    pub failed: i64,
    /// Photo tags, text search and faces on this server.
    pub ml: MlStatusDto,
}

impl MediaStatus {
    pub fn new(counts: StatusCounts, ml: MlStatusDto) -> Self {
        Self {
            queued: counts.queued,
            processing: counts.processing,
            failed: counts.failed,
            ml,
        }
    }
}

/// Which model groups this server found installed.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModelsInstalledDto {
    /// Photo tags and text search (the CLIP image model).
    pub clip: bool,
    pub faces: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MlStateDto {
    Ready,
    Unavailable,
}

/// Whether a worker can run the ML models, and the caller's photos still to
/// analyse.
#[derive(Debug, Serialize, ToSchema)]
pub struct MlStatusDto {
    pub state: MlStateDto,
    /// Why ML is unavailable (e.g. "models not installed"); null when ready.
    pub reason: Option<String>,
    /// The caller's photos waiting for tags and search.
    pub queued: i64,
    /// The caller's photos that could not be analysed.
    pub failed: i64,
    /// What the server found in its models directory.
    pub models_installed: ModelsInstalledDto,
}

impl MlStatusDto {
    pub fn new(
        status: MlStatus,
        counts: MlCounts,
        installed: crate::state::ModelsInstalled,
    ) -> Self {
        Self {
            state: match status.state {
                MlState::Ready => MlStateDto::Ready,
                MlState::Unavailable => MlStateDto::Unavailable,
            },
            reason: status.reason,
            queued: counts.queued,
            failed: counts.failed,
            models_installed: ModelsInstalledDto {
                clip: installed.clip,
                faces: installed.faces,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn times_and_offsets_are_formatted_for_display() {
        assert_eq!(
            format_taken_at(datetime!(2024-07-14 18:30:05.999)),
            "2024-07-14T18:30:05"
        );
        assert_eq!(format_offset(120), "+02:00");
        assert_eq!(format_offset(-570), "-09:30");
        assert_eq!(format_offset(0), "+00:00");
    }
}
