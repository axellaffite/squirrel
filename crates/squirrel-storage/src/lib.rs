//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

pub mod derived;
mod error;
mod fsutil;
mod instance;
pub mod mime;
mod scan;
mod store;
mod uploads;

pub use derived::{DerivedStore, Scratch, Variant};
pub use error::{Result, StorageError};
pub use instance::{Binding, BindingError, INSTANCE_MARKER};
pub use store::{Adopted, BlobStore};
pub use uploads::HashedUpload;
