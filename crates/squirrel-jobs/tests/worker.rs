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
        ml_kinds: Vec::new(),
        ml_concurrency: 1,
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
    start_worker(Worker::new(pool.clone(), registry, config).unwrap())
}

fn start_worker(worker: Worker) -> Running {
    let (stop, stop_rx) = watch::channel(false);
    let handle = tokio::spawn(worker.run(stop_rx));
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

fn noop_registry() -> Registry {
    let mut registry = Registry::new();
    registry.register("noop", Duration::from_secs(5), |_ctx| async { Ok(()) });
    registry
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_worker_that_cannot_listen_at_start_polls_instead(pool: PgPool) {
    // Two connections, both held by the test while the worker starts: its
    // LISTEN connection cannot be acquired.
    let tight = tight_pool(&pool).await;
    let held = (acquire(&tight).await, acquire(&tight).await);
    let mut cfg = config();
    cfg.poll_interval = Duration::from_millis(200);
    let worker = start(&tight, noop_registry(), cfg);
    // Past the LISTEN connection's acquire timeout.
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!worker.handle.is_finished(), "the worker must not give up");
    drop(held);

    add(&pool, NewJob::new("noop", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    worker.stop().await;
}

/// A pool of two connections that gives up acquiring after 500 ms. Lazy: no
/// connection is opened (and timed) before it is needed.
async fn tight_pool(pool: &PgPool) -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy_with((*pool.connect_options()).clone())
}

/// Opening connections can be slow while other tests run: retry.
async fn acquire(pool: &PgPool) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    for _ in 0..20 {
        if let Ok(conn) = pool.acquire().await {
            return conn;
        }
    }
    panic!("no connection");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_failing_listener_does_not_delay_stopping(pool: PgPool) {
    let own = tight_pool(&pool).await;
    let mut cfg = config();
    cfg.poll_interval = Duration::from_secs(5);
    let worker = start(&own, noop_registry(), cfg);
    let listening = "SELECT count(*) FROM pg_stat_activity \
          WHERE datname = current_database() AND query LIKE 'LISTEN%'";
    wait_until(&pool, listening, 1).await;
    // Closing the worker's pool makes its listener's `recv` fail (`PoolClosed`).
    // `close` waits for the listener's connection, so it is not awaited.
    let closing = tokio::spawn({
        let own = own.clone();
        async move { own.close().await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let started = std::time::Instant::now();
    worker.stop().await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stopping took {:?}",
        started.elapsed()
    );
    closing.await.unwrap();
}

/// A handler that flags when it starts and when it finishes, after `work`.
fn flagged(work: Duration, timeout: Duration) -> (Registry, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let started = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let (s, f) = (started.clone(), finished.clone());
    let mut registry = Registry::new();
    registry.register("work", timeout, move |_ctx| {
        let (s, f) = (s.clone(), f.clone());
        async move {
            s.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(work).await;
            f.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    (registry, started, finished)
}

async fn wait_for(counter: &AtomicUsize, expected: usize) {
    for _ in 0..200 {
        if counter.load(Ordering::SeqCst) == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {expected}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_handler_whose_claim_is_taken_over_is_stopped(pool: PgPool) {
    let (registry, started, finished) = flagged(Duration::from_secs(1), Duration::from_secs(5));
    let mut cfg = config();
    cfg.lease = Duration::from_millis(300);
    let worker = start(&pool, registry, cfg);
    add(&pool, NewJob::new("work", json!({}))).await;
    wait_for(&started, 1).await;
    // Another claimant now owns the job (its own token, its own lease).
    sqlx::query(
        "UPDATE jobs SET claim_id = gen_random_uuid(), locked_until = now() + interval '1 hour'",
    )
    .execute(&pool)
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;

    assert_eq!(
        finished.load(Ordering::SeqCst),
        0,
        "the handler was stopped"
    );
    let row: (String, i32, Option<String>) =
        sqlx::query_as("SELECT status, attempts, last_error FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, ("running".to_owned(), 1, None), "nothing recorded");
    worker.stop().await;
    let after: (String, i32) = sqlx::query_as("SELECT status, attempts FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, ("running".to_owned(), 1), "not released either");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn heartbeats_keep_a_job_longer_than_its_lease(pool: PgPool) {
    let (registry, started, finished) = flagged(Duration::from_secs(1), Duration::from_secs(5));
    let mut cfg = config();
    cfg.lease = Duration::from_millis(300);
    let worker = start(&pool, registry, cfg);
    add(&pool, NewJob::new("work", json!({}))).await;
    wait_for(&started, 1).await;
    // While it runs (the reaper runs every 50 ms poll), the lease never lapses.
    for _ in 0..200 {
        let expired: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE last_error LIKE 'lease expired%' OR attempts > 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(expired, 0);
        if finished.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    assert_eq!(
        (
            started.load(Ordering::SeqCst),
            finished.load(Ordering::SeqCst)
        ),
        (1, 1),
        "completed exactly once"
    );
    worker.stop().await;
}

type Clock = Arc<std::sync::Mutex<Option<std::time::Instant>>>;

/// Keeps the first time only: the reaped job runs (and is abandoned) again.
fn mark(clock: &Clock) {
    clock
        .lock()
        .unwrap()
        .get_or_insert_with(std::time::Instant::now);
}

/// Records when the handler's future is dropped before finishing.
struct DropClock(Clock);

impl Drop for DropClock {
    fn drop(&mut self) {
        mark(&self.0);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_handler_is_abandoned_before_a_lease_heartbeats_cannot_renew(pool: PgPool) {
    let started_at: Clock = Arc::default();
    let dropped_at: Clock = Arc::default();
    let (start_clock, drop_clock) = (started_at.clone(), dropped_at.clone());
    let mut registry = Registry::new();
    registry.register("work", Duration::from_secs(5), move |_ctx| {
        let (start_clock, drop_clock) = (start_clock.clone(), drop_clock.clone());
        async move {
            mark(&start_clock);
            let guard = DropClock(drop_clock);
            tokio::time::sleep(Duration::from_secs(2)).await;
            std::mem::forget(guard); // finished: not an abort
            Ok(())
        }
    });
    let mut cfg = config();
    cfg.lease = Duration::from_millis(300);
    // Every heartbeat fails, as if the database had become unreachable.
    let worker = Worker::new(pool.clone(), registry, cfg)
        .unwrap()
        .with_heartbeat_fault(|| true);
    let worker = start_worker(worker);
    add(&pool, NewJob::new("work", json!({}))).await;
    // The reaper (not the worker) puts the job back once the lease has lapsed.
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE last_error LIKE 'lease expired%'",
        1,
    )
    .await;
    let started = started_at.lock().unwrap().expect("the handler started");
    let dropped = dropped_at.lock().unwrap().expect("the handler was aborted");
    assert!(
        dropped.duration_since(started) < Duration::from_millis(290),
        "aborted {:?} after starting, past its 300 ms lease",
        dropped.duration_since(started)
    );
    worker.stop().await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn configurations_heartbeats_cannot_serve_are_refused(pool: PgPool) {
    let mut short = config();
    short.lease = Duration::from_millis(299);
    assert!(Worker::new(pool.clone(), noop_registry(), short).is_err());
    let mut zero = Registry::new();
    zero.register("instant", Duration::ZERO, |_ctx| async { Ok(()) });
    assert!(Worker::new(pool.clone(), zero, config()).is_err());
    let mut ok = config();
    ok.lease = Duration::from_millis(300);
    assert!(Worker::new(pool, noop_registry(), ok).is_ok());
}

/// ML jobs never take more than `ml_concurrency` slots: a fresh upload's
/// thumbnails start at once even with a backlog of inference jobs.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ml_jobs_leave_a_slot_for_other_work(pool: PgPool) {
    #[derive(Default)]
    struct Seen {
        ml_running: AtomicUsize,
        ml_max: AtomicUsize,
        ml_done: AtomicUsize,
        /// (ML jobs running, ML jobs done) when the thumbnails job started.
        at_thumbnails: std::sync::Mutex<Option<(usize, usize)>>,
    }
    let seen = Arc::new(Seen::default());
    let mut registry = Registry::new();
    let ml = seen.clone();
    registry.register("infer", Duration::from_secs(30), move |_ctx| {
        let seen = ml.clone();
        async move {
            let now = seen.ml_running.fetch_add(1, Ordering::SeqCst) + 1;
            seen.ml_max.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(400)).await;
            seen.ml_running.fetch_sub(1, Ordering::SeqCst);
            seen.ml_done.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let thumbs = seen.clone();
    registry.register("thumbnails", Duration::from_secs(30), move |_ctx| {
        let seen = thumbs.clone();
        async move {
            let state = (
                seen.ml_running.load(Ordering::SeqCst),
                seen.ml_done.load(Ordering::SeqCst),
            );
            *seen.at_thumbnails.lock().unwrap() = Some(state);
            Ok(())
        }
    });
    let mut cfg = config();
    cfg.ml_kinds = vec!["infer"];
    let worker = start(&pool, registry, cfg);
    for n in 0..5 {
        add(&pool, NewJob::new("infer", json!({ "n": n }))).await;
    }
    wait_until(
        &pool,
        "SELECT count(*) FROM jobs WHERE kind = 'infer' AND status = 'running'",
        1,
    )
    .await;
    add(&pool, NewJob::new("thumbnails", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    worker.stop().await;
    assert_eq!(seen.ml_max.load(Ordering::SeqCst), 1);
    assert_eq!(seen.ml_done.load(Ordering::SeqCst), 5);
    let (running, done) = seen.at_thumbnails.lock().unwrap().expect("thumbnails ran");
    assert_eq!(running, 1, "an ML job was running");
    assert_eq!(
        done, 0,
        "the thumbnails job did not wait for an ML job to end"
    );
}
