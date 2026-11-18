//! The label bank's text embeddings (`clip_label_embeddings`), computed once
//! per (model, labels version) through a `clip-text` child, then reused by
//! every `clip_embed` run.

use sqlx::PgPool;
use squirrel_db::clip;
use squirrel_ml::clip::labels::{self, LABELS_VERSION, Label};
use squirrel_ml::protocol::{MAX_TEXTS, MlBody, MlOp, MlOutput};

use crate::ml_client::MlClient;

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

/// Every label and the background are stored.
fn complete(stored: &[(String, Vec<f32>)], labels: &[Label]) -> bool {
    stored.len() == labels.len() + 1
        && stored.iter().any(|(slug, _)| slug == labels::BACKGROUND)
        && labels
            .iter()
            .all(|label| stored.iter().any(|(slug, _)| *slug == label.slug))
}

/// Every prompt of every label (and the background prompts), encoded in
/// batches of [`MAX_TEXTS`], averaged per bank entry.
async fn compute(text: &MlClient, labels: &[Label]) -> anyhow::Result<Vec<(String, Vec<f32>)>> {
    let prompts = labels::bank_prompts(labels);
    let mut grouped: Vec<(String, Vec<Vec<f32>>)> = Vec::new();
    for batch in prompts.chunks(MAX_TEXTS) {
        let texts = batch.iter().map(|(_, prompt)| prompt.clone()).collect();
        let embeddings = match text.request(MlOp::ClipText { texts }).await? {
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
