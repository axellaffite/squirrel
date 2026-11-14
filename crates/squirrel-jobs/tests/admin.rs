use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::NewJob;
use squirrel_jobs::admin::{self, RetryOutcome};
use time::{Duration, OffsetDateTime};

async fn add(pool: &PgPool, job: NewJob) -> i64 {
    let mut conn = pool.acquire().await.unwrap();
    assert!(squirrel_jobs::enqueue(&mut conn, &job).await.unwrap());
    sqlx::query_scalar("SELECT max(id) FROM jobs")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Marks a job failed `age` ago with `error`, as the worker would.
async fn fail(pool: &PgPool, id: i64, error: &str, age: Duration) {
    sqlx::query(
        "UPDATE jobs SET status = 'failed', attempts = 5, last_error = $2, \
                updated_at = now() - make_interval(secs => $3) WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .bind(age.as_seconds_f64())
    .execute(pool)
    .await
    .unwrap();
}

async fn status_of(pool: &PgPool, id: i64) -> Option<(String, i32)> {
    sqlx::query_as("SELECT status, attempts FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn summary_counts_each_kind_by_status(pool: PgPool) {
    add(&pool, NewJob::new("thumbnails", json!({}))).await;
    add(&pool, NewJob::new("thumbnails", json!({}))).await;
    let failed = add(&pool, NewJob::new("geocode", json!({}))).await;
    fail(&pool, failed, "boom", Duration::ZERO).await;
    add(&pool, NewJob::new("a", json!({}))).await;
    squirrel_jobs::queue::claim(&pool, &["a"], 1, std::time::Duration::from_secs(60), "w")
        .await
        .unwrap();

    let summary = admin::summary(&pool).await.unwrap();
    let rows: Vec<_> = summary
        .iter()
        .map(|k| (k.kind.as_str(), k.queued, k.running, k.failed))
        .collect();
    assert_eq!(
        rows,
        [
            ("a", 0, 1, 0),
            ("geocode", 0, 0, 1),
            ("thumbnails", 2, 0, 0)
        ]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn failures_are_listed_newest_first(pool: PgPool) {
    let old = add(&pool, NewJob::new("a", json!({}))).await;
    let new = add(&pool, NewJob::new("b", json!({}))).await;
    add(&pool, NewJob::new("c", json!({}))).await;
    fail(&pool, old, "old error", Duration::hours(2)).await;
    fail(&pool, new, "new error", Duration::minutes(1)).await;

    let failures = admin::failed(&pool, 10).await.unwrap();
    let rows: Vec<_> = failures
        .iter()
        .map(|f| (f.id, f.kind.as_str(), f.attempts, f.last_error.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            (new, "b", 5, Some("new error")),
            (old, "a", 5, Some("old error"))
        ]
    );
    assert!(failures[0].updated_at > failures[1].updated_at);
    assert_eq!(admin::failed(&pool, 1).await.unwrap().len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn retrying_a_failed_job_queues_it_with_fresh_attempts(pool: PgPool) {
    let id = add(&pool, NewJob::new("a", json!({})).dedupe("a:1")).await;
    fail(&pool, id, "boom", Duration::ZERO).await;
    assert_eq!(
        admin::retry_failed(&pool, id).await.unwrap(),
        RetryOutcome::Requeued
    );
    assert_eq!(status_of(&pool, id).await, Some(("queued".into(), 0)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn retrying_a_job_already_queued_again_drops_the_failed_copy(pool: PgPool) {
    let id = add(&pool, NewJob::new("a", json!({})).dedupe("a:1")).await;
    fail(&pool, id, "boom", Duration::ZERO).await;
    let again = add(&pool, NewJob::new("a", json!({})).dedupe("a:1")).await;
    assert_eq!(
        admin::retry_failed(&pool, id).await.unwrap(),
        RetryOutcome::AlreadyQueued
    );
    assert_eq!(status_of(&pool, id).await, None);
    assert_eq!(status_of(&pool, again).await, Some(("queued".into(), 0)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_failed_jobs_can_be_retried(pool: PgPool) {
    let queued = add(&pool, NewJob::new("a", json!({}))).await;
    assert_eq!(
        admin::retry_failed(&pool, queued).await.unwrap(),
        RetryOutcome::NotFound
    );
    assert_eq!(
        admin::retry_failed(&pool, 999_999).await.unwrap(),
        RetryOutcome::NotFound
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purging_removes_only_old_failures(pool: PgPool) {
    let old = add(&pool, NewJob::new("a", json!({}))).await;
    let recent = add(&pool, NewJob::new("a", json!({}))).await;
    let queued = add(&pool, NewJob::new("a", json!({}))).await;
    sqlx::query("UPDATE jobs SET updated_at = now() - interval '40 days' WHERE id = $1")
        .bind(queued)
        .execute(&pool)
        .await
        .unwrap();
    fail(&pool, old, "x", Duration::days(31)).await;
    fail(&pool, recent, "x", Duration::days(29)).await;

    let cutoff = OffsetDateTime::now_utc() - Duration::days(30);
    assert_eq!(
        admin::purge_failed_older_than(&pool, cutoff).await.unwrap(),
        1
    );
    assert_eq!(status_of(&pool, old).await, None);
    assert!(status_of(&pool, recent).await.is_some());
    assert!(status_of(&pool, queued).await.is_some());
}
