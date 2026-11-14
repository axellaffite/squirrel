//! What administrators see of the queue: counts, failures, manual retries,
//! and the retention of failed jobs.

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;

/// Partial unique index on `dedupe_key` for queued and running jobs.
const DEDUPE_INDEX: &str = "jobs_dedupe_key";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct KindSummary {
    pub kind: String,
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct FailedJob {
    pub id: i64,
    pub kind: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    /// Back in the queue with fresh attempts.
    Requeued,
    /// An equivalent job (same dedupe key) is already queued or running: the
    /// failed copy was removed instead.
    AlreadyQueued,
    /// No failed job has this id.
    NotFound,
}

/// Job counts per kind and status, kinds in alphabetical order.
pub async fn summary(pool: &PgPool) -> Result<Vec<KindSummary>, sqlx::Error> {
    sqlx::query_as::<_, KindSummary>(
        "SELECT kind, \
                count(*) FILTER (WHERE status = 'queued')  AS queued, \
                count(*) FILTER (WHERE status = 'running') AS running, \
                count(*) FILTER (WHERE status = 'failed')  AS failed \
           FROM jobs GROUP BY kind ORDER BY kind",
    )
    .fetch_all(pool)
    .await
}

/// The `limit` most recent failures, newest first.
pub async fn failed(pool: &PgPool, limit: i64) -> Result<Vec<FailedJob>, sqlx::Error> {
    sqlx::query_as::<_, FailedJob>(
        "SELECT id, kind, attempts, last_error, updated_at FROM jobs \
          WHERE status = 'failed' ORDER BY updated_at DESC, id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Puts a failed job back in the queue, due now, with all its attempts.
pub async fn retry_failed(pool: &PgPool, id: i64) -> Result<RetryOutcome, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET status = 'queued', attempts = 0, run_at = now(), updated_at = now() \
          WHERE id = $1 AND status = 'failed'",
    )
    .bind(id)
    .execute(pool)
    .await;
    match result {
        Ok(done) if done.rows_affected() == 1 => Ok(RetryOutcome::Requeued),
        Ok(_) => Ok(RetryOutcome::NotFound),
        Err(sqlx::Error::Database(db)) if db.constraint() == Some(DEDUPE_INDEX) => {
            sqlx::query("DELETE FROM jobs WHERE id = $1 AND status = 'failed'")
                .bind(id)
                .execute(pool)
                .await?;
            Ok(RetryOutcome::AlreadyQueued)
        }
        Err(err) => Err(err),
    }
}

/// Deletes failed jobs last touched before `cutoff`. Returns how many.
pub async fn purge_failed_older_than(
    pool: &PgPool,
    cutoff: OffsetDateTime,
) -> Result<u64, sqlx::Error> {
    let purged = sqlx::query("DELETE FROM jobs WHERE status = 'failed' AND updated_at < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(purged.rows_affected())
}
