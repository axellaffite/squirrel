use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::queue::{self, Finished};
use squirrel_jobs::{NewJob, priority};

const LEASE: Duration = Duration::from_secs(300);
const ALL: &[&str] = &["a", "b", "thumbnails"];

async fn add(pool: &PgPool, job: NewJob) -> bool {
    let mut conn = pool.acquire().await.unwrap();
    squirrel_jobs::enqueue(&mut conn, &job).await.unwrap()
}

async fn claim_one(pool: &PgPool, lease: Duration, worker: &str) -> queue::ClaimedJob {
    queue::claim(pool, ALL, 1, lease, worker)
        .await
        .unwrap()
        .remove(0)
}

async fn count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn claims_by_priority_then_age_and_only_known_kinds(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({"n": 1}))).await;
    add(
        &pool,
        NewJob::new("b", json!({"n": 2})).priority(priority::INTERACTIVE),
    )
    .await;
    add(&pool, NewJob::new("unknown", json!({}))).await;
    let claimed = queue::claim(&pool, ALL, 10, LEASE, "test").await.unwrap();
    let kinds: Vec<_> = claimed.iter().map(|j| j.kind.as_str()).collect();
    assert_eq!(kinds, ["b", "a"]);
    assert!(claimed.iter().all(|j| j.attempts == 1));
    assert_eq!(claimed[0].payload, json!({"n": 2}));
    assert!(
        queue::claim(&pool, ALL, 10, LEASE, "test")
            .await
            .unwrap()
            .is_empty(),
        "nothing left"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delayed_jobs_wait_for_their_time(pool: PgPool) {
    add(
        &pool,
        NewJob::new("a", json!({})).delay(Duration::from_secs(3600)),
    )
    .await;
    assert!(
        queue::claim(&pool, ALL, 1, LEASE, "w")
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_claims_never_share_a_job(pool: PgPool) {
    for n in 0..20 {
        add(&pool, NewJob::new("a", json!({"n": n}))).await;
    }
    let (left, right) = tokio::join!(
        queue::claim(&pool, ALL, 15, LEASE, "w1"),
        queue::claim(&pool, ALL, 15, LEASE, "w2")
    );
    let mut ids: Vec<i64> = left
        .unwrap()
        .into_iter()
        .chain(right.unwrap())
        .map(|j| j.id)
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!((ids.len(), total), (20, 20));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_deletes_the_job_but_only_for_the_current_claim(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = claim_one(&pool, LEASE, "w").await;
    assert!(
        !queue::complete(&pool, job.id, uuid::Uuid::now_v7())
            .await
            .unwrap(),
        "stale claim"
    );
    assert!(queue::complete(&pool, job.id, job.claim_id).await.unwrap());
    assert_eq!(count(&pool).await, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn retries_back_off_then_fail_after_max_attempts(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    for attempt in 1..=5 {
        sqlx::query("UPDATE jobs SET run_at = now()")
            .execute(&pool)
            .await
            .unwrap();
        let job = claim_one(&pool, LEASE, "w").await;
        assert_eq!(job.attempts, attempt);
        let finished = queue::retry(&pool, &job, "boom", Duration::from_secs(60))
            .await
            .unwrap();
        let expected = if attempt < 5 {
            Finished::Requeued
        } else {
            Finished::Failed
        };
        assert_eq!(finished, expected, "attempt {attempt}");
    }
    let (status, error): (String, String) = sqlx::query_as("SELECT status, last_error FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((status.as_str(), error.as_str()), ("failed", "boom"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_retried_job_waits_for_its_backoff(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = claim_one(&pool, LEASE, "w").await;
    queue::retry(&pool, &job, "boom", Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        queue::claim(&pool, ALL, 1, LEASE, "w")
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn failing_ends_the_job_at_once_and_long_errors_are_truncated(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = claim_one(&pool, LEASE, "w").await;
    let long = "é".repeat(5000);
    assert_eq!(
        queue::fail(&pool, &job, &long).await.unwrap(),
        Finished::Failed
    );
    assert_eq!(
        queue::fail(&pool, &job, "again").await.unwrap(),
        Finished::LostLease
    );
    let (status, chars): (String, i32) =
        sqlx::query_as("SELECT status, char_length(last_error) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), chars), ("failed", 2000));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn reschedule_and_release_do_not_use_an_attempt(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = claim_one(&pool, LEASE, "w").await;
    assert!(
        queue::reschedule(&pool, &job, Duration::ZERO)
            .await
            .unwrap()
    );
    let job = claim_one(&pool, LEASE, "w").await;
    assert_eq!(job.attempts, 1);
    assert!(queue::release(&pool, &job).await.unwrap());
    let job = claim_one(&pool, LEASE, "w").await;
    assert_eq!(job.attempts, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_leases_are_reaped_and_fenced(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let first = claim_one(&pool, Duration::from_millis(1), "w1").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(queue::reap_expired(&pool).await.unwrap(), 1);
    let second = claim_one(&pool, LEASE, "w2").await;
    assert_eq!(second.attempts, 2);
    assert!(
        !queue::heartbeat(&pool, &first, LEASE).await.unwrap(),
        "lost lease"
    );
    assert!(
        !queue::complete(&pool, first.id, first.claim_id)
            .await
            .unwrap()
    );
    assert!(queue::heartbeat(&pool, &second, LEASE).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_expired_last_attempt_fails(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    sqlx::query("UPDATE jobs SET max_attempts = 1")
        .execute(&pool)
        .await
        .unwrap();
    claim_one(&pool, Duration::from_millis(1), "w").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(queue::reap_expired(&pool).await.unwrap(), 1);
    let status: String = sqlx::query_scalar("SELECT status FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn enqueue_is_transactional_and_deduplicated(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    let job = NewJob::new("thumbnails", json!({})).dedupe("thumbnails:x");
    assert!(squirrel_jobs::enqueue(&mut tx, &job).await.unwrap());
    assert!(!squirrel_jobs::enqueue(&mut tx, &job).await.unwrap());
    tx.rollback().await.unwrap();
    assert_eq!(count(&pool).await, 0, "rolled back with the transaction");
}

/// Whether a notification arrives on the jobs channel within a short wait.
async fn notified(listener: &mut sqlx::postgres::PgListener) -> bool {
    tokio::time::timeout(Duration::from_millis(500), listener.recv())
        .await
        .is_ok_and(|note| note.is_ok())
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn jobs_handed_back_for_now_wake_idle_workers(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    listener.listen(queue::CHANNEL).await.unwrap();

    let job = claim_one(&pool, LEASE, "w").await;
    assert!(queue::release(&pool, &job).await.unwrap());
    assert!(notified(&mut listener).await, "release notifies");

    let job = claim_one(&pool, LEASE, "w").await;
    assert!(
        queue::reschedule(&pool, &job, Duration::from_secs(60))
            .await
            .unwrap()
    );
    assert!(!notified(&mut listener).await, "a delayed job wakes nobody");

    // A stale claim changes nothing and wakes nobody.
    assert!(!queue::release(&pool, &job).await.unwrap());
    assert!(!notified(&mut listener).await);
}
