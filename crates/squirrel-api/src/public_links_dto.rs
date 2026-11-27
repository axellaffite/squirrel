//! Request and response bodies of public links: the owner's side
//! (`/api/public-links`) and the visitor's (`/api/public/{token}`).

use serde::{Deserialize, Serialize};
use squirrel_db::public_links::PublicLinkRecord;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::files_dto::{NodeDto, PathSegmentDto};
use crate::sharing_dto::SubjectRef;

/// A public link as its owner sees it. Its URL is shown only once, when it is made.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicLinkDto {
    pub id: Uuid,
    pub has_password: bool,
    /// `null`: never expires.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
    /// Past its expiry: it no longer opens.
    pub expired: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

impl PublicLinkDto {
    pub fn new(link: &PublicLinkRecord, now: OffsetDateTime) -> Self {
        Self {
            id: link.id,
            has_password: link.has_password(),
            expires_at: link.expires_at,
            expired: link.expires_at.is_some_and(|at| at <= now),
            created_at: link.created_at,
        }
    }
}

/// The links of one folder, file or album.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicLinkList {
    /// Whether this server can make public links (its public URL is set).
    pub enabled: bool,
    /// Newest first, expired ones included.
    pub links: Vec<PublicLinkDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePublicLinkRequest {
    #[serde(flatten)]
    pub subject: SubjectRef,
    /// When it stops opening; must be in the future. Omitted or `null`: never.
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
    /// Asked of visitors before anything is shown (10–256 characters).
    /// Omitted or `null`: none.
    #[serde(default)]
    pub password: Option<String>,
}

/// A new link, with the URL to give out. The URL is never shown again.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedPublicLink {
    pub link: PublicLinkDto,
    pub url: String,
}

/// What a public link opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PublicLinkKind {
    Folder,
    File,
    Album,
}

/// An album opened by a public link.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicAlbumDto {
    pub id: Uuid,
    pub name: String,
    pub item_count: i64,
    /// A photo of it to show as its cover; `null` when empty.
    pub cover_id: Option<Uuid>,
}

/// What the visitor of a link sees first.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicLinkInfo {
    /// A password is needed (`POST /api/public/{token}/unlock`); nothing
    /// else is set until then.
    pub locked: bool,
    pub kind: Option<PublicLinkKind>,
    /// The display name of who shared it.
    pub shared_by: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
    /// The folder or file (`kind` folder or file).
    pub node: Option<NodeDto>,
    /// The album (`kind` album).
    pub album: Option<PublicAlbumDto>,
}

impl PublicLinkInfo {
    pub fn locked() -> Self {
        Self {
            locked: true,
            kind: None,
            shared_by: None,
            expires_at: None,
            node: None,
            album: None,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UnlockRequest {
    pub password: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PublicNodesQuery {
    /// A folder inside the link; omitted: the link's folder.
    pub parent_id: Option<Uuid>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// A folder inside a public link and a page of what it holds.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicFolderListing {
    pub folder: NodeDto,
    /// From the link's folder down to this folder's parent (empty at the top).
    pub path: Vec<PathSegmentDto>,
    pub items: Vec<NodeDto>,
    pub next_cursor: Option<String>,
}
