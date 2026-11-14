//! Postgres repositories and migrations.

pub mod blobs;
mod error;
pub mod instance;
pub mod invitations;
pub mod nodes;
mod page;
pub mod quota;
mod retry;
pub mod sessions;
pub mod settings;
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

pub async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
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
