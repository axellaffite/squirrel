//! Request and response bodies of friends.

use serde::{Deserialize, Serialize};
use squirrel_db::friends::FriendRecord;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::sharing_dto::MemberDto;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddFriendRequest {
    /// The account to add.
    pub username: String,
}

/// A friend: an account the caller picked for quick sharing.
#[derive(Debug, Serialize, ToSchema)]
pub struct FriendDto {
    pub username: String,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub added_at: OffsetDateTime,
}

impl From<&FriendRecord> for FriendDto {
    fn from(friend: &FriendRecord) -> Self {
        Self {
            username: friend.username.clone(),
            display_name: friend.display_name.clone(),
            added_at: friend.added_at,
        }
    }
}

impl From<&FriendRecord> for MemberDto {
    fn from(person: &FriendRecord) -> Self {
        Self {
            username: person.username.clone(),
            display_name: person.display_name.clone(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FriendsPage {
    /// By display name, at most 500.
    pub friends: Vec<FriendDto>,
    /// People the caller shared with or got a share from, not yet friends,
    /// most recent first, at most 20.
    pub suggestions: Vec<MemberDto>,
}
