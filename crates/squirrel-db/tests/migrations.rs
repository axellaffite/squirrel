use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_singleton_instance_settings(pool: PgPool) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
