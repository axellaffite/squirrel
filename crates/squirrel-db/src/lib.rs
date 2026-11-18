//! Postgres repositories and migrations.

pub mod albums;
pub mod assets;
pub mod blobs;
pub mod clip;
pub mod device_assets;
pub mod devices;
pub mod documents;
mod error;
pub mod instance;
pub mod invitations;
pub mod media;
pub mod ml_settings;
pub mod ml_status;
pub mod nodes;
mod page;
pub mod quota;
mod retry;
pub mod sessions;
pub mod settings;
pub mod sync_folders;
pub mod tags;
pub mod timeline;
pub mod trash;
pub mod uploads;
pub mod users;

pub use error::DbError;
pub use page::Page;

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::PgPoolOptions;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const MAX_CONNECTIONS: u32 = 16;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect(database_url)
        .await
}

/// Why the database cannot hold Squirrel's schema.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error(
        "the pgvector extension is not available on this Postgres server: \
         use the pgvector/pgvector image or install pgvector 0.8"
    )]
    VectorUnavailable,
    /// pgvector is not a trusted extension: only a superuser may create it.
    #[error("pgvector must be installed by a superuser: CREATE EXTENSION vector;")]
    VectorNeedsSuperuser,
    #[error("checking the pgvector extension: {0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error(transparent)]
    Extension(#[from] ExtensionError),
    #[error(transparent)]
    Migrate(#[from] MigrateError),
}

/// Checks that the migrations will be able to `CREATE EXTENSION vector`:
/// it is installed already, or available and the current role is a superuser.
pub async fn check_vector_extension(pool: &PgPool) -> Result<(), ExtensionError> {
    let (installed, available, superuser): (bool, bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector'), \
                EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'vector'), \
                (SELECT rolsuper FROM pg_roles WHERE rolname = current_user)",
    )
    .fetch_one(pool)
    .await?;
    match (installed, available, superuser) {
        (true, _, _) | (false, true, true) => Ok(()),
        (false, false, _) => Err(ExtensionError::VectorUnavailable),
        (false, true, false) => Err(ExtensionError::VectorNeedsSuperuser),
    }
}

/// Applies pending migrations, after checking the extensions they need.
pub async fn migrate(pool: &PgPool) -> Result<(), MigrationError> {
    check_vector_extension(pool).await?;
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Versions of [`MIGRATOR`] not successfully applied to the database (all of
/// them on an empty database), in order. Reads only: never migrates.
pub async fn pending_migrations(pool: &PgPool) -> Result<Vec<i64>, DbError> {
    let has_table: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    let applied: Vec<i64> = if has_table {
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
            .fetch_all(pool)
            .await?
    } else {
        Vec::new()
    };
    Ok(MIGRATOR
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| migration.version)
        .filter(|version| !applied.contains(version))
        .collect())
}
