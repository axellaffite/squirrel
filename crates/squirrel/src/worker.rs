//! `squirrel worker` and `squirrel serve --worker`: the job queue's executor.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, ValueEnum};
use sqlx::PgPool;
use squirrel_jobs::{NewJob, Registry, Worker, WorkerConfig};
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;
use squirrel_worker::MediaContext;
use squirrel_worker::exec::{Executor, SubprocessConfig};
use tokio::sync::watch;

/// A claimed job is someone else's after this long without a heartbeat.
const LEASE: Duration = Duration::from_secs(5 * 60);
/// Catches delayed retries, expired leases and notifications missed while reconnecting.
const POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Running jobs get this long to finish after a stop; the rest are released.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const MIGRATION_POLL: Duration = Duration::from_secs(5);
const MIGRATION_LOG_EVERY: Duration = Duration::from_secs(60);
const MIGRATION_GIVE_UP: Duration = Duration::from_secs(10 * 60);
/// `jobs.claimed_by` holds at most this many characters.
const MAX_NAME_CHARS: usize = 200;
/// Does nothing: checks that a worker is alive (`squirrel admin jobs ping`).
pub const NOOP: &str = "noop";
const NOOP_TIMEOUT: Duration = Duration::from_secs(60);
/// Media sandbox limits: below the thumbnails handler's 180 s timeout and the
/// 300 s lease, so the sandbox always gives up first.
const SANDBOX_TIMEOUT: Duration = Duration::from_secs(150);
const SANDBOX_MEMORY_MB: u64 = 2048;
const SANDBOX_CPU_SECS: u64 = 120;

/// Where untrusted media is decoded.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaExecMode {
    /// A short-lived, resource-limited `squirrel media-exec` child per job.
    Subprocess,
    /// Inside the worker (tests; an escape hatch, without isolation).
    InProcess,
}

/// Worker options shared by `squirrel worker` and `squirrel serve --worker`.
#[derive(Args, Debug, Clone)]
pub struct WorkerOptions {
    /// Jobs run at once. Default: half the cores, at most 2, at least 1.
    #[arg(long, env = "SQUIRREL_WORKER_CONCURRENCY", value_parser = clap::value_parser!(u16).range(1..=64))]
    pub concurrency: Option<u16>,
    /// Where untrusted images are decoded: `subprocess` (isolated, default)
    /// or `in-process`.
    #[arg(long, env = "SQUIRREL_MEDIA_EXEC", value_enum, default_value_t = MediaExecMode::Subprocess)]
    pub media_exec: MediaExecMode,
}

/// `(min(cores, 4) / 2).max(1)`: the mini PC also serves HTTP.
pub fn default_concurrency(cores: usize) -> usize {
    (cores.min(4) / 2).max(1)
}

fn available_cores() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

/// `host:pid`, shown to admins as the holder of a running job.
pub fn worker_name() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    name_for(&host, std::process::id())
}

fn name_for(host: &str, pid: u32) -> String {
    let host = host.trim();
    let host = if host.is_empty() { "worker" } else { host };
    let suffix = format!(":{pid}");
    let keep = MAX_NAME_CHARS.saturating_sub(suffix.chars().count());
    host.chars().take(keep).chain(suffix.chars()).collect()
}

pub fn noop_job() -> NewJob {
    NewJob::new(NOOP, serde_json::json!({}))
}

fn executor(mode: MediaExecMode) -> anyhow::Result<Executor> {
    Ok(match mode {
        MediaExecMode::InProcess => Executor::InProcess,
        MediaExecMode::Subprocess => Executor::Subprocess(SubprocessConfig {
            program: std::env::current_exe().context("locating the squirrel executable")?,
            max_memory_mb: SANDBOX_MEMORY_MB,
            max_cpu_secs: SANDBOX_CPU_SECS,
            timeout: SANDBOX_TIMEOUT,
        }),
    })
}

/// The job kinds this binary executes.
fn registry(media: Arc<MediaContext>) -> Registry {
    let mut registry = Registry::new();
    registry.register(NOOP, NOOP_TIMEOUT, |_ctx| async { Ok(()) });
    squirrel_worker::register_all(&mut registry, media);
    registry
}

/// Queues the first backfill page (a no-op if one is already queued or running).
pub async fn enqueue_backfill(pool: &PgPool) -> anyhow::Result<bool> {
    let mut conn = pool.acquire().await?;
    Ok(squirrel_jobs::enqueue(&mut conn, &squirrel_worker::jobs::backfill(None)).await?)
}

fn config(options: &WorkerOptions) -> WorkerConfig {
    WorkerConfig {
        concurrency: options
            .concurrency
            .map_or_else(|| default_concurrency(available_cores()), usize::from),
        lease: LEASE,
        poll_interval: POLL_INTERVAL,
        shutdown_grace: SHUTDOWN_GRACE,
        name: worker_name(),
    }
}

/// The worker never migrates: it waits for the server (or `squirrel migrate`)
/// to bring the schema up to this binary's version, then gives up after 10 minutes.
pub async fn wait_for_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    let mut last_log: Option<tokio::time::Instant> = None;
    loop {
        let pending = squirrel_db::pending_migrations(pool)
            .await
            .context("checking applied migrations")?;
        if pending.is_empty() {
            return Ok(());
        }
        if started.elapsed() >= MIGRATION_GIVE_UP {
            anyhow::bail!("{}", crate::pending_message(&pending));
        }
        if last_log.is_none_or(|at| at.elapsed() >= MIGRATION_LOG_EVERY) {
            tracing::info!(pending = pending.len(), "waiting for migrations");
            last_log = Some(tokio::time::Instant::now());
        }
        tokio::time::sleep(MIGRATION_POLL).await;
    }
}

/// Runs the worker loop until `stop` turns true, then drains it. Queues a
/// backfill first, so files stored before an upgrade get processed.
pub async fn run(
    pool: PgPool,
    storage: BlobStore,
    options: WorkerOptions,
    stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let media = Arc::new(MediaContext {
        pool: pool.clone(),
        storage,
        executor: executor(options.media_exec)?,
        pdfium_dir: None,
        decode_limits: DecodeLimits::default(),
    });
    if let Err(err) = enqueue_backfill(&pool).await {
        tracing::warn!(error = %format!("{err:#}"), "could not queue the media backfill");
    }
    Worker::new(pool, registry(media), config(&options))
        .run(stop)
        .await
        .context("running the job worker")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_concurrency_is_half_the_cores_capped_at_two() {
        assert_eq!(default_concurrency(1), 1);
        assert_eq!(default_concurrency(2), 1);
        assert_eq!(default_concurrency(4), 2);
        assert_eq!(default_concurrency(16), 2);
    }

    #[test]
    fn worker_names_carry_host_and_pid() {
        let name = worker_name();
        assert!(name.contains(':') && name.chars().count() <= 200, "{name}");
        assert!(name.ends_with(&format!(":{}", std::process::id())));
    }

    #[test]
    fn long_host_names_are_cut() {
        let name = name_for(&"h".repeat(500), 42);
        assert_eq!(name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(name_for("  ", 42), "worker:42");
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn a_migrated_database_needs_no_wait(pool: PgPool) {
        tokio::time::timeout(Duration::from_secs(5), wait_for_migrations(&pool))
            .await
            .expect("no wait")
            .unwrap();
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn the_embedded_worker_runs_noop_jobs_and_stops(pool: PgPool) {
        let (stop_tx, stop_rx) = watch::channel(false);
        let options = WorkerOptions {
            concurrency: None,
            media_exec: MediaExecMode::InProcess,
        };
        let dir = tempfile::tempdir().unwrap();
        let storage = BlobStore::open(dir.path()).unwrap();
        let worker = tokio::spawn(run(pool.clone(), storage, options, stop_rx));
        let mut conn = pool.acquire().await.unwrap();
        squirrel_jobs::enqueue(&mut conn, &noop_job())
            .await
            .unwrap();
        let mut left = 1;
        for _ in 0..200 {
            left = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
                .fetch_one(&pool)
                .await
                .unwrap();
            if left == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(left, 0);
        stop_tx.send(true).unwrap();
        worker.await.unwrap().unwrap();
    }
}
