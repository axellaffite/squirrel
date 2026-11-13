//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

mod error;
mod fsutil;
mod store;

pub use error::{Result, StorageError};
pub use store::{Adopted, BlobStore};
