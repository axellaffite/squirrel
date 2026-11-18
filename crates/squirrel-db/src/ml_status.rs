//! What worker hosts reported about machine learning (`ml_worker_status`).

use sqlx::PgPool;

use crate::DbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlState {
    Ready,
    Unavailable,
}

impl MlState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlStatus {
    pub state: MlState,
    /// Why not ready: a fixed, generic phrase.
    pub reason: Option<String>,
}

/// Why ML is off when no worker host ever reported.
pub const NO_REPORT: &str = "no worker has reported";

/// Records what `host` found at start (replacing its previous report).
pub async fn report(
    pool: &PgPool,
    host: &str,
    state: MlState,
    reason: Option<&str>,
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO ml_worker_status (host, state, reason) VALUES ($1, $2, $3) \
         ON CONFLICT (host) DO UPDATE \
            SET state = EXCLUDED.state, reason = EXCLUDED.reason, updated_at = now()",
    )
    .bind(host)
    .bind(state.as_str())
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ready when any host is; otherwise the most recent host's reason.
pub async fn current(pool: &PgPool) -> Result<MlStatus, DbError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT state, reason FROM ml_worker_status \
          ORDER BY state = 'ready' DESC, updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((state, _)) if state == "ready" => MlStatus {
            state: MlState::Ready,
            reason: None,
        },
        Some((_, reason)) => MlStatus {
            state: MlState::Unavailable,
            reason,
        },
        None => MlStatus {
            state: MlState::Unavailable,
            reason: Some(NO_REPORT.to_owned()),
        },
    })
}
