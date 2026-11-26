#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("an account already exists")]
    NotFirstUser,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error("not found")]
    NotFound,
    #[error("a file or folder with this name already exists here")]
    NameConflict,
    #[error("top-level folders cannot be renamed, moved or deleted")]
    ProtectedRoot,
    #[error("a folder cannot be moved into itself or one of its subfolders")]
    MoveIntoSelf,
    #[error("this would exceed your storage quota")]
    QuotaExceeded,
    #[error("this upload is already being completed")]
    UploadBusy,
    /// The owner already has the maximum number of open upload sessions.
    #[error("too many uploads in progress; finish or cancel some first")]
    TooManyUploads,
    /// Adding, removing or contributing photos to a dynamic album.
    #[error("a dynamic album's photos come from its rules: edit them instead")]
    DynamicAlbum,
    /// A deadlock or serialization failure that persisted after retries.
    #[error("the database is busy, try again in a moment")]
    Busy(#[source] sqlx::Error),
    /// A caller passed an argument outside the accepted range.
    #[error("{0}")]
    InvalidInput(&'static str),
    #[error(transparent)]
    Sqlx(sqlx::Error),
}

/// SQLSTATEs of transient conflicts that a retry can resolve.
const DEADLOCK_DETECTED: &str = "40P01";
const SERIALIZATION_FAILURE: &str = "40001";

impl From<sqlx::Error> for DbError {
    fn from(err: sqlx::Error) -> Self {
        if is_transient(&err) {
            DbError::Busy(err)
        } else {
            DbError::Sqlx(err)
        }
    }
}

fn is_transient(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db)
            if matches!(db.code().as_deref(), Some(DEADLOCK_DETECTED | SERIALIZATION_FAILURE))
    )
}

pub(crate) fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation() && db.constraint() == Some(constraint))
}

/// Maps a violation of the live-name unique index to `NameConflict`.
pub(crate) fn name_conflict_or(err: sqlx::Error) -> DbError {
    if is_unique_violation(&err, crate::nodes::LIVE_NAME_CONSTRAINT) {
        DbError::NameConflict
    } else {
        err.into()
    }
}
