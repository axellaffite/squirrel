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
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub(crate) fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation() && db.constraint() == Some(constraint))
}
