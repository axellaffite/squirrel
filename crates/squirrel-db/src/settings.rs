use sqlx::{FromRow, PgPool};

use crate::DbError;

#[derive(Debug, Clone, Copy, FromRow)]
pub struct InstanceSettings {
    pub invites_admin_only: bool,
}

pub async fn get(pool: &PgPool) -> Result<InstanceSettings, DbError> {
    Ok(
        sqlx::query_as::<_, InstanceSettings>("SELECT invites_admin_only FROM instance_settings")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn set_invites_admin_only(
    pool: &PgPool,
    value: bool,
) -> Result<InstanceSettings, DbError> {
    Ok(sqlx::query_as::<_, InstanceSettings>(
        "UPDATE instance_settings SET invites_admin_only = $1 RETURNING invites_admin_only",
    )
    .bind(value)
    .fetch_one(pool)
    .await?)
}
