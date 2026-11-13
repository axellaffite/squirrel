//! Pure domain types for Squirrel. No I/O lives here.

mod display_name;
mod node_name;
mod password;
mod role;
mod username;

pub use display_name::{DisplayName, DisplayNameError};
pub use node_name::{NodeName, NodeNameError};
pub use password::{Password, PasswordError, normalize_password};
pub use role::{Role, UnknownRole};
pub use username::{Username, UsernameError};
