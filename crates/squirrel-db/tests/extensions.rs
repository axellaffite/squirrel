//! The pgvector check that runs before migrating.

use sqlx::PgPool;
use sqlx::postgres::PgConnectOptions;
use squirrel_db::ExtensionError;

/// A superuser (the test role) can create the extension: the check passes.
#[sqlx::test(migrations = false)]
async fn a_superuser_passes_the_vector_check(pool: PgPool) {
    squirrel_db::check_vector_extension(&pool).await.unwrap();
}

/// A role that is not a superuser cannot `CREATE EXTENSION vector` (it is not
/// a trusted extension): the check names the statement a superuser must run.
#[sqlx::test(migrations = false)]
async fn a_plain_role_without_the_extension_is_told_what_to_run(pool: PgPool) {
    let role = format!("squirrel_plain_{}", std::process::id());
    sqlx::query(&format!("DROP ROLE IF EXISTS {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'plain' NOSUPERUSER"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let options: PgConnectOptions = pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password("plain");
    let plain = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let result = squirrel_db::check_vector_extension(&plain).await;
    plain.close().await;
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
    let err = result.unwrap_err();
    assert!(matches!(err, ExtensionError::VectorNeedsSuperuser), "{err}");
    assert!(
        err.to_string()
            .contains("pgvector must be installed by a superuser: CREATE EXTENSION vector;"),
        "{err}"
    );
}

/// Once a superuser created it, anyone passes.
#[sqlx::test(migrations = false)]
async fn an_installed_extension_passes_for_anyone(pool: PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(&pool)
        .await
        .unwrap();
    let role = format!("squirrel_plain2_{}", std::process::id());
    sqlx::query(&format!("DROP ROLE IF EXISTS {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'plain' NOSUPERUSER"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let options: PgConnectOptions = pool
        .connect_options()
        .as_ref()
        .clone()
        .username(&role)
        .password("plain");
    let plain = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let result = squirrel_db::check_vector_extension(&plain).await;
    plain.close().await;
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
    result.unwrap();
}
