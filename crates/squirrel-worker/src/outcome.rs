//! How a step's handler ends when it cannot produce its result.

use squirrel_db::media::{self, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};

use crate::context::MediaContext;

/// A terminal state for this blob's `step`: the job itself succeeded.
pub async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    step: Step,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    tracing::info!(
        sha256,
        ?step,
        state = state.as_str(),
        reason,
        "step not done"
    );
    media::set_state(&ctx.pool, sha256, step, state, Some(reason))
        .await
        .map_err(JobError::retry)
}

/// Retries a transient failure; on the last attempt records `failed` first so
/// the UI does not show "Preparing…" forever.
pub async fn give_up_or_retry(
    ctx: &MediaContext,
    job: &JobContext,
    sha256: &str,
    step: Step,
    reason: String,
) -> Result<(), JobError> {
    if job.is_last_attempt() {
        media::set_state(&ctx.pool, sha256, step, MediaState::Failed, Some(&reason))
            .await
            .map_err(JobError::retry)?;
    }
    Err(JobError::retry(anyhow::anyhow!(reason)))
}
