//! Request and response bodies of the notification inbox.

use serde::{Deserialize, Serialize};
use squirrel_db::notifications::{Kind, NotificationRecord};
use squirrel_db::shares::Subject;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::sharing_dto::{MemberDto, SharePermission};

/// Longest comment excerpt shown in a notification, in characters.
pub const EXCERPT_CHARS: usize = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    /// Something was shared with you.
    Share,
    /// Someone commented on something you can see.
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SubjectKind {
    Folder,
    File,
    Album,
}

/// What a notification is about.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationSubjectDto {
    pub kind: SubjectKind,
    /// The folder, file or album id.
    pub id: Uuid,
    pub name: String,
    /// Files only.
    pub mime: Option<String>,
    /// For a file: the folder holding it, when the caller may open that
    /// folder (`null` when the file is shared on its own or seen through an
    /// album).
    pub folder_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationDto {
    pub id: Uuid,
    pub kind: NotificationKind,
    /// Who shared or commented; `null` once their account was deleted.
    pub actor: Option<MemberDto>,
    pub subject: NotificationSubjectDto,
    /// Shares: the permission given (now).
    pub permission: Option<SharePermission>,
    /// Comments: the start of its current text.
    pub excerpt: Option<String>,
    /// Comments: its id, to scroll the thread to it.
    pub comment_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub read: bool,
}

/// The start of `body`, cut at a character boundary with an ellipsis.
pub fn excerpt(body: &str) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXCERPT_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(EXCERPT_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

impl NotificationDto {
    pub fn new(record: &NotificationRecord, folder_id: Option<Uuid>) -> Self {
        let actor = record
            .actor_username
            .clone()
            .zip(record.actor_display_name.clone())
            .map(|(username, display_name)| MemberDto {
                username,
                display_name,
            });
        let (kind, id) = match record.subject() {
            Subject::Album(id) => (SubjectKind::Album, id),
            Subject::Node(id) if record.subject_is_folder => (SubjectKind::Folder, id),
            Subject::Node(id) => (SubjectKind::File, id),
        };
        let permission = record.permission.as_deref().map(|p| {
            if p == "write" {
                SharePermission::Write
            } else {
                SharePermission::Read
            }
        });
        Self {
            id: record.id,
            kind: match record.kind() {
                Kind::Share => NotificationKind::Share,
                Kind::Comment => NotificationKind::Comment,
            },
            actor,
            subject: NotificationSubjectDto {
                kind,
                id,
                name: record.subject_name.clone(),
                mime: record.subject_mime.clone(),
                folder_id: folder_id.filter(|_| kind == SubjectKind::File),
            },
            permission,
            excerpt: record.comment_body.as_deref().map(excerpt),
            comment_id: record.comment_id,
            created_at: record.created_at,
            read: record.read_at.is_some(),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NotificationsQuery {
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Default 30, at most 100.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationPage {
    /// Newest first.
    pub items: Vec<NotificationDto>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountDto {
    pub unread: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkReadRequest {
    /// The notifications to mark read; omitted or `null`: all of them.
    pub ids: Option<Vec<Uuid>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_flatten_and_cut_long_comments() {
        assert_eq!(excerpt("  hello\n\n world "), "hello world");
        let long = "é".repeat(EXCERPT_CHARS + 10);
        let cut = excerpt(&long);
        assert_eq!(cut.chars().count(), EXCERPT_CHARS);
        assert!(cut.ends_with('…'));
        assert_eq!(
            excerpt(&"a".repeat(EXCERPT_CHARS)).chars().count(),
            EXCERPT_CHARS
        );
    }
}
