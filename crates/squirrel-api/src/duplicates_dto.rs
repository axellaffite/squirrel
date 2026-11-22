//! Request and response bodies of the duplicates review.

use serde::{Deserialize, Serialize};
use squirrel_db::duplicates::review::{Group, GroupState, MemberRecord};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::photos_dto::{AssetSummary, format_taken_at};

/// Whether a group waits for the user's decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateGroupState {
    #[default]
    Open,
    /// A photo was kept; the others are hidden.
    Resolved,
}

impl From<DuplicateGroupState> for GroupState {
    fn from(state: DuplicateGroupState) -> Self {
        match state {
            DuplicateGroupState::Open => Self::Open,
            DuplicateGroupState::Resolved => Self::Resolved,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DuplicatesQuery {
    /// `open` (default) or `resolved`.
    pub state: Option<DuplicateGroupState>,
    pub cursor: Option<String>,
    /// 1–100, default 20.
    pub limit: Option<u32>,
}

pub const DEFAULT_GROUP_PAGE: u32 = 20;
pub const MAX_GROUP_PAGE: u32 = 100;

pub fn group_page_size(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_GROUP_PAGE)
        .clamp(1, MAX_GROUP_PAGE)
}

/// One photo of a group.
#[derive(Debug, Serialize, ToSchema)]
pub struct DuplicateMemberDto {
    pub asset: AssetSummary,
    /// File size in bytes.
    pub size: i64,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    /// Hidden everywhere (kept another photo of the group).
    pub hidden: bool,
    /// Bits (of 64) between this photo's visual hash and the group's anchor.
    pub distance: u8,
}

impl From<&MemberRecord> for DuplicateMemberDto {
    fn from(member: &MemberRecord) -> Self {
        Self {
            asset: AssetSummary::from(&member.summary),
            size: member.summary.size,
            camera_make: member.camera_make.clone(),
            camera_model: member.camera_model.clone(),
            hidden: member.hidden,
            distance: u8::try_from(member.distance).unwrap_or(u8::MAX),
        }
    }
}

/// Near-identical photos of the caller, taken within a day of each other.
#[derive(Debug, Serialize, ToSchema)]
pub struct DuplicateGroupDto {
    pub id: Uuid,
    pub state: DuplicateGroupState,
    /// Capture time of the group's first photo, `2024-07-14T18:30:05`.
    pub taken_at: String,
    /// The photo kept, while it is in the group.
    pub kept_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub resolved_at: Option<OffsetDateTime>,
    /// Oldest capture first.
    pub members: Vec<DuplicateMemberDto>,
}

impl From<&Group> for DuplicateGroupDto {
    fn from(group: &Group) -> Self {
        let record = &group.group;
        Self {
            id: record.id,
            state: if record.resolved_at.is_some() {
                DuplicateGroupState::Resolved
            } else {
                DuplicateGroupState::Open
            },
            taken_at: format_taken_at(record.taken_at),
            kept_id: record.kept_node_id,
            resolved_at: record.resolved_at,
            members: group.members.iter().map(DuplicateMemberDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DuplicatesPage {
    /// Newest group first.
    pub items: Vec<DuplicateGroupDto>,
    /// Groups waiting for a decision (all pages).
    pub open_count: i64,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct KeepDuplicateRequest {
    /// The photo (file node id) to keep; the group's other photos are hidden.
    pub asset: Uuid,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RestoreDuplicatesRequest {
    /// The hidden photo to show again; every hidden photo of the group when absent.
    #[serde(default)]
    pub asset: Option<Uuid>,
}
