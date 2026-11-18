//! Per-user ML preferences (`user_ml_settings`; no row = defaults).

use sqlx::postgres::PgExecutor;
use uuid::Uuid;

use crate::DbError;

/// Faces are on unless the user turned them off.
pub const FACES_ENABLED_DEFAULT: bool = true;

pub async fn faces_enabled<'e>(executor: impl PgExecutor<'e>, user: Uuid) -> Result<bool, DbError> {
    let stored: Option<bool> =
        sqlx::query_scalar("SELECT faces_enabled FROM user_ml_settings WHERE user_id = $1")
            .bind(user)
            .fetch_optional(executor)
            .await?;
    Ok(stored.unwrap_or(FACES_ENABLED_DEFAULT))
}

pub async fn set_faces_enabled<'e>(
    executor: impl PgExecutor<'e>,
    user: Uuid,
    enabled: bool,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO user_ml_settings (user_id, faces_enabled) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET faces_enabled = EXCLUDED.faces_enabled, \
                                             updated_at = now()",
    )
    .bind(user)
    .bind(enabled)
    .execute(executor)
    .await?;
    Ok(())
}
