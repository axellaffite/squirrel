//! Background jobs: expired login sessions, abandoned uploads, old trash and
//! unreferenced blobs.

use std::future::Future;
use std::time::Duration;

use squirrel_api::{AppState, maintenance};
use tokio::time::MissedTickBehavior;

const HOURLY: Duration = Duration::from_secs(60 * 60);
const STORAGE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Runs `job` now and then every `every`. Jobs log their own failures and are
/// retried on the next tick.
fn spawn_periodic<F, Fut>(every: Duration, mut job: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(every);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            job().await;
        }
    });
}

pub fn spawn_all(state: &AppState) {
    let pool = state.pool.clone();
    spawn_periodic(HOURLY, move || {
        let pool = pool.clone();
        async move {
            match squirrel_db::sessions::delete_expired(&pool).await {
                Ok(0) => {}
                Ok(deleted) => tracing::info!(deleted, "removed expired sessions"),
                Err(err) => tracing::error!(error = %err, "failed to remove expired sessions"),
            }
        }
    });
    let uploads_state = state.clone();
    spawn_periodic(HOURLY, move || {
        let state = uploads_state.clone();
        async move {
            match maintenance::purge_expired_uploads(&state).await {
                Ok(cleanup) if cleanup.is_empty() => {}
                Ok(cleanup) => tracing::info!(?cleanup, "removed expired uploads"),
                Err(err) => tracing::error!(error = %err, "failed to remove expired uploads"),
            }
        }
    });
    spawn_storage_job(state);
}

/// Purges old trash, then collects garbage: blobs freed by this purge wait out
/// their grace period and go on a later run.
fn spawn_storage_job(state: &AppState) {
    let storage_state = state.clone();
    spawn_periodic(STORAGE_INTERVAL, move || {
        let state = storage_state.clone();
        async move {
            match maintenance::purge_old_trash(&state).await {
                Ok(0) => {}
                Ok(purged) => tracing::info!(purged, "purged old trash entries"),
                Err(err) => tracing::error!(error = %err, "failed to purge the trash"),
            }
            match maintenance::collect_garbage(&state).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "deleted unreferenced blobs"),
                Err(err) => tracing::error!(error = %err, "blob garbage collection failed"),
            }
        }
    });
}
