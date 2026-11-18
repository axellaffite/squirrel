//! `squirrel worker` and `squirrel serve --worker`: the job queue's executor.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, ValueEnum};
use sqlx::PgPool;
use squirrel_jobs::{NewJob, Registry, Worker, WorkerConfig};
use squirrel_media::geo_data::GeoSource;
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;
use squirrel_worker::MediaContext;
use squirrel_worker::exec::{Executor, SubprocessConfig};
use squirrel_worker::geocode::GeoCache;

use crate::ml::{MlExecMode, MlSettings};
use crate::ml_start::MlWanted;
use tokio::sync::watch;

/// A claimed job is someone else's after this long without a heartbeat.
const LEASE: Duration = Duration::from_secs(5 * 60);
/// Catches delayed retries, expired leases and notifications missed while reconnecting.
const POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Running jobs get this long to finish after a stop; the rest are released
/// (back to the queue, no attempt used). Below Docker's default 10 s stop
/// timeout, so releasing happens before SIGKILL instead of leaving leases to
/// expire 5 minutes later.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);
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
    /// Reverse-geocoding dataset to use instead of the pinned snapshot
    /// installed in `<data_dir>/geo/` (trusted as is: no checksum).
    #[arg(long, env = "SQUIRREL_GEODATA_FILE")]
    pub geodata_file: Option<PathBuf>,
    /// Directory holding the pdfium library (`scripts/fetch-pdfium.sh`):
    /// PDF thumbnails and text. Without it, PDFs are left unsupported until
    /// it is set (the next start queues them again).
    #[arg(long, env = "SQUIRREL_PDFIUM_DIR")]
    pub pdfium_dir: Option<PathBuf>,
    /// Run only these job kinds (comma-separated), e.g. `clip_embed,faces,cluster_faces`
    /// on a separate ML machine.
    #[arg(
        long,
        env = "SQUIRREL_WORKER_KINDS",
        value_delimiter = ',',
        conflicts_with = "exclude_kinds"
    )]
    pub kinds: Vec<String>,
    /// Run every job kind but these (comma-separated).
    #[arg(long, env = "SQUIRREL_WORKER_EXCLUDE_KINDS", value_delimiter = ',')]
    pub exclude_kinds: Vec<String>,
    /// Pinned model files (`scripts/fetch-models.sh`). Default: `<data_dir>/models`.
    /// Verified against their SHA-256 at start, wherever they come from.
    #[arg(long, env = "SQUIRREL_MODELS_DIR")]
    pub models_dir: Option<PathBuf>,
    /// The pinned ONNX Runtime library (`scripts/fetch-onnxruntime.sh`),
    /// needed once models are installed.
    #[arg(long, env = "SQUIRREL_ORT_DYLIB")]
    pub ort_dylib: Option<PathBuf>,
    /// Threads for one inference. Default: half the cores.
    #[arg(long, env = "SQUIRREL_ML_THREADS", value_parser = clap::value_parser!(u16).range(1..=64))]
    pub ml_threads: Option<u16>,
    /// Where model inference runs: `subprocess` (isolated, default) or `in-process`.
    #[arg(long, env = "SQUIRREL_ML_EXEC", value_enum, default_value_t = MlExecMode::Subprocess)]
    pub ml_exec: MlExecMode,
    /// Model inference jobs run at once, so the other slots stay free for
    /// fresh uploads. Only 1 for now: the worker has one `ml-exec` child,
    /// which serves one request at a time (each already uses several cores),
    /// so a second ML job would only wait for it.
    #[arg(long, env = "SQUIRREL_ML_CONCURRENCY", default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..=1))]
    pub ml_concurrency: u16,
}

/// `(min(cores, 4) / 2).max(1)`: the mini PC also serves HTTP.
pub fn default_concurrency(cores: usize) -> usize {
    (cores.min(4) / 2).max(1)
}

fn available_cores() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

/// This machine's name (the ML status is reported per host).
fn worker_host() -> String {
    let name = worker_name();
    name.rsplit_once(':')
        .map_or(name.clone(), |(host, _)| host.to_owned())
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

/// The pdfium directory as an absolute path, refused unless it holds the
/// library: a typo should stop the worker, not silently skip every PDF.
pub fn pdfium_dir(configured: Option<&Path>) -> anyhow::Result<Option<PathBuf>> {
    let Some(dir) = configured else {
        return Ok(None);
    };
    let dir = std::fs::canonicalize(dir)
        .with_context(|| format!("SQUIRREL_PDFIUM_DIR {}", dir.display()))?;
    let library = squirrel_media::pdf::library_path(&dir);
    if !library.is_file() {
        anyhow::bail!(
            "SQUIRREL_PDFIUM_DIR {} has no {} (run scripts/fetch-pdfium.sh)",
            dir.display(),
            library.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    Ok(Some(dir))
}

/// Every job kind this binary knows, registered or not.
fn known_kinds() -> Vec<&'static str> {
    let mut known: Vec<_> = std::iter::once(NOOP)
        .chain(squirrel_worker::jobs::KNOWN_KINDS)
        .collect();
    known.sort_unstable();
    known
}

/// The job kinds this binary executes, narrowed by `--kinds` / `--exclude-kinds`.
fn registry(media: Arc<MediaContext>, options: &WorkerOptions) -> anyhow::Result<Registry> {
    let mut registry = Registry::new();
    registry.register(NOOP, NOOP_TIMEOUT, |_ctx| async { Ok(()) });
    squirrel_worker::register_all(&mut registry, media);
    select_kinds(&mut registry, &options.kinds, &options.exclude_kinds)?;
    Ok(registry)
}

/// Keeps the kinds of `only` (when given), minus those of `except`. A name
/// this binary does not know is an error (a typo would silently stop a kind).
fn select_kinds(registry: &mut Registry, only: &[String], except: &[String]) -> anyhow::Result<()> {
    if !only.is_empty() && !except.is_empty() {
        anyhow::bail!("--kinds and --exclude-kinds cannot be used together");
    }
    let known = known_kinds();
    let unknown: Vec<&str> = only
        .iter()
        .chain(except)
        .map(String::as_str)
        .filter(|name| !known.contains(name))
        .collect();
    if !unknown.is_empty() {
        anyhow::bail!(
            "unknown job kinds: {} (known kinds: {})",
            unknown.join(", "),
            known.join(", ")
        );
    }
    let registered = registry.kinds();
    for name in only {
        if !registered.contains(&name.as_str()) {
            tracing::warn!(kind = %name, "--kinds names a kind this worker cannot run (missing models?)");
        }
    }
    if !only.is_empty() {
        registry.retain(|kind| only.iter().any(|name| name == kind));
    }
    registry.retain(|kind| !except.iter().any(|name| name == kind));
    Ok(())
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
        ml_kinds: squirrel_worker::jobs::ML_KINDS.to_vec(),
        ml_concurrency: usize::from(options.ml_concurrency),
    }
}

/// Runs the embedded worker on its own task and logs its failure as soon as
/// it happens: the server keeps serving, so nothing else would report it.
pub fn spawn_logged<F>(worker: F) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(err) = worker.await {
            tracing::error!(error = %format!("{err:#}"), "the job worker stopped with an error");
        }
    })
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
    data_dir: PathBuf,
    options: WorkerOptions,
    stop: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let geo = GeoSource::resolve(options.geodata_file.clone(), &data_dir);
    let pdfium_dir = pdfium_dir(options.pdfium_dir.as_deref())?;
    if let Some(dir) = &pdfium_dir {
        tracing::info!(dir = %dir.display(), "PDF thumbnails and text use pdfium");
    }
    let media = Arc::new(MediaContext {
        pool: pool.clone(),
        storage,
        executor: executor(options.media_exec)?,
        pdfium_dir,
        decode_limits: DecodeLimits::default(),
        geo: GeoCache::new(geo),
        ml: start_ml(&options, &data_dir, &pool).await?,
    });
    if let Err(err) = enqueue_backfill(&pool).await {
        tracing::warn!(error = %format!("{err:#}"), "could not queue the media backfill");
    }
    Worker::new(pool, registry(media, &options)?, config(&options))
        .context("configuring the job worker")?
        .run(stop)
        .await
        .context("running the job worker")
}

/// Starts machine learning when this worker's kinds include ML kinds (see
/// [`crate::ml_start::start`]).
async fn start_ml(
    options: &WorkerOptions,
    data_dir: &Path,
    pool: &PgPool,
) -> anyhow::Result<Option<Arc<squirrel_worker::context::MlContext>>> {
    let settings = MlSettings {
        models_dir: options
            .models_dir
            .clone()
            .unwrap_or_else(|| data_dir.join("models")),
        ort_dylib: options.ort_dylib.clone(),
        threads: options
            .ml_threads
            .map_or_else(squirrel_ml::runtime::default_threads, usize::from),
        mode: options.ml_exec,
    };
    crate::ml_start::start(settings, pool, &worker_host(), ml_wanted(options)).await
}

/// Every kind that needs models, whatever is installed.
fn all_ml_kinds() -> Vec<&'static str> {
    squirrel_worker::context::ml_kinds(true, true)
}

fn ml_wanted(options: &WorkerOptions) -> MlWanted {
    let ml = all_ml_kinds();
    if !options.kinds.is_empty() {
        return if options.kinds.iter().any(|k| ml.contains(&k.as_str())) {
            MlWanted::Required
        } else {
            MlWanted::No
        };
    }
    if ml
        .iter()
        .all(|kind| options.exclude_kinds.iter().any(|e| e == kind))
    {
        return MlWanted::No;
    }
    MlWanted::IfAvailable
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

    /// Collects what a subscriber writes, to check what was logged.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_failing_embedded_worker_is_logged_when_it_ends() {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // Current-thread test runtime: the spawned task sees this subscriber.
        spawn_logged(async { Err(anyhow::anyhow!("listener gone")) })
            .await
            .unwrap();
        let logged = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(logged.contains("ERROR"), "{logged}");
        assert!(logged.contains("listener gone"), "{logged}");
    }

    #[test]
    fn a_pdfium_dir_without_the_library_is_refused() {
        assert!(pdfium_dir(None).unwrap().is_none());
        let dir = tempfile::tempdir().unwrap();
        let err = pdfium_dir(Some(dir.path())).unwrap_err();
        assert!(format!("{err:#}").contains("fetch-pdfium"), "{err:#}");
        assert!(pdfium_dir(Some(&dir.path().join("missing"))).is_err());
        std::fs::write(squirrel_media::pdf::library_path(dir.path()), b"").unwrap();
        let found = pdfium_dir(Some(dir.path())).unwrap().unwrap();
        assert!(found.is_absolute());
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn sample_registry() -> Registry {
        let mut registry = Registry::new();
        for kind in [NOOP, "thumbnails", "geocode", "faces"] {
            registry.register(kind, NOOP_TIMEOUT, |_ctx| async { Ok(()) });
        }
        registry
    }

    fn options(kinds: &[&str], exclude: &[&str]) -> WorkerOptions {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Cli {
            #[command(flatten)]
            options: WorkerOptions,
        }
        let mut args = vec!["x".to_owned()];
        if !kinds.is_empty() {
            args.extend(["--kinds".to_owned(), kinds.join(",")]);
        }
        if !exclude.is_empty() {
            args.extend(["--exclude-kinds".to_owned(), exclude.join(",")]);
        }
        Cli::try_parse_from(args).unwrap().options
    }

    #[test]
    fn ml_is_required_only_when_kinds_name_it() {
        assert_eq!(ml_wanted(&options(&[], &[])), MlWanted::IfAvailable);
        assert_eq!(ml_wanted(&options(&["faces"], &[])), MlWanted::Required);
        assert_eq!(ml_wanted(&options(&["thumbnails"], &[])), MlWanted::No);
        let all = all_ml_kinds();
        assert_eq!(ml_wanted(&options(&[], &all)), MlWanted::No);
        assert_eq!(ml_wanted(&options(&[], &["faces"])), MlWanted::IfAvailable);
        assert_eq!(worker_host(), worker_name().rsplit_once(':').unwrap().0);
    }

    #[test]
    fn kinds_keeps_only_the_named_kinds() {
        let mut registry = sample_registry();
        select_kinds(&mut registry, &names(&["faces", "noop"]), &[]).unwrap();
        assert_eq!(registry.kinds(), ["faces", "noop"]);
    }

    #[test]
    fn exclude_kinds_removes_the_named_kinds() {
        let mut registry = sample_registry();
        select_kinds(&mut registry, &[], &names(&["faces", "clip_embed"])).unwrap();
        assert_eq!(registry.kinds(), ["geocode", "noop", "thumbnails"]);
    }

    #[test]
    fn unknown_kinds_are_refused_with_the_known_list() {
        let mut registry = sample_registry();
        let err = select_kinds(&mut registry, &names(&["thumbnail"]), &[]).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("thumbnail"), "{message}");
        assert!(
            message.contains("clip_embed") && message.contains("thumbnails"),
            "{message}"
        );
        assert!(select_kinds(&mut registry, &[], &names(&["nope"])).is_err());
        assert_eq!(registry.kinds().len(), 4, "nothing removed on error");
    }

    #[test]
    fn kinds_and_exclude_kinds_together_are_refused() {
        let mut registry = sample_registry();
        let err =
            select_kinds(&mut registry, &names(&["faces"]), &names(&["geocode"])).unwrap_err();
        assert!(err.to_string().contains("together"), "{err}");
        #[derive(clap::Parser)]
        struct Cli {
            #[command(flatten)]
            options: WorkerOptions,
        }
        use clap::Parser;
        assert!(
            Cli::try_parse_from(["x", "--kinds", "faces", "--exclude-kinds", "geocode"]).is_err()
        );
        let cli = Cli::try_parse_from(["x", "--kinds", "faces,cluster_faces"]).unwrap();
        assert_eq!(cli.options.kinds, names(&["faces", "cluster_faces"]));
        assert_eq!(cli.options.ml_concurrency, 1);
        assert!(Cli::try_parse_from(["x", "--ml-concurrency", "2"]).is_err());
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
            geodata_file: None,
            pdfium_dir: None,
            kinds: Vec::new(),
            exclude_kinds: Vec::new(),
            ml_concurrency: 1,
            models_dir: None,
            ort_dylib: None,
            ml_threads: None,
            ml_exec: MlExecMode::InProcess,
        };
        let dir = tempfile::tempdir().unwrap();
        let storage = BlobStore::open(dir.path()).unwrap();
        let worker = tokio::spawn(run(
            pool.clone(),
            storage,
            dir.path().to_owned(),
            options,
            stop_rx,
        ));
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
