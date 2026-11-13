use crate::error::ApiError;
use crate::state::AppState;

/// Runs Argon2 work off the async executor, holding one of the global hashing
/// permits for its whole duration. Answers `busy` (503) if no permit frees up
/// within `hash_wait`, instead of queueing unboundedly.
pub async fn run_hashing<T, F>(state: &AppState, work: F) -> Result<T, ApiError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let permit = tokio::time::timeout(
        state.config.hash_wait,
        state.hash_permits.clone().acquire_owned(),
    )
    .await
    .map_err(|_| {
        tracing::warn!("no password hashing slot available, answering busy");
        ApiError::Busy
    })?
    .map_err(|err| ApiError::Internal(err.into()))?;
    tokio::task::spawn_blocking(move || {
        // Released only when hashing finishes, even if the request was dropped.
        let _permit = permit;
        work()
    })
    .await
    .map_err(|err| ApiError::Internal(err.into()))
}
