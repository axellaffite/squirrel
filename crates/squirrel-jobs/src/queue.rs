//! SQL for the `jobs` table. Every state change after a claim matches the
//! claim's fencing token (`claim_id`), so a worker that lost its lease is a no-op.

use std::time::Duration;

use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::job::NewJob;

/// `LISTEN` channel notified (at commit) whenever jobs are inserted.
pub const CHANNEL: &str = "squirrel_jobs";
/// `jobs.last_error` holds at most this many characters.
const MAX_ERROR_CHARS: usize = 2000;

#[derive(Debug, Clone, FromRow)]
pub struct ClaimedJob {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    pub priority: i16,
    /// Including this claim (1 on the first run).
    pub attempts: i32,
    pub max_attempts: i32,
    /// Fencing token of this claim.
    pub claim_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finished {
    Requeued,
    Failed,
    /// The claim was stale: another worker owns the job now.
    LostLease,
}

fn secs(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

/// Fits `jobs.last_error`: bounded length, and no NUL (Postgres text refuses it).
fn truncate(message: &str) -> String {
    message
        .chars()
        .filter(|c| *c != '\0')
        .take(MAX_ERROR_CHARS)
        .collect()
}

/// Enqueues in the caller's connection or transaction: the job exists exactly
/// when the transaction commits. `false`: an equivalent job (same dedupe key)
/// is already queued or running.
pub async fn enqueue(conn: &mut PgConnection, job: &NewJob) -> Result<bool, sqlx::Error> {
    let delay = job.delay.map_or(0.0, secs);
    let result = sqlx::query(
        "INSERT INTO jobs (kind, payload, priority, dedupe_key, run_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) \
         ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') \
         DO NOTHING",
    )
    .bind(job.kind)
    .bind(&job.payload)
    .bind(job.priority)
    .bind(job.dedupe_key.as_deref())
    .bind(delay)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Claims up to `limit` due jobs of `kinds`, highest priority first.
pub async fn claim(
    pool: &PgPool,
    kinds: &[&str],
    limit: i64,
    lease: Duration,
    worker: &str,
) -> Result<Vec<ClaimedJob>, sqlx::Error> {
    let mut jobs = sqlx::query_as::<_, ClaimedJob>(
        "WITH picked AS ( \
             SELECT id FROM jobs \
              WHERE status = 'queued' AND run_at <= now() AND kind = ANY($1) \
              ORDER BY priority DESC, run_at, id \
              LIMIT $2 \
                FOR UPDATE SKIP LOCKED) \
         UPDATE jobs j \
            SET status = 'running', attempts = j.attempts + 1, \
                locked_until = now() + make_interval(secs => $3), \
                claim_id = gen_random_uuid(), claimed_by = $4, updated_at = now() \
           FROM picked WHERE j.id = picked.id \
         RETURNING j.id, j.kind, j.payload, j.priority, j.attempts, j.max_attempts, j.claim_id",
    )
    .bind(kinds)
    .bind(limit)
    .bind(secs(lease))
    .bind(worker)
    .fetch_all(pool)
    .await?;
    // RETURNING order is unspecified: restore the claim order (deterministic logs and tests).
    jobs.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    Ok(jobs)
}

/// Success: the job row is deleted. `false` if the claim was stale.
pub async fn complete(pool: &PgPool, id: i64, claim_id: Uuid) -> Result<bool, sqlx::Error> {
    let done =
        sqlx::query("DELETE FROM jobs WHERE id = $1 AND claim_id = $2 AND status = 'running'")
            .bind(id)
            .bind(claim_id)
            .execute(pool)
            .await?;
    Ok(done.rows_affected() == 1)
}

/// A failed attempt: back to the queue after `delay`, or `failed` if it was the last.
pub async fn retry(
    pool: &PgPool,
    job: &ClaimedJob,
    error: &str,
    delay: Duration,
) -> Result<Finished, sqlx::Error> {
    let status: Option<String> = sqlx::query_scalar(
        "UPDATE jobs SET status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END, \
                run_at = now() + make_interval(secs => $3), locked_until = NULL, claim_id = NULL, \
                claimed_by = NULL, last_error = $4, updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running' RETURNING status",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(delay))
    .bind(truncate(error))
    .fetch_optional(pool)
    .await?;
    Ok(match status.as_deref() {
        Some("failed") => Finished::Failed,
        Some(_) => Finished::Requeued,
        None => Finished::LostLease,
    })
}

/// Fails the job now, whatever attempts remain.
pub async fn fail(pool: &PgPool, job: &ClaimedJob, error: &str) -> Result<Finished, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE jobs SET status = 'failed', locked_until = NULL, claim_id = NULL, claimed_by = NULL, \
                last_error = $3, updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(truncate(error))
    .execute(pool)
    .await?;
    Ok(if updated.rows_affected() == 1 {
        Finished::Failed
    } else {
        Finished::LostLease
    })
}

/// Back to the queue after `delay`, giving the attempt back. `false`: lease lost.
/// A job due at once also wakes idle workers (the notify trigger only fires on
/// INSERT), in the same statement.
pub async fn reschedule(
    pool: &PgPool,
    job: &ClaimedJob,
    delay: Duration,
) -> Result<bool, sqlx::Error> {
    let updated: i64 = sqlx::query_scalar(
        "WITH updated AS ( \
             UPDATE jobs SET status = 'queued', attempts = attempts - 1, \
                    run_at = now() + make_interval(secs => $3), locked_until = NULL, \
                    claim_id = NULL, claimed_by = NULL, updated_at = now() \
              WHERE id = $1 AND claim_id = $2 AND status = 'running' RETURNING id) \
         SELECT count(*) FROM updated \
           LEFT JOIN LATERAL (SELECT pg_notify($4, '') WHERE $3 <= 0) woken ON true",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(delay))
    .bind(CHANNEL)
    .fetch_one(pool)
    .await?;
    Ok(updated == 1)
}

/// Shutdown: hand an unfinished job back at once, attempt not counted.
pub async fn release(pool: &PgPool, job: &ClaimedJob) -> Result<bool, sqlx::Error> {
    reschedule(pool, job, Duration::ZERO).await
}

/// Extends the lease of a job still being worked on. `false`: the lease was lost.
pub async fn heartbeat(
    pool: &PgPool,
    job: &ClaimedJob,
    lease: Duration,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE jobs SET locked_until = now() + make_interval(secs => $3), updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(lease))
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Running jobs whose lease expired (worker crashed or stalled) go back to the
/// queue, or to `failed` when that was their last attempt.
pub async fn reap_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let reaped = sqlx::query(
        "WITH expired AS ( \
             SELECT id FROM jobs WHERE status = 'running' AND locked_until < now() \
                FOR UPDATE SKIP LOCKED) \
         UPDATE jobs j \
            SET status = CASE WHEN j.attempts >= j.max_attempts THEN 'failed' ELSE 'queued' END, \
                locked_until = NULL, claim_id = NULL, claimed_by = NULL, \
                last_error = 'lease expired: the worker stopped or the job ran too long', \
                run_at = now(), updated_at = now() \
           FROM expired WHERE j.id = expired.id",
    )
    .execute(pool)
    .await?;
    Ok(reaped.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_cut_to_the_column_limit_and_lose_nul_bytes() {
        assert_eq!(truncate("a\0b"), "ab");
        assert_eq!(truncate(&"é".repeat(3000)).chars().count(), MAX_ERROR_CHARS);
    }
}
