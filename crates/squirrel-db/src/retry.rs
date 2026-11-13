use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::DbError;

const MAX_ATTEMPTS: u32 = 3;
const BACKOFF_STEP_MS: u64 = 10;

/// Runs `op` (one whole transaction) again when it fails with
/// [`DbError::Busy`] (deadlock or serialization failure), up to three attempts
/// with a small jittered backoff. The last `Busy` is returned as is.
pub(crate) async fn retry_transient<T, F, Fut>(mut op: F) -> Result<T, DbError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, DbError>>,
{
    let mut attempt = 1;
    loop {
        match op().await {
            Err(DbError::Busy(source)) if attempt < MAX_ATTEMPTS => {
                tracing::debug!(attempt, error = %source, "retrying after a transient conflict");
                tokio::time::sleep(backoff(attempt)).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// `attempt × 10 ms` plus up to 10 ms of jitter, so colliding transactions
/// do not retry in lockstep.
fn backoff(attempt: u32) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let jitter = u64::from(nanos) % BACKOFF_STEP_MS;
    Duration::from_millis(BACKOFF_STEP_MS * u64::from(attempt) + jitter)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn busy() -> DbError {
        DbError::Busy(sqlx::Error::PoolTimedOut)
    }

    #[tokio::test]
    async fn retries_busy_until_success() {
        let calls = Cell::new(0);
        let result = retry_transient(|| {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move { if n < 3 { Err(busy()) } else { Ok(n) } }
        })
        .await;
        assert_eq!(result.unwrap(), 3);
    }

    #[tokio::test]
    async fn gives_up_after_three_attempts() {
        let calls = Cell::new(0);
        let result: Result<(), _> = retry_transient(|| {
            calls.set(calls.get() + 1);
            async { Err(busy()) }
        })
        .await;
        assert!(matches!(result, Err(DbError::Busy(_))));
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test]
    async fn does_not_retry_other_errors() {
        let calls = Cell::new(0);
        let result: Result<(), _> = retry_transient(|| {
            calls.set(calls.get() + 1);
            async { Err(DbError::NotFound) }
        })
        .await;
        assert!(matches!(result, Err(DbError::NotFound)));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn backoff_grows_with_the_attempt() {
        assert!(backoff(1) < Duration::from_millis(20));
        assert!(backoff(2) >= Duration::from_millis(20));
    }
}
