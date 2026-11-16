//! Pure domain types for Squirrel. No I/O lives here.

mod album_name;
mod content_hash;
mod display_name;
mod media_kind;
mod node_kind;
mod node_name;
mod password;
mod role;
mod upload_plan;
mod username;

pub use album_name::{AlbumName, AlbumNameError};
pub use content_hash::{ContentHash, ContentHashError};
pub use display_name::{DisplayName, DisplayNameError};
pub use media_kind::MediaKind;
pub use node_kind::{NodeKind, UnknownNodeKind};
pub use node_name::{NodeName, NodeNameError};
pub use password::{Password, PasswordError, normalize_password};
pub use role::{Role, UnknownRole};
pub use upload_plan::UploadPlan;
pub use username::{Username, UsernameError};
