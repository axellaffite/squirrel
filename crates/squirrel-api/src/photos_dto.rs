//! Response bodies of the photo timeline.

use serde::{Deserialize, Serialize};
use squirrel_db::media::MediaState;
use squirrel_db::timeline::{AssetSummaryRecord, MonthCount};
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
