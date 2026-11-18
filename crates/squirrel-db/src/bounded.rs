//! Read queries that scan a whole library (searches, Explore lists, month
//! counts) run under a statement timeout: past it they fail as
//! [`DbError::Busy`] (the API answers 503) instead of holding a connection.

use sqlx::{PgPool, Postgres, Transaction};

use crate::DbError;
use crate::documents::SEARCH_TIMEOUT;

/// A transaction whose statements are cut after [`SEARCH_TIMEOUT`].
pub(crate) async fn begin(pool: &PgPool) -> Result<Transaction<'static, Postgres>, DbError> {
    begin_with(pool, SEARCH_TIMEOUT).await
}

async fn begin_with(
    pool: &PgPool,
    timeout: &str,
) -> Result<Transaction<'static, Postgres>, DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query(&format!("SET LOCAL statement_timeout = '{timeout}'"))
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::timed_out_is_busy;

    #[sqlx::test(migrations = false)]
    async fn an_overrunning_statement_is_busy(pool: PgPool) {
        let mut tx = begin_with(&pool, "20ms").await.unwrap();
        let err = sqlx::query("SELECT pg_sleep(1)")
            .execute(&mut *tx)
            .await
            .map_err(timed_out_is_busy)
            .unwrap_err();
        assert!(matches!(err, DbError::Busy(_)), "{err}");
    }
}
