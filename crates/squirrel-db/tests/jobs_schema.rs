use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;

async fn insert(pool: &PgPool, kind: &str, key: Option<&str>) -> u64 {
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key) VALUES ($1, $2) \
         ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') \
         DO NOTHING",
    )
    .bind(kind)
    .bind(key)
    .execute(pool)
    .await
    .unwrap()
    .rows_affected()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn dedupe_keys_are_unique_while_queued_or_running(pool: PgPool) {
    assert_eq!(insert(&pool, "thumbnails", Some("thumbnails:a")).await, 1);
    assert_eq!(insert(&pool, "thumbnails", Some("thumbnails:a")).await, 0);
    assert_eq!(insert(&pool, "thumbnails", None).await, 1);
    assert_eq!(
        insert(&pool, "thumbnails", None).await,
        1,
        "no key, no dedupe"
    );
    sqlx::query("UPDATE jobs SET status = 'failed' WHERE dedupe_key = 'thumbnails:a'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        insert(&pool, "thumbnails", Some("thumbnails:a")).await,
        1,
        "failed jobs do not block"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn running_jobs_must_hold_a_lease_and_a_claim(pool: PgPool) {
    insert(&pool, "x", None).await;
    let err = sqlx::query("UPDATE jobs SET status = 'running'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("jobs_running_is_claimed"), "{err}");
    for bad in ["Bad-Kind", "", "x y"] {
        assert!(
            sqlx::query("INSERT INTO jobs (kind) VALUES ($1)")
                .bind(bad)
                .execute(&pool)
                .await
                .is_err(),
            "{bad}"
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn inserting_jobs_notifies_listeners(pool: PgPool) {
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("squirrel_jobs").await.unwrap();
    insert(&pool, "x", None).await;
    let note = tokio::time::timeout(Duration::from_secs(5), listener.recv())
        .await
        .expect("a notification")
        .unwrap();
    assert_eq!(note.channel(), "squirrel_jobs");
}
