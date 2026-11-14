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
    /// The source file was already moved into (or dropped in favour of) the
    /// blob store when this happened: the operation cannot be retried from
    /// it. A blob the operation itself stored has been removed again.
    #[error("storing the blob failed after the file was moved: {0}")]
    Moved(io::Error),
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
