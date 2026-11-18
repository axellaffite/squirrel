//! CLIP image embeddings (one per blob and model) and the label bank's text
//! embeddings. Content-derived, like `media`: never answer a user from them
//! without resolving one of the caller's own assets first.

use pgvector::HalfVector;
use sqlx::postgres::PgExecutor;
use sqlx::{PgConnection, PgPool};

use crate::DbError;

/// Dimensions of the stored embeddings (`halfvec(512)`).
pub const DIM: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredEmbedding {
    pub values: Vec<f32>,
    /// The label bank version the blob's `clip` tags were computed with.
    pub tagged_labels_version: Option<i16>,
}

fn to_f32(vector: &HalfVector) -> Vec<f32> {
    vector.as_slice().iter().map(|v| v.to_f32()).collect()
}

/// Stores (or replaces) the blob's embedding for `model`; its tags must be
/// computed again (`tagged_labels_version` back to NULL).
pub async fn store_embedding<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
    model: &str,
    values: &[f32],
) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO clip_embeddings (sha256, model, embedding) VALUES ($1, $2, $3) \
         ON CONFLICT (sha256, model) DO UPDATE \
            SET embedding = EXCLUDED.embedding, tagged_labels_version = NULL, created_at = now()",
    )
    .bind(sha256)
    .bind(model)
    .bind(HalfVector::from_f32_slice(values))
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn embedding<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
    model: &str,
) -> Result<Option<StoredEmbedding>, DbError> {
    let row: Option<(HalfVector, Option<i16>)> = sqlx::query_as(
        "SELECT embedding, tagged_labels_version FROM clip_embeddings \
          WHERE sha256 = $1 AND model = $2",
    )
    .bind(sha256)
    .bind(model)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|(vector, tagged_labels_version)| StoredEmbedding {
        values: to_f32(&vector),
        tagged_labels_version,
    }))
}

/// Records that the blob's tags were computed with `labels_version`.
pub async fn mark_tagged(
    conn: &mut PgConnection,
    sha256: &str,
    model: &str,
    labels_version: i16,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE clip_embeddings SET tagged_labels_version = $3 WHERE sha256 = $1 AND model = $2",
    )
    .bind(sha256)
    .bind(model)
    .bind(labels_version)
    .execute(conn)
    .await?;
    Ok(())
}

/// The label bank for (`model`, `labels_version`) computed from content of
/// `digest`, sorted by tag; empty when it was never computed (or computed
/// from other content).
pub async fn label_embeddings(
    pool: &PgPool,
    model: &str,
    labels_version: i16,
    digest: &str,
) -> Result<Vec<(String, Vec<f32>)>, DbError> {
    let rows: Vec<(String, HalfVector)> = sqlx::query_as(
        "SELECT e.tag, e.embedding FROM clip_label_embeddings e \
           JOIN clip_label_banks b \
             ON b.model = e.model AND b.labels_version = e.labels_version AND b.digest = $3 \
          WHERE e.model = $1 AND e.labels_version = $2 ORDER BY e.tag",
    )
    .bind(model)
    .bind(labels_version)
    .bind(digest)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(tag, vector)| (tag, to_f32(&vector)))
        .collect())
}

/// Replaces the whole bank for (`model`, `labels_version`), with the digest of
/// the content it was computed from, in one transaction.
pub async fn store_label_embeddings(
    pool: &PgPool,
    model: &str,
    labels_version: i16,
    digest: &str,
    labels: &[(String, Vec<f32>)],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO clip_label_banks (model, labels_version, digest) VALUES ($1, $2, $3) \
         ON CONFLICT (model, labels_version) DO UPDATE \
            SET digest = EXCLUDED.digest, created_at = now()",
    )
    .bind(model)
    .bind(labels_version)
    .bind(digest)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM clip_label_embeddings WHERE model = $1 AND labels_version = $2")
        .bind(model)
        .bind(labels_version)
        .execute(&mut *tx)
        .await?;
    for (tag, values) in labels {
        sqlx::query(
            "INSERT INTO clip_label_embeddings (model, labels_version, tag, embedding) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(model)
        .bind(labels_version)
        .bind(tag)
        .bind(HalfVector::from_f32_slice(values))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
