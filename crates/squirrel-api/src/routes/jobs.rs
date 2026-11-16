//! The background job queue, as administrators see it: counts and failures.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use squirrel_jobs::admin::{self, FailedJob, KindSummary, RetryOutcome};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::client_ip::ClientIp;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::ApiPath;
use crate::state::AppState;

/// Failures listed by `GET /api/admin/jobs`, newest first.
const RECENT_FAILURES: i64 = 50;

#[derive(Debug, Serialize, ToSchema)]
pub struct JobSummaryDto {
    pub kind: String,
    pub queued: i64,
    pub running: i64,
    pub failed: i64,
}

impl From<KindSummary> for JobSummaryDto {
    fn from(summary: KindSummary) -> Self {
        Self {
            kind: summary.kind,
            queued: summary.queued,
            running: summary.running,
            failed: summary.failed,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FailedJobDto {
    pub id: i64,
    pub kind: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

impl From<FailedJob> for FailedJobDto {
    fn from(job: FailedJob) -> Self {
        Self {
            id: job.id,
            kind: job.kind,
            attempts: job.attempts,
            last_error: job.last_error,
            updated_at: job.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobsOverview {
    /// Queued, running and failed counts per job kind, kinds sorted.
    pub kinds: Vec<JobSummaryDto>,
    /// The 50 most recent failures, newest first (kept 30 days).
    pub failures: Vec<FailedJobDto>,
}

fn internal(err: sqlx::Error) -> ApiError {
    ApiError::Internal(err.into())
}

#[utoipa::path(get, path = "/api/admin/jobs", tag = "jobs",
    responses((status = 200, body = JobsOverview), (status = 403, body = ErrorBody)))]
pub async fn jobs_overview(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<JobsOverview>, ApiError> {
    current.require_admin()?;
    let kinds = admin::summary(&state.pool).await.map_err(internal)?;
    let failures = admin::failed(&state.pool, RECENT_FAILURES)
        .await
        .map_err(internal)?;
    Ok(Json(JobsOverview {
        kinds: kinds.into_iter().map(Into::into).collect(),
        failures: failures.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(post, path = "/api/admin/jobs/{id}/retry", tag = "jobs",
    params(("id" = i64, Path, description = "Failed job id")),
    responses(
        (status = 204, description = "Queued again with fresh attempts (or an equivalent job already was)"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "No failed job with this id")))]
pub async fn retry_job(
    State(state): State<AppState>,
    current: CurrentUser,
    client_ip: ClientIp,
    ApiPath(id): ApiPath<i64>,
) -> Result<StatusCode, ApiError> {
    current.require_admin()?;
    let outcome = admin::retry_failed(&state.pool, id)
        .await
        .map_err(internal)?;
    if outcome == RetryOutcome::NotFound {
        return Err(ApiError::NotFound);
    }
    tracing::info!(user_id = %current.user.id, username = %current.user.username, %client_ip,
        job_id = id, ?outcome, "failed job retried");
    Ok(StatusCode::NO_CONTENT)
}
