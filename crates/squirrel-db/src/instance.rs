//! The installation's identity, which binds a data directory to its database.

use sqlx::PgPool;
use uuid::Uuid;

use crate::DbError;

/// The random id created with this database (by its migrations).
pub async fn installation_id(pool: &PgPool) -> Result<Uuid, DbError> {
    Ok(
        sqlx::query_scalar("SELECT installation_id FROM instance_identity")
            .fetch_one(pool)
            .await?,
    )
}
