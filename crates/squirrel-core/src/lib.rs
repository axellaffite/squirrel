//! Pure domain types for Squirrel. No I/O lives here.

mod content_hash;
mod display_name;
mod node_kind;
mod node_name;
mod password;
mod role;
mod upload_plan;
mod username;

pub use content_hash::{ContentHash, ContentHashError};
pub use display_name::{DisplayName, DisplayNameError};
pub use node_kind::{NodeKind, UnknownNodeKind};
pub use node_name::{NodeName, NodeNameError};
pub use password::{Password, PasswordError, normalize_password};
pub use role::{Role, UnknownRole};
pub use upload_plan::UploadPlan;
pub use username::{Username, UsernameError};
