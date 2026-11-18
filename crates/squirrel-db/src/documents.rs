//! Text extracted from documents (`document_text`, one row per blob) and the
//! search over it and over file names.
//!
//! Rows are content-derived and shared by every node referencing the blob:
//! [`search`] only ever reaches them through one of the caller's live nodes.

use sqlx::{FromRow, PgPool};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::nodes::NodeRecord;
use crate::{DbError, Page, media};

/// `document_text.body` holds at most this many bytes of UTF-8.
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Words of a query that count (shorter ones match too much to be useful).
const MIN_WORD_CHARS: usize = 2;
const MAX_QUERY_WORDS: usize = 16;
const MAX_WORD_CHARS: usize = 64;
/// A search may not run longer than this (a pathological query or a huge
/// library); it then fails as [`DbError::Busy`].
pub(crate) const SEARCH_TIMEOUT: &str = "5s";
/// SQLSTATE of a statement cancelled by `statement_timeout`.
const QUERY_CANCELED: &str = "57014";
/// Marks around matched words in [`SearchHit::snippet`].
pub const HIGHLIGHT_START: char = '\u{2}';
pub const HIGHLIGHT_END: char = '\u{3}';

/// Stores the text of a blob (replacing any earlier text) and marks its text
/// step ready, in one transaction. Control characters other than tabs and
/// line breaks are dropped (they would collide with the highlight marks; NUL
/// does not fit in `text`), then the body is cut to [`MAX_BODY_BYTES`].
/// `page_count`, when given, is recorded in the same transaction.
/// `false`: the media row is gone (the blob was collected meanwhile).
pub async fn store_text(
    pool: &PgPool,
    sha256: &str,
    text: &str,
    page_count: Option<u32>,
) -> Result<bool, DbError> {
    let body = clean_body(text);
    let mut tx = pool.begin().await?;
    if media::lock(&mut tx, sha256).await?.is_none() {
        return Ok(false);
    }
    if let Some(pages) = page_count {
        media::set_page_count(&mut *tx, sha256, pages).await?;
    }
    sqlx::query(
        "INSERT INTO document_text (sha256, body, content) \
         VALUES ($1, $2, to_tsvector('squirrel_text', translate($2, '._-/', '    '))) \
         ON CONFLICT (sha256) DO UPDATE \
            SET body = EXCLUDED.body, content = EXCLUDED.content, created_at = now()",
    )
    .bind(sha256)
    .bind(&body)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE media SET text_state = 'ready', updated_at = now() WHERE sha256 = $1")
        .bind(sha256)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// What `store_text` keeps of `text`.
pub fn clean_body(text: &str) -> String {
    let mut body: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
        .collect();
    body.truncate(body.floor_char_boundary(MAX_BODY_BYTES));
    body
}

/// The words of a user's query that are searched: runs of letters and digits
/// of at least two characters, at most 16 of them, joined by spaces. `None`
/// when nothing is left. Only letters, digits and spaces reach the database,
/// so no input can form text-search syntax.
pub fn search_terms(query: &str) -> Option<String> {
    // Composed first: a decomposed "é" would otherwise split the word.
    let query: String = query.nfc().collect();
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_WORD_CHARS)
        .take(MAX_QUERY_WORDS)
        .map(|word| word.chars().take(MAX_WORD_CHARS).collect())
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// A live file of the caller whose name or text matches a query.
#[derive(Debug, Clone, FromRow)]
pub struct SearchHit {
    #[sqlx(flatten)]
    pub node: NodeRecord,
    pub page_count: Option<i32>,
    /// Passages of the text around the matches (between [`HIGHLIGHT_START`]
    /// and [`HIGHLIGHT_END`]); `None` when only the name matched.
    pub snippet: Option<String>,
}

/// Every word of `query` must start a word of the file's name or text
/// (accents and case ignored). Name matches rank first, then by text
/// relevance, then the most recently modified. Only the owner's live files.
pub async fn search(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
    offset: u32,
    limit: u32,
) -> Result<Page<SearchHit>, DbError> {
    let Some(terms) = search_terms(query) else {
        return Ok(Page {
            items: Vec::new(),
            has_more: false,
        });
    };
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL statement_timeout = '{SEARCH_TIMEOUT}'"))
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query_as::<_, SearchHit>(SEARCH_SQL)
        .bind(owner_id)
        .bind(terms)
        .bind(i64::from(limit) + 1)
        .bind(i64::from(offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(timed_out_is_busy)?;
    tx.commit().await?;
    Ok(Page::from_overfetch(rows, limit))
}

pub(crate) fn timed_out_is_busy(err: sqlx::Error) -> DbError {
    match &err {
        sqlx::Error::Database(db) if db.code().as_deref() == Some(QUERY_CANCELED) => {
            DbError::Busy(err)
        }
        _ => err.into(),
    }
}

/// `$1` owner, `$2` the [`search_terms`], `$3` limit (+1), `$4` offset.
/// Each term becomes a quoted lexeme, a prefix from 3 characters on
/// (`'ecole':* & 'el'`), quotes and backslashes escaped although the terms
/// cannot contain them. The name expression is the one
/// `nodes_name_search_idx` indexes. Snippets are cut from the start of the
/// body with the same separators as the index, so "Rapport.pdf" highlights.
const SEARCH_SQL: &str = "\
WITH q AS (
    SELECT to_tsquery('squirrel_text', (
        SELECT string_agg(
                   '''' || replace(replace(u.lexeme, '\\', '\\\\'), '''', '''''') || ''''
                   || CASE WHEN length(u.lexeme) >= 3 THEN ':*' ELSE '' END,
                   ' & ')
          FROM unnest(to_tsvector('squirrel_text', $2)) AS u
         WHERE length(u.lexeme) >= 2)) AS query
), hits AS (
    SELECT n.id
      FROM nodes n, q
     WHERE n.owner_id = $1 AND n.kind = 'file' AND n.deleted_at IS NULL
       AND to_tsvector('squirrel_text', translate(n.name, '._-/', '    ')) @@ q.query
    UNION
    SELECT n.id
      FROM q, document_text d
      JOIN nodes n ON n.blob_sha256 = d.sha256
     WHERE d.content @@ q.query
       AND n.owner_id = $1 AND n.kind = 'file' AND n.deleted_at IS NULL
), page AS (
    SELECT n.id, n.owner_id, n.parent_id, n.name, n.kind, n.blob_sha256, n.size, n.mime,
           n.created_at, n.modified_at, n.deleted_at, m.page_count, d.body,
           d.content @@ q.query AS text_matches,
           to_tsvector('squirrel_text', translate(n.name, '._-/', '    ')) @@ q.query
             AS name_matches,
           COALESCE(ts_rank(d.content, q.query), 0.0::real) AS rank
      FROM hits h
      JOIN nodes n ON n.id = h.id
      CROSS JOIN q
      LEFT JOIN media m ON m.sha256 = n.blob_sha256
      LEFT JOIN document_text d ON d.sha256 = n.blob_sha256
     ORDER BY name_matches DESC, rank DESC, n.modified_at DESC, n.id
     LIMIT $3 OFFSET $4
)
SELECT p.id, p.owner_id, p.parent_id, p.name, p.kind, p.blob_sha256, p.size, p.mime,
       p.created_at, p.modified_at, p.deleted_at, p.page_count,
       CASE WHEN p.text_matches THEN
           ts_headline('squirrel_text', left(translate(p.body, '._-/', '    '), 65536), q.query,
               'StartSel=' || chr(2) || ', StopSel=' || chr(3)
               || ', MaxFragments=2, MaxWords=18, MinWords=6, FragmentDelimiter=\" … \"')
       END AS snippet
  FROM page p CROSS JOIN q
 ORDER BY p.name_matches DESC, p.rank DESC, p.modified_at DESC, p.id";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_keep_only_words_of_two_characters_or_more() {
        assert_eq!(search_terms("ecole elec").as_deref(), Some("ecole elec"));
        assert_eq!(
            search_terms("  Facture-2026.pdf ").as_deref(),
            Some("Facture 2026 pdf")
        );
        assert_eq!(
            search_terms("École 'élec':*").as_deref(),
            Some("École élec")
        );
        assert_eq!(search_terms("e\u{301}cole").as_deref(), Some("école"));
        for nothing in ["", "   ", "' & | !", "a", "a b c", "\\ :*"] {
            assert_eq!(search_terms(nothing), None, "{nothing:?}");
        }
    }

    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_timed_out_statement_is_busy(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL statement_timeout = '10ms'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let err = sqlx::query("SELECT pg_sleep(1)")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(matches!(timed_out_is_busy(err), DbError::Busy(_)));
        let other = sqlx::query("SELECT nope").execute(&pool).await.unwrap_err();
        assert!(matches!(timed_out_is_busy(other), DbError::Sqlx(_)));
    }

    #[test]
    fn terms_are_bounded() {
        let many = "word ".repeat(100);
        assert_eq!(search_terms(&many).unwrap().split(' ').count(), 16);
        let long = "x".repeat(1000);
        assert_eq!(search_terms(&long).unwrap().chars().count(), 64);
    }

    #[test]
    fn bodies_lose_control_characters_and_are_capped() {
        assert_eq!(clean_body("a\0b\u{2}c\u{3}d\ne\tf"), "abcd\ne\tf");
        let long = "é".repeat(MAX_BODY_BYTES);
        let body = clean_body(&long);
        assert!(body.len() <= MAX_BODY_BYTES);
        assert_eq!(body.len(), MAX_BODY_BYTES, "cut on a character boundary");
    }
}
