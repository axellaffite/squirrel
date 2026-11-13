//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

mod error;
mod fsutil;
pub mod mime;
mod store;
mod uploads;

pub use error::{Result, StorageError};
pub use store::{Adopted, BlobStore};
pub use uploads::HashedUpload;
