mod admin;
mod data_dir;
mod geodata;
mod media_exec;
mod tasks;
mod worker;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{ArgAction, Args, Parser, Subcommand};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, AppState, SetupToken, TrustedProxies};
use squirrel_storage::BlobStore;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

/// Docker's default `stop` timeout: SIGKILL follows SIGTERM after this long.
/// Every shutdown step below finishes within it.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
/// In-flight requests get this long to finish after a shutdown signal.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(9);
/// Time for the embedded worker to release its jobs after its grace period
/// (a few single-row updates). The embedded worker drains at the same time as
/// HTTP, so the server is done within `max(SHUTDOWN_DEADLINE,
/// worker::SHUTDOWN_GRACE + WORKER_RELEASE_MARGIN)` of the signal.
const WORKER_RELEASE_MARGIN: Duration = Duration::from_secs(1);
const _: () = assert!(SHUTDOWN_DEADLINE.as_millis() < STOP_TIMEOUT.as_millis());
const _: () = assert!(
    worker::SHUTDOWN_GRACE.as_millis() + WORKER_RELEASE_MARGIN.as_millis()
        < STOP_TIMEOUT.as_millis()
);
/// A configured setup token shorter than this is guessable; warn about it.
const MIN_SETUP_TOKEN_LEN: usize = 16;

#[derive(Parser)]
#[command(name = "squirrel", version, about = "Self-hosted home drive")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (applies pending migrations first).
    Serve(ServeArgs),
    /// Apply pending database migrations and exit.
    Migrate(DatabaseArgs),
    /// Print the OpenAPI document to stdout.
    Openapi,
    /// Run the background job worker. Never migrates: waits until the server
    /// (or `squirrel migrate`) has applied this binary's migrations.
    Worker(WorkerArgs),
    /// Administrative commands (run on the server). Never migrates: fails if
    /// migrations are pending (run `squirrel migrate` or start the server first).
    Admin(AdminArgs),
    /// Internal: decode one untrusted media input under resource limits
    /// (spawned by the worker; JSON on stdin and stdout).
    #[command(hide = true)]
    MediaExec(media_exec::MediaExecArgs),
}

#[derive(Args)]
struct AdminArgs {
    /// Required by every command except `geodata`.
    #[arg(long, env = "SQUIRREL_DATABASE_URL")]
    database_url: Option<String>,
    #[command(subcommand)]
    command: admin::AdminCommand,
}

#[derive(Args)]
struct WorkerArgs {
    #[command(flatten)]
    database: DatabaseArgs,
    /// The server's data directory: blobs are read from it, thumbnails
    /// written under `derived/`.
    #[arg(long, env = "SQUIRREL_DATA_DIR")]
    data_dir: PathBuf,
    #[arg(long, env = "SQUIRREL_LOG_JSON", default_value_t = false, action = ArgAction::Set)]
    log_json: bool,
    #[command(flatten)]
    options: worker::WorkerOptions,
}

#[derive(Args)]
struct DatabaseArgs {
    #[arg(long, env = "SQUIRREL_DATABASE_URL")]
    database_url: String,
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    database: DatabaseArgs,
    #[arg(long, env = "SQUIRREL_BIND", default_value = "0.0.0.0:8080")]
    bind: SocketAddr,
    /// Set to false only for local HTTP development.
    #[arg(long, env = "SQUIRREL_COOKIE_SECURE", default_value_t = true, action = ArgAction::Set)]
    cookie_secure: bool,
    #[arg(long, env = "SQUIRREL_LOG_JSON", default_value_t = false, action = ArgAction::Set)]
    log_json: bool,
    /// Code required to create the first administrator. Generated and logged
    /// on startup when unset and no account exists yet.
    #[arg(long, env = "SQUIRREL_SETUP_TOKEN", hide_env_values = true)]
    setup_token: Option<String>,
    /// Reverse proxies (comma-separated IPs or CIDRs) whose X-Forwarded-For is
    /// believed for rate limiting and logs. Empty: use the TCP peer address.
    #[arg(long, env = "SQUIRREL_TRUSTED_PROXIES", default_value = "")]
    trusted_proxies: TrustedProxies,
    /// Where blobs and in-progress uploads live (created if missing). `tmp/` and
    /// `blobs/` inside it must be on the same filesystem.
    #[arg(long, env = "SQUIRREL_DATA_DIR")]
    data_dir: PathBuf,
    /// Also run the job worker inside the server (development, tests, tiny
    /// installs). By default it runs as its own process (`squirrel worker`).
    #[arg(
        long = "worker",
        env = "SQUIRREL_EMBEDDED_WORKER",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        action = ArgAction::Set
    )]
    embedded_worker: bool,
    #[command(flatten)]
    worker_options: worker::WorkerOptions,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        // Before any runtime thread exists, and without logging to stdout,
        // which carries the response.
        Command::MediaExec(args) => media_exec::run(&args),
        command => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("starting the async runtime")?
            .block_on(run(command)),
    }
}

async fn run(command: Command) -> anyhow::Result<()> {
    match command {
        Command::MediaExec(args) => media_exec::run(&args),
        Command::Openapi => print_openapi(),
        Command::Migrate(args) => {
            init_tracing(false);
            migrate(&args).await
        }
        Command::Serve(args) => {
            init_tracing(args.log_json);
            serve(args).await
        }
        Command::Worker(args) => {
            init_tracing(args.log_json);
            run_worker(args).await
        }
        Command::Admin(args) => {
            init_tracing(false);
            // Offline tools first: they need no database.
            let command = match admin::run_offline(args.command)? {
                Some(command) => command,
                None => return Ok(()),
            };
            let database_url = args
                .database_url
                .context("--database-url (or SQUIRREL_DATABASE_URL) is required")?;
            let pool = connect_migrated(&DatabaseArgs { database_url }).await?;
            admin::run(&pool, command).await
        }
    }
}

fn print_openapi() -> anyhow::Result<()> {
    println!("{}", squirrel_api::openapi().to_pretty_json()?);
    Ok(())
}

async fn connect_and_migrate(args: &DatabaseArgs) -> anyhow::Result<PgPool> {
    let pool = squirrel_db::connect(&args.database_url)
        .await
        .context("connecting to Postgres")?;
    squirrel_db::migrate(&pool)
        .await
        .context("applying migrations")?;
    Ok(pool)
}

/// Connects without migrating; refuses a database whose schema is behind
/// this binary, so an admin command never upgrades a live instance by surprise.
async fn connect_migrated(args: &DatabaseArgs) -> anyhow::Result<PgPool> {
    let pool = squirrel_db::connect(&args.database_url)
        .await
        .context("connecting to Postgres")?;
    let pending = squirrel_db::pending_migrations(&pool)
        .await
        .context("checking applied migrations")?;
    if !pending.is_empty() {
        anyhow::bail!("{}", pending_message(&pending));
    }
    Ok(pool)
}

fn pending_message(pending: &[i64]) -> String {
    let versions: Vec<String> = pending.iter().map(i64::to_string).collect();
    format!(
        "the database has {} pending migration(s) ({}); run `squirrel migrate` \
         (or start the server) first",
        pending.len(),
        versions.join(", ")
    )
}

async fn run_worker(args: WorkerArgs) -> anyhow::Result<()> {
    let pool = squirrel_db::connect(&args.database.database_url)
        .await
        .context("connecting to Postgres")?;
    worker::wait_for_migrations(&pool).await?;
    let storage = BlobStore::open(&args.data_dir)
        .with_context(|| format!("opening data directory {}", args.data_dir.display()))?;
    data_dir::bind(&pool, &storage, &args.data_dir).await?;
    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = stop_tx.send(true);
    });
    worker::run(pool, storage, args.data_dir, args.options, stop_rx).await
}

async fn migrate(args: &DatabaseArgs) -> anyhow::Result<()> {
    connect_and_migrate(args).await?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let pool = connect_and_migrate(&args.database).await?;
    let storage = BlobStore::open(&args.data_dir)
        .with_context(|| format!("opening data directory {}", args.data_dir.display()))?;
    data_dir::bind(&pool, &storage, &args.data_dir).await?;
    if args.embedded_worker {
        // Refuse a wrong pdfium directory now rather than in the background.
        worker::pdfium_dir(args.worker_options.pdfium_dir.as_deref())?;
    }
    let setup_token = resolve_setup_token(&pool, args.setup_token.clone()).await?;
    let config = ApiConfig {
        cookie_secure: args.cookie_secure,
        setup_token,
        trusted_proxies: args.trusted_proxies.clone(),
        ..ApiConfig::default()
    };
    let state = AppState::new(pool.clone(), storage.clone(), config);
    let app = squirrel_api::router(state.clone()).layer(TraceLayer::new_for_http());

    // Precompute the timing-equalizer hash so the first unknown-user login isn't slower.
    tokio::task::spawn_blocking(squirrel_auth::warm_up)
        .await
        .context("warming up auth")?;
    tasks::spawn_all(&state);

    let listener = TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("binding {}", args.bind))?;
    tracing::info!(address = %args.bind, "squirrel is listening");
    let (stopping_tx, mut stopping_rx) = watch::channel(false);
    let embedded_worker = args.embedded_worker.then(|| {
        tracing::info!("running the job worker inside the server");
        worker::spawn_logged(worker::run(
            pool.clone(),
            storage,
            args.data_dir.clone(),
            args.worker_options.clone(),
            stopping_rx.clone(),
        ))
    });
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        let _ = stopping_tx.send(true);
    });
    let deadline = async move {
        // Errors only if the sender is gone, i.e. the server already stopped.
        let _ = stopping_rx.wait_for(|stopping| *stopping).await;
        tokio::time::sleep(SHUTDOWN_DEADLINE).await;
    };
    let served = tokio::select! {
        result = server => result.context("serving HTTP"),
        () = deadline => {
            tracing::warn!(
                deadline_secs = SHUTDOWN_DEADLINE.as_secs(),
                "graceful shutdown deadline reached, dropping remaining connections"
            );
            Ok(())
        }
    };
    if let Some(worker) = embedded_worker {
        stop_embedded_worker(worker).await;
    }
    served
}

/// The worker saw the same stop signal as the server (or its sender is gone):
/// wait for it to drain and release its jobs, bounded by its grace period.
/// Its failure, if any, was already logged when it happened ([`worker::spawn_logged`]).
async fn stop_embedded_worker(worker: tokio::task::JoinHandle<()>) {
    let bound = worker::SHUTDOWN_GRACE + WORKER_RELEASE_MARGIN;
    match tokio::time::timeout(bound, worker).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => tracing::error!(error = %err, "the job worker panicked"),
        Err(_) => tracing::warn!("the job worker did not stop in time; its leases will expire"),
    }
}

/// Uses the configured setup token, or generates and logs one while the
/// instance still has no account. Once set up, no token is needed.
async fn resolve_setup_token(
    pool: &PgPool,
    configured: Option<String>,
) -> anyhow::Result<Option<SetupToken>> {
    if let Some(token) = configured.filter(|t| !t.trim().is_empty()) {
        let token = token.trim();
        if is_weak_setup_token(token) {
            tracing::warn!(
                min_len = MIN_SETUP_TOKEN_LEN,
                "SQUIRREL_SETUP_TOKEN is short and could be guessed; use a longer random value"
            );
        }
        return Ok(Some(SetupToken::new(token)));
    }
    if squirrel_db::users::any_exists(pool)
        .await
        .context("checking for existing accounts")?
    {
        return Ok(None);
    }
    let token = SetupToken::new(squirrel_auth::generate_token());
    tracing::warn!(
        "first-run setup code: {} (enter it on the setup page to create the administrator)",
        token.expose()
    );
    Ok(Some(token))
}

fn is_weak_setup_token(token: &str) -> bool {
    token.chars().count() < MIN_SETUP_TOKEN_LEN
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,squirrel=debug,tower_http=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_migrations_are_listed_in_the_error() {
        assert_eq!(
            pending_message(&[20261113000002, 20261114000001]),
            "the database has 2 pending migration(s) (20261113000002, 20261114000001); \
             run `squirrel migrate` (or start the server) first"
        );
    }

    fn serve_args(extra: &[&str]) -> ServeArgs {
        let base = [
            "squirrel",
            "serve",
            "--database-url",
            "postgres://x",
            "--data-dir",
            "/tmp/x",
        ];
        let argv: Vec<&str> = base.iter().chain(extra).copied().collect();
        match Cli::try_parse_from(argv).unwrap().command {
            Command::Serve(args) => args,
            _ => unreachable!("parsed as serve"),
        }
    }

    #[test]
    fn the_embedded_worker_is_a_flag_that_can_be_turned_off() {
        assert!(serve_args(&["--worker"]).embedded_worker);
        assert!(serve_args(&["--worker", "true"]).embedded_worker);
        assert!(!serve_args(&["--worker", "false"]).embedded_worker);
        assert_eq!(
            serve_args(&["--concurrency", "3"])
                .worker_options
                .concurrency,
            Some(3)
        );
        let zero = [
            "squirrel",
            "worker",
            "--database-url",
            "x",
            "--data-dir",
            "/tmp/x",
            "--concurrency",
            "0",
        ];
        assert!(Cli::try_parse_from(zero).is_err());
        assert_eq!(
            serve_args(&["--media-exec", "in-process"])
                .worker_options
                .media_exec,
            worker::MediaExecMode::InProcess
        );
        assert_eq!(
            serve_args(&[]).worker_options.media_exec,
            worker::MediaExecMode::Subprocess
        );
        assert_eq!(
            serve_args(&["--pdfium-dir", "/opt/pdfium/lib"])
                .worker_options
                .pdfium_dir,
            Some("/opt/pdfium/lib".into())
        );
    }

    #[test]
    fn setup_tokens_shorter_than_16_characters_are_weak() {
        assert!(is_weak_setup_token("short"));
        assert!(is_weak_setup_token("fifteen-chars-x"));
        assert!(!is_weak_setup_token("sixteen-chars-xx"));
        assert!(!is_weak_setup_token(&squirrel_auth::generate_token()));
    }
}
