//! Postgres repositories and migrations.

pub mod access;
pub mod album_rules;
pub mod albums;
pub mod assets;
pub mod blobs;
pub mod blur_filter;
mod bounded;
pub mod clip;
pub mod clustering;
pub mod collab;
pub mod comments;
pub mod device_assets;
pub mod devices;
pub mod documents;
pub mod duplicates;
mod error;
pub mod explore;
pub mod faces;
pub mod friends;
pub mod instance;
pub mod invitations;
pub mod media;
pub mod ml_settings;
pub mod ml_status;
pub mod nodes;
pub mod notifications;
mod page;
pub mod people;
pub mod photo_filter;
pub mod photo_search;
pub mod quota;
mod retry;
pub mod sessions;
pub mod settings;
pub mod shares;
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
        "the pgvector extension is not available to database {database}: use the \
         pgvector/pgvector image or install pgvector >= {MIN_VECTOR_VERSION} on the server"
    )]
    VectorUnavailable { database: String },
    /// pgvector is not a trusted extension: only a superuser may create it.
    #[error(
        "pgvector must be installed by a superuser in database {database}: \
         CREATE EXTENSION vector;"
    )]
    VectorNeedsSuperuser { database: String },
    #[error("database {database} cannot create the pgvector extension: {message}")]
    VectorUnsupported { database: String, message: String },
    #[error(
        "database {database} has pgvector {found}; Squirrel needs >= {MIN_VECTOR_VERSION} \
         (ALTER EXTENSION vector UPDATE;)"
    )]
    VectorTooOld { database: String, found: String },
    #[error("checking the pgvector extension: {0}")]
    Db(#[from] sqlx::Error),
}

/// `halfvec` and the iterative index scans arrived with 0.7.
pub const MIN_VECTOR_VERSION: &str = "0.7";

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error(transparent)]
    Extension(#[from] ExtensionError),
    #[error(transparent)]
    Migrate(#[from] MigrateError),
}

/// Makes sure pgvector (>= 0.7) is installed in the database: creates it when
/// missing (which needs a superuser, or a superuser's prior
/// `CREATE EXTENSION vector`), with a clear message for each way that fails.
pub async fn check_vector_extension(pool: &PgPool) -> Result<(), ExtensionError> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    let installed = installed_vector(pool).await?;
    let version = match installed {
        Some(version) => version,
        None => {
            if let Err(err) = sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
                .execute(pool)
                .await
            {
                return Err(create_error(err, database));
            }
            installed_vector(pool).await?.unwrap_or_default()
        }
    };
    if !version_at_least(&version, MIN_VECTOR_VERSION) {
        return Err(ExtensionError::VectorTooOld {
            database,
            found: version,
        });
    }
    Ok(())
}

async fn installed_vector(pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT extversion FROM pg_extension WHERE extname = 'vector'")
        .fetch_optional(pool)
        .await
}

/// SQLSTATE of a failed `CREATE EXTENSION`, mapped to what the operator must do.
fn create_error(err: sqlx::Error, database: String) -> ExtensionError {
    let code = err
        .as_database_error()
        .and_then(|db| db.code())
        .map(|code| code.into_owned());
    match code.as_deref() {
        // insufficient_privilege
        Some("42501") => ExtensionError::VectorNeedsSuperuser { database },
        // undefined_file: no vector.control on the server
        Some("58P01") => ExtensionError::VectorUnavailable { database },
        // feature_not_supported (e.g. a version the server cannot load)
        Some("0A000") => ExtensionError::VectorUnsupported {
            database,
            message: err
                .as_database_error()
                .map_or_else(String::new, |db| db.message().to_owned()),
        },
        _ => ExtensionError::Db(err),
    }
}

/// `found >= wanted`, comparing dotted numeric versions ("0.8.7" >= "0.7").
fn version_at_least(found: &str, wanted: &str) -> bool {
    let parse =
        |v: &str| -> Vec<u32> { v.split('.').map(|part| part.parse().unwrap_or(0)).collect() };
    let (found, wanted) = (parse(found), parse(wanted));
    let len = found.len().max(wanted.len());
    let at = |v: &[u32], i: usize| v.get(i).copied().unwrap_or(0);
    (0..len)
        .map(|i| at(&found, i).cmp(&at(&wanted, i)))
        .find(|order| order.is_ne())
        .is_none_or(|order| order.is_gt())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(version_at_least("0.8.7", "0.7"));
        assert!(version_at_least("0.7", "0.7"));
        assert!(version_at_least("0.10.0", "0.7"));
        assert!(!version_at_least("0.6.2", "0.7"));
        assert!(!version_at_least("", "0.7"));
    }
}
