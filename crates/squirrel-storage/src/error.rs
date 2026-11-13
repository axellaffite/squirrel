use std::io;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// ENOSPC or EDQUOT: the disk (or the filesystem quota) is full.
    #[error("not enough disk space")]
    NoSpace,
    #[error("blob not found")]
    NotFound,
    #[error("{0} must be on the same filesystem as the blobs directory")]
    CrossDevice(String),
    #[error(transparent)]
    Io(io::Error),
}

impl From<io::Error> for StorageError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Self::NoSpace,
            _ => Self::Io(err),
        }
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
