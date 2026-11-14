use sqlx::PgPool;
use squirrel_db::instance;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn every_database_gets_one_stable_installation_id(pool: PgPool) {
    let first = instance::installation_id(&pool).await.unwrap();
    assert_eq!(instance::installation_id(&pool).await.unwrap(), first);
    assert!(!first.is_nil());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}
