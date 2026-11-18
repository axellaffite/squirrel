//! Tags of a blob (`asset_tags`), per source. Content-derived like `media`.

use sqlx::PgConnection;
use sqlx::postgres::PgExecutor;

use crate::DbError;

/// Where a tag came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagSource {
    /// Zero-shot CLIP scoring against the label bank.
    Clip,
}

impl TagSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clip => "clip",
        }
    }
}

/// Replaces the blob's tags of `source` (other sources are untouched).
pub async fn replace_tags(
    conn: &mut PgConnection,
    sha256: &str,
    source: TagSource,
    tags: &[(String, f32)],
    labels_version: i16,
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM asset_tags WHERE sha256 = $1 AND source = $2")
        .bind(sha256)
        .bind(source.as_str())
        .execute(&mut *conn)
        .await?;
    for (tag, score) in tags {
        sqlx::query(
            "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(sha256)
        .bind(tag)
        .bind(source.as_str())
        .bind(score)
        .bind(labels_version)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// The blob's tags, best score first.
pub async fn tags_of<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
) -> Result<Vec<(String, f32)>, DbError> {
    Ok(sqlx::query_as(
        "SELECT tag, score FROM asset_tags WHERE sha256 = $1 ORDER BY score DESC, tag",
    )
    .bind(sha256)
    .fetch_all(executor)
    .await?)
}
