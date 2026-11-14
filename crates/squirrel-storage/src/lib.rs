//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

mod error;
mod fsutil;
mod instance;
pub mod mime;
mod scan;
mod store;
mod uploads;

pub use error::{Result, StorageError};
pub use instance::{Binding, BindingError, INSTANCE_MARKER};
pub use store::{Adopted, BlobStore};
pub use uploads::HashedUpload;
