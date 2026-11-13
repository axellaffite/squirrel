use crate::error::ApiError;

/// Runs CPU-heavy work (Argon2) off the async executor.
pub async fn run_blocking<T, F>(work: F) -> Result<T, ApiError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| ApiError::Internal(err.into()))
}
