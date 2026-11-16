//! Periodic maintenance: expired login sessions, abandoned uploads, old trash,
//! unreferenced blobs, blob files without a catalogue row and old failed jobs.

use std::future::Future;
use std::time::Duration;

use squirrel_api::{AppState, maintenance};
use tokio::time::{Instant, Interval, MissedTickBehavior};

const HOURLY: Duration = Duration::from_secs(60 * 60);
const STORAGE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// The orphan sweep deletes files, so it never runs right after a start: an
/// operator who started the server against the wrong database has this long
/// to notice and stop it (the data-directory marker and the sweep's own brake
/// come first).
const FIRST_SWEEP_DELAY: Duration = STORAGE_INTERVAL;
/// Failed queue jobs stay visible to administrators this long.
const FAILED_JOB_RETENTION: time::Duration = time::Duration::days(30);

/// Runs `job` now and then every `every`. Jobs log their own failures and are
/// retried on the next tick.
fn spawn_periodic<F, Fut>(every: Duration, job: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    spawn_periodic_after(Duration::ZERO, every, job);
}

/// Like [`spawn_periodic`], but the first run waits `first`.
fn spawn_periodic_after<F, Fut>(first: Duration, every: Duration, mut job: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut ticker = schedule(first, every);
        loop {
            ticker.tick().await;
            job().await;
        }
    });
}

fn schedule(first: Duration, every: Duration) -> Interval {
    let mut ticker = tokio::time::interval_at(Instant::now() + first, every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticker
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
    spawn_orphan_sweep(state);
}

/// Purges old trash, then collects garbage (blobs freed by this purge wait out
/// their grace period and go on a later run), then forgets failed jobs older
/// than [`FAILED_JOB_RETENTION`].
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
            let cutoff = time::OffsetDateTime::now_utc() - FAILED_JOB_RETENTION;
            match squirrel_jobs::admin::purge_failed_older_than(&state.pool, cutoff).await {
                Ok(0) => {}
                Ok(purged) => tracing::info!(purged, "purged old failed jobs"),
                Err(err) => tracing::error!(error = %err, "failed to purge old failed jobs"),
            }
        }
    });
}

/// Removes blob files without a catalogue row, first after [`FIRST_SWEEP_DELAY`].
fn spawn_orphan_sweep(state: &AppState) {
    let sweep_state = state.clone();
    spawn_periodic_after(FIRST_SWEEP_DELAY, STORAGE_INTERVAL, move || {
        let state = sweep_state.clone();
        async move {
            match maintenance::sweep_orphan_blobs(&state).await {
                Ok(sweep) if sweep.is_empty() => {}
                Ok(sweep) => tracing::info!(?sweep, "swept blob files without a catalogue row"),
                Err(err) => tracing::error!(error = %err, "orphan blob sweep failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHORT: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn periodic_jobs_run_at_start() {
        let mut ticker = schedule(Duration::ZERO, HOURLY);
        assert!(tokio::time::timeout(SHORT, ticker.tick()).await.is_ok());
    }

    #[tokio::test]
    async fn the_orphan_sweep_waits_before_its_first_run() {
        assert!(FIRST_SWEEP_DELAY >= HOURLY);
        let mut ticker = schedule(FIRST_SWEEP_DELAY, STORAGE_INTERVAL);
        assert!(tokio::time::timeout(SHORT, ticker.tick()).await.is_err());
    }
}
