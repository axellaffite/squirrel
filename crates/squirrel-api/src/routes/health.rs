use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
}

/// Healthy when the database answers and the data directory is reachable.
#[utoipa::path(get, path = "/api/healthz", tag = "system", responses((status = 200, body = Health)))]
pub async fn healthz(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;
    let store = state.storage.clone();
    run_storage(move || store.available_space()).await?;
    Ok(Json(Health { status: "ok" }))
}
