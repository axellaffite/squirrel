use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::{JobError, NewJob, Registry, Worker, WorkerConfig};
use tokio::sync::watch;
use tokio::task::JoinHandle;

fn config() -> WorkerConfig {
    WorkerConfig {
        concurrency: 2,
        lease: Duration::from_secs(30),
        poll_interval: Duration::from_millis(50),
        shutdown_grace: Duration::from_secs(2),
        name: "test-worker".into(),
    }
}

async fn add(pool: &PgPool, job: NewJob) {
    let mut conn = pool.acquire().await.unwrap();
    squirrel_jobs::enqueue(&mut conn, &job).await.unwrap();
}

async fn wait_until(pool: &PgPool, sql: &str, expected: i64) {
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar(sql).fetch_one(pool).await.unwrap();
        if n == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {sql} = {expected}");
}

struct Running {
    stop: watch::Sender<bool>,
    handle: JoinHandle<anyhow::Result<()>>,
}

impl Running {
    async fn stop(self) {
        self.stop.send(true).unwrap();
        self.handle.await.unwrap().unwrap();
    }
}

fn start(pool: &PgPool, registry: Registry, config: WorkerConfig) -> Running {
    let (stop, stop_rx) = watch::channel(false);
    let handle = tokio::spawn(Worker::new(pool.clone(), registry, config).run(stop_rx));
    Running { stop, handle }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn runs_jobs_and_deletes_them(pool: PgPool) {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    let counter = seen.clone();
    registry.register("count", Duration::from_secs(5), move |_ctx| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let worker = start(&pool, registry, config());
    for n in 0..5 {
        add(&pool, NewJob::new("count", json!({ "n": n }))).await;
    }
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    assert_eq!(seen.load(Ordering::SeqCst), 5);
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn notifications_wake_an_idle_worker(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("noop", Duration::from_secs(5), |_ctx| async { Ok(()) });
    let mut cfg = config();
    cfg.poll_interval = Duration::from_secs(3600);
    let worker = start(&pool, registry, cfg);
    // Let the worker reach its idle wait (claim found nothing, LISTEN active).
    tokio::time::sleep(Duration::from_millis(300)).await;
    add(&pool, NewJob::new("noop", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn errors_retry_panics_too_and_permanent_errors_fail(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("flaky", Duration::from_secs(5), |_ctx| async {
        Err(JobError::retry(anyhow::anyhow!("later")))
    });
    registry.register("broken", Duration::from_secs(5), |_ctx| async {
        Err(JobError::permanent(anyhow::anyhow!("never")))
    });
    registry.register("panics", Duration::from_secs(5), |_ctx| async {
        panic!("handler bug")
    });
    let worker = start(&pool, registry, config());
    add(&pool, NewJob::new("flaky", json!({}))).await;
    add(&pool, NewJob::new("broken", json!({}))).await;
    add(&pool, NewJob::new("panics", json!({}))).await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE kind = 'broken' AND status = 'failed' AND last_error = 'never'",
        1,
    )
    .await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE kind IN ('flaky', 'panics') AND status = 'queued' \
         AND attempts = 1 AND run_at > now()",
        2,
    )
    .await;
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn timeouts_count_as_retryable_failures(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("slow", Duration::from_millis(50), |_ctx| async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(())
    });
    let worker = start(&pool, registry, config());
    add(&pool, NewJob::new("slow", json!({}))).await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE status = 'queued' AND last_error LIKE 'timed out%'",
        1,
    )
    .await;
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shutdown_releases_unfinished_jobs(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("forever", Duration::from_secs(600), |_ctx| async {
        tokio::time::sleep(Duration::from_secs(600)).await;
        Ok(())
    });
    let mut cfg = config();
    cfg.shutdown_grace = Duration::from_millis(100);
    let worker = start(&pool, registry, cfg);
    add(&pool, NewJob::new("forever", json!({}))).await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE status = 'running'",
        1,
    )
    .await;
    worker.stop().await;
    let (status, attempts): (String, i32) = sqlx::query_as("SELECT status, attempts FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((status.as_str(), attempts), ("queued", 0));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn reschedule_keeps_the_attempt(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("later", Duration::from_secs(5), |_ctx| async {
        Err(JobError::Reschedule(Duration::from_secs(3600)))
    });
    let worker = start(&pool, registry, config());
    add(&pool, NewJob::new("later", json!({}))).await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE status = 'queued' AND attempts = 0 \
         AND run_at > now() + interval '50 minutes'",
        1,
    )
    .await;
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn handlers_see_their_attempt_and_payload(pool: PgPool) {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut registry = Registry::new();
    let log = seen.clone();
    registry.register("echo", Duration::from_secs(5), move |ctx| {
        let log = log.clone();
        async move {
            log.lock()
                .unwrap()
                .push((ctx.payload.clone(), ctx.attempt, ctx.is_last_attempt()));
            Ok(())
        }
    });
    let worker = start(&pool, registry, config());
    add(&pool, NewJob::new("echo", json!({"n": 7}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    worker.stop().await;
    assert_eq!(*seen.lock().unwrap(), [(json!({"n": 7}), 1, false)]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_timed_out_handler_is_stopped_not_left_running(pool: PgPool) {
    let finished = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    let flag = finished.clone();
    registry.register("slow", Duration::from_millis(50), move |_ctx| {
        let flag = flag.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            flag.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let worker = start(&pool, registry, config());
    add(&pool, NewJob::new("slow", json!({}))).await;
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE last_error LIKE 'timed out%'",
        1,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(finished.load(Ordering::SeqCst), 0);
    worker.stop().await;
}
