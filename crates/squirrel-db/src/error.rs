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
