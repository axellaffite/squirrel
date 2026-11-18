//! The label bank's text embeddings (`clip_label_embeddings`), computed once
//! per (model, labels version) through a `clip-text` child, then reused by
//! every `clip_embed` run.

use sqlx::PgPool;
use squirrel_db::clip;
use squirrel_jobs::{JobContext, JobError};
use squirrel_ml::clip::labels::{self, LABELS_VERSION, Label};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};

use crate::context::{MediaContext, MlContext};
use crate::ml_client::MlClient;

/// Prompts per request: small batches keep each request short (≈ 0.5 s),
/// so the child never looks stuck and a cancelled job loses little.
pub const BATCH: usize = 32;
/// One batch, however slow the machine.
pub const BATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// `label_bank`: computes the bank (when missing or stale), then queues the
/// re-tagging of photos embedded meanwhile.
pub async fn run(ctx: &MediaContext, ml: &MlContext, _job: JobContext) -> Result<(), JobError> {
    let Some(text) = &ml.text else {
        return Err(JobError::permanent(anyhow::anyhow!(
            "this worker has no CLIP text model"
        )));
    };
    let computed = ensure(&ctx.pool, text, squirrel_ml::CLIP_MODEL_ID)
        .await
        .map_err(JobError::retry)?;
    text.stop().await;
    if computed {
        let mut conn = ctx.pool.acquire().await.map_err(JobError::retry)?;
        crate::backfill::queue_retag(&mut conn).await?;
    }
    Ok(())
}

/// Whether the bank of `model` for the current labels version and content
/// still has to be computed.
pub async fn missing(pool: &PgPool, model: &str) -> anyhow::Result<bool> {
    let stored =
        clip::label_embeddings(pool, model, LABELS_VERSION, &labels::bank_digest()).await?;
    Ok(!complete(&stored, &labels::bank()))
}

/// Ensures the bank of `model` for [`LABELS_VERSION`] is stored. `true` when
/// it had to be computed (one text inference per prompt), `false` when it was
/// already complete (no inference).
pub async fn ensure(pool: &PgPool, text: &MlClient, model: &str) -> anyhow::Result<bool> {
    let labels = labels::bank();
    anyhow::ensure!(
        !labels.is_empty(),
        "the committed label bank does not parse"
    );
    let digest = labels::bank_digest();
    let stored = clip::label_embeddings(pool, model, LABELS_VERSION, &digest).await?;
    if complete(&stored, &labels) {
        return Ok(false);
    }
    let bank = compute(text, &labels).await?;
    clip::store_label_embeddings(pool, model, LABELS_VERSION, &digest, &bank).await?;
    tracing::info!(
        labels = bank.len(),
        version = LABELS_VERSION,
        "label bank computed"
    );
    Ok(true)
}

/// Every label is stored, and nothing else.
fn complete(stored: &[(String, Vec<f32>)], labels: &[Label]) -> bool {
    stored.len() == labels.len()
        && labels
            .iter()
            .all(|label| stored.iter().any(|(slug, _)| *slug == label.slug))
}

/// Every prompt of every label, encoded in
/// batches of [`BATCH`], averaged per bank entry.
async fn compute(text: &MlClient, labels: &[Label]) -> anyhow::Result<Vec<(String, Vec<f32>)>> {
    let prompts = labels::bank_prompts(labels);
    let mut grouped: Vec<(String, Vec<Vec<f32>>)> = Vec::new();
    for batch in prompts.chunks(BATCH) {
        let texts = batch.iter().map(|(_, prompt)| prompt.clone()).collect();
        let answer = tokio::time::timeout(BATCH_TIMEOUT, text.request(MlOp::ClipText { texts }))
            .await
            .map_err(|_| anyhow::anyhow!("a batch of label prompts took over {BATCH_TIMEOUT:?}"))?;
        let embeddings = match answer? {
            MlBody::Ok(MlOutput::Embeddings { embeddings }) => embeddings,
            other => anyhow::bail!("encoding the label prompts: {other:?}"),
        };
        anyhow::ensure!(embeddings.len() == batch.len(), "the child skipped prompts");
        for ((entry, _), embedding) in batch.iter().zip(embeddings) {
            squirrel_ml::clip::check_embedding(&embedding)
                .map_err(|why| anyhow::anyhow!("invalid text embedding: {why}"))?;
            match grouped.last_mut() {
                Some((last, list)) if last == entry => list.push(embedding),
                _ => grouped.push((entry.clone(), vec![embedding])),
            }
        }
    }
    grouped
        .into_iter()
        .map(|(entry, embeddings)| Ok((entry, labels::mean_embedding(&embeddings)?)))
        .collect::<Result<_, squirrel_ml::clip::ClipError>>()
        .map_err(Into::into)
}
