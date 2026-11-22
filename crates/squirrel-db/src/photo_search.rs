//! Text search over the caller's photos: an exact cosine scan of the CLIP
//! embeddings of their own live assets (no ANN index), so another user's
//! library never changes a caller's results, recall is total, and the
//! timeline's filters compose as plain predicates.

use pgvector::HalfVector;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::documents::{SEARCH_TIMEOUT, timed_out_is_busy};
use crate::photo_filter::PhotoFilter;
use crate::timeline::{AssetSummaryRecord, FROM, SUMMARY_COLUMNS, TimelineCursor};
use crate::{DbError, Page};

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PhotoHit {
    #[sqlx(flatten)]
    pub summary: AssetSummaryRecord,
    /// Cosine distance to the query (0 = same direction).
    pub distance: f64,
}

/// What to look for and how far.
#[derive(Debug, Clone)]
pub struct PhotoQuery<'a> {
    /// The CLIP model whose embeddings are compared.
    pub model: &'a str,
    /// The query's unit embedding.
    pub embedding: &'a [f32],
    /// Results farther than this are dropped.
    pub max_distance: f64,
    /// Results farther than the best hit's distance plus this are dropped
    /// (a weak tail far behind a clear match is noise).
    pub relative_cutoff: f64,
    /// Unit embeddings competing with the query (e.g. the label "cat" for
    /// the query "dog"): a photo is kept only if its cosine with the query
    /// beats its cosine with every rival by `rival_margin`.
    pub rivals: &'a [Vec<f32>],
    pub rival_margin: f64,
    pub filter: &'a PhotoFilter,
}

/// How a page of results is ordered and where it starts.
#[derive(Debug, Clone, Copy)]
enum Order<'c> {
    /// Nearest first (distance, then node id), from `offset`.
    Relevance { offset: u32 },
    /// The `within` nearest results, newest capture first (`taken_at DESC,
    /// node_id DESC`, the timeline's order), after `after`.
    Date {
        within: u32,
        after: Option<&'c TimelineCursor>,
    },
}

/// The distance is computed in a fenced subquery (`OFFSET 0`: never pulled
/// up) over the caller's own joined rows, so the distance floor cannot be
/// pushed down into a scan of every user's embeddings: only the caller's
/// photos are ever scored. Within the floor, only results within
/// `relative_cutoff` of the best one are kept (a window over the caller's
/// rows). Every rival's distance is computed in the same pass; a photo
/// closer to a rival than `rival_margin` allows is dropped. Ranked by
/// distance, then node id; [`Order::Date`] then keeps the `within` best and
/// orders those by capture time, resuming after its keyset cursor.
fn search_query<'a>(
    prefix: &str,
    owner_id: Uuid,
    query: &PhotoQuery<'_>,
    order: Order<'_>,
    limit: u32,
) -> QueryBuilder<'a, Postgres> {
    let mut sql = QueryBuilder::<Postgres>::new(format!(
        "{prefix}WITH ranked AS (SELECT * FROM (SELECT {SUMMARY_COLUMNS}, (e.embedding <=> "
    ));
    sql.push_bind(HalfVector::from_f32_slice(query.embedding))
        .push(")::float8 AS distance, ");
    if query.rivals.is_empty() {
        sql.push("NULL::float8");
    } else {
        sql.push("LEAST(");
        for (index, rival) in query.rivals.iter().enumerate() {
            if index > 0 {
                sql.push(", ");
            }
            sql.push("e.embedding <=> ")
                .push_bind(HalfVector::from_f32_slice(rival));
        }
        sql.push(")::float8");
    }
    sql.push(format!(
        " AS rival {FROM} \
             JOIN clip_embeddings e ON e.sha256 = a.sha256 AND e.model = "
    ))
    .push_bind(query.model.to_owned())
    .push(" WHERE a.owner_id = ")
    .push_bind(owner_id);
    query.filter.push(&mut sql);
    sql.push(" OFFSET 0) mine WHERE distance <= ")
        .push_bind(query.max_distance)
        .push(" AND (rival IS NULL OR rival - distance >= ")
        .push_bind(query.rival_margin)
        .push(")")
        .push(
            "), matched AS (SELECT * FROM (SELECT ranked.*, min(distance) OVER () AS best FROM ranked) cut \
               WHERE distance <= best + ",
        )
        .push_bind(query.relative_cutoff)
        .push(") ");
    match order {
        Order::Relevance { offset } => {
            sql.push("SELECT * FROM matched ORDER BY distance, node_id OFFSET ")
                .push_bind(i64::from(offset));
        }
        Order::Date { within, after } => {
            sql.push("SELECT * FROM (SELECT * FROM matched ORDER BY distance, node_id LIMIT ")
                .push_bind(i64::from(within))
                .push(") nearest");
            if let Some(cursor) = after {
                sql.push(" WHERE (taken_at, node_id) < (")
                    .push_bind(cursor.taken_at)
                    .push(", ")
                    .push_bind(cursor.node_id)
                    .push(")");
            }
            sql.push(" ORDER BY taken_at DESC, node_id DESC");
        }
    }
    sql.push(" LIMIT ").push_bind(i64::from(limit) + 1);
    sql
}

/// Runs a search query under the search statement timeout.
async fn run(
    pool: &PgPool,
    owner_id: Uuid,
    query: &PhotoQuery<'_>,
    order: Order<'_>,
    limit: u32,
) -> Result<Page<PhotoHit>, DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL statement_timeout = '{SEARCH_TIMEOUT}'"))
        .execute(&mut *tx)
        .await?;
    let rows = search_query("", owner_id, query, order, limit)
        .build_query_as::<PhotoHit>()
        .fetch_all(&mut *tx)
        .await
        .map_err(timed_out_is_busy)?;
    tx.commit().await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// One page of `owner_id`'s live photos closest to the query, nearest first.
pub async fn search(
    pool: &PgPool,
    owner_id: Uuid,
    query: &PhotoQuery<'_>,
    offset: u32,
    limit: u32,
) -> Result<Page<PhotoHit>, DbError> {
    run(pool, owner_id, query, Order::Relevance { offset }, limit).await
}

/// One page of the `within` results [`search`] ranks first, newest capture
/// first (`taken_at DESC, node_id DESC`, as the timeline), after `after`.
/// The match set is [`search`]'s: only the order and the paging differ.
pub async fn search_by_date(
    pool: &PgPool,
    owner_id: Uuid,
    query: &PhotoQuery<'_>,
    within: u32,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<PhotoHit>, DbError> {
    run(pool, owner_id, query, Order::Date { within, after }, limit).await
}

/// The plan of [`search`]'s query (performance tests).
#[doc(hidden)]
pub async fn explain(
    pool: &PgPool,
    owner_id: Uuid,
    query: &PhotoQuery<'_>,
) -> Result<String, DbError> {
    let lines: Vec<(String,)> = search_query(
        "EXPLAIN ANALYZE ",
        owner_id,
        query,
        Order::Relevance { offset: 0 },
        50,
    )
    .build_query_as()
    .fetch_all(pool)
    .await?;
    Ok(lines
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n"))
}
