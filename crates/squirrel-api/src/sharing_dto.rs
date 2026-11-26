//! Request and response bodies of shares and comments.

use serde::{Deserialize, Serialize};
use squirrel_db::access::Access;
use squirrel_db::comments::CommentRecord;
use squirrel_db::shares::{Permission, ShareRecord, Subject};
use squirrel_db::users::UserRecord;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::albums_dto::AlbumDto;
use crate::error::ApiError;
use crate::files_dto::NodeDto;

/// What the caller may do with a folder, file or album.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AccessLevel {
    /// Everything, including sharing it.
    Owner,
    /// Folders: add, rename, move and trash inside. Albums: add and remove
    /// one's own photos.
    Write,
    /// See, download and comment.
    Read,
}

impl From<Access> for AccessLevel {
    fn from(access: Access) -> Self {
        match access {
            Access::Owner => Self::Owner,
            Access::Write => Self::Write,
            Access::Read => Self::Read,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SharePermission {
    Read,
    Write,
}

impl From<SharePermission> for Permission {
    fn from(permission: SharePermission) -> Self {
        match permission {
            SharePermission::Read => Self::Read,
            SharePermission::Write => Self::Write,
        }
    }
}

impl From<Permission> for SharePermission {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::Read => Self::Read,
            Permission::Write => Self::Write,
        }
    }
}

/// Another account, as shown next to a share or a comment.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemberDto {
    pub username: String,
    pub display_name: String,
}

impl From<&UserRecord> for MemberDto {
    fn from(user: &UserRecord) -> Self {
        Self {
            username: user.username.clone(),
            display_name: user.display_name.clone(),
        }
    }
}

/// Exactly one of `node_id` and `album_id`.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubjectRef {
    /// A folder or file.
    pub node_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
}

impl SubjectRef {
    pub fn subject(self) -> Result<Subject, ApiError> {
        match (self.node_id, self.album_id) {
            (Some(id), None) => Ok(Subject::Node(id)),
            (None, Some(id)) => Ok(Subject::Album(id)),
            _ => Err(ApiError::Validation(
                "give exactly one of node_id and album_id".into(),
            )),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShareRequest {
    #[serde(flatten)]
    pub subject: SubjectRef,
    /// The account to share with.
    pub username: String,
    pub permission: SharePermission,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ShareDto {
    pub id: Uuid,
    /// The person it is shared with.
    pub person: MemberDto,
    pub permission: SharePermission,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

impl From<&ShareRecord> for ShareDto {
    fn from(share: &ShareRecord) -> Self {
        Self {
            id: share.id,
            person: MemberDto {
                username: share.person_username.clone(),
                display_name: share.person_display_name.clone(),
            },
            permission: share.permission().into(),
            created_at: share.created_at,
        }
    }
}

/// Something shared with the caller: a folder, a file or an album.
#[derive(Debug, Serialize, ToSchema)]
pub struct SharedItemDto {
    /// The share (pass it to `DELETE /api/shares/{id}` to leave it).
    pub share_id: Uuid,
    /// Who shared it.
    pub owner: MemberDto,
    pub permission: SharePermission,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub shared_at: OffsetDateTime,
    /// Set for a folder or file.
    pub node: Option<NodeDto>,
    /// Set for an album.
    pub album: Option<AlbumDto>,
}

/// One of the caller's folders, files or albums, and the people it is shared with.
#[derive(Debug, Serialize, ToSchema)]
pub struct SharedByMeDto {
    /// Set for a folder or file.
    pub node: Option<NodeDto>,
    /// Set for an album.
    pub album: Option<AlbumDto>,
    /// The people it is shared with, in sharing order (pass a share's `id` to
    /// `DELETE /api/shares/{id}` to stop sharing with that person).
    pub people: Vec<ShareDto>,
    /// When it was last shared with someone.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub shared_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCommentRequest {
    #[serde(flatten)]
    pub subject: SubjectRef,
    /// 1–4000 characters.
    pub body: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditCommentRequest {
    /// 1–4000 characters.
    pub body: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CommentsQuery {
    pub node_id: Option<Uuid>,
    pub album_id: Option<Uuid>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Default 100, at most 200.
    pub limit: Option<u32>,
}

impl CommentsQuery {
    pub fn subject(&self) -> Result<Subject, ApiError> {
        SubjectRef {
            node_id: self.node_id,
            album_id: self.album_id,
        }
        .subject()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommentDto {
    pub id: Uuid,
    /// `null` once the author's account was deleted.
    pub author: Option<MemberDto>,
    pub body: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub edited_at: Option<OffsetDateTime>,
    /// Whether the caller wrote it (and so may edit it).
    pub mine: bool,
    /// Whether the caller may delete it (its author, or the subject's owner).
    pub can_delete: bool,
}

impl CommentDto {
    pub fn new(record: &CommentRecord, caller: Uuid) -> Self {
        let author = record
            .author_username
            .clone()
            .zip(record.author_display_name.clone())
            .map(|(username, display_name)| MemberDto {
                username,
                display_name,
            });
        let mine = record.author_id == Some(caller);
        Self {
            id: record.id,
            author,
            body: record.body.clone(),
            created_at: record.created_at,
            edited_at: record.edited_at,
            mine,
            can_delete: mine || record.owner_id == caller,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommentPage {
    /// Oldest first.
    pub items: Vec<CommentDto>,
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_subject_is_accepted() {
        let id = Uuid::now_v7();
        let both = SubjectRef {
            node_id: Some(id),
            album_id: Some(id),
        };
        let none = SubjectRef {
            node_id: None,
            album_id: None,
        };
        assert!(both.subject().is_err());
        assert!(none.subject().is_err());
        let node = SubjectRef {
            node_id: Some(id),
            album_id: None,
        };
        assert_eq!(node.subject().unwrap(), Subject::Node(id));
    }
}
