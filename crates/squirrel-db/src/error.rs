#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("an account already exists")]
    NotFirstUser,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub(crate) fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation() && db.constraint() == Some(constraint))
}
