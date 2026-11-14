mod admin;
mod tasks;

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

/// In-flight requests get this long to finish after a shutdown signal.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(10);
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
    /// Administrative commands (run on the server). Never migrates: fails if
    /// migrations are pending (run `squirrel migrate` or start the server first).
    Admin(AdminArgs),
}

#[derive(Args)]
struct AdminArgs {
    #[command(flatten)]
    database: DatabaseArgs,
    #[command(subcommand)]
    command: admin::AdminCommand,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Openapi => print_openapi(),
        Command::Migrate(args) => {
            init_tracing(false);
            migrate(&args).await
        }
        Command::Serve(args) => {
            init_tracing(args.log_json);
            serve(args).await
        }
        Command::Admin(args) => {
            init_tracing(false);
            let pool = connect_migrated(&args.database).await?;
            admin::run(&pool, args.command).await
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

async fn migrate(args: &DatabaseArgs) -> anyhow::Result<()> {
    connect_and_migrate(args).await?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let pool = connect_and_migrate(&args.database).await?;
    let storage = BlobStore::open(&args.data_dir)
        .with_context(|| format!("opening data directory {}", args.data_dir.display()))?;
    let setup_token = resolve_setup_token(&pool, args.setup_token.clone()).await?;
    let config = ApiConfig {
        cookie_secure: args.cookie_secure,
        setup_token,
        trusted_proxies: args.trusted_proxies.clone(),
        ..ApiConfig::default()
    };
    let state = AppState::new(pool.clone(), storage, config);
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
    tokio::select! {
        result = server => result.context("serving HTTP"),
        () = deadline => {
            tracing::warn!(
                deadline_secs = SHUTDOWN_DEADLINE.as_secs(),
                "graceful shutdown deadline reached, dropping remaining connections"
            );
            Ok(())
        }
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

    #[test]
    fn setup_tokens_shorter_than_16_characters_are_weak() {
        assert!(is_weak_setup_token("short"));
        assert!(is_weak_setup_token("fifteen-chars-x"));
        assert!(!is_weak_setup_token("sixteen-chars-xx"));
        assert!(!is_weak_setup_token(&squirrel_auth::generate_token()));
    }
}
