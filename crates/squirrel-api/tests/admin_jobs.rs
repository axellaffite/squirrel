mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn insert_job(pool: &PgPool, kind: &str, failed: bool) -> i64 {
    let sql = if failed {
        "INSERT INTO jobs (kind, status, attempts, last_error) VALUES ($1, 'failed', 5, 'boom') RETURNING id"
    } else {
        "INSERT INTO jobs (kind) VALUES ($1) RETURNING id"
    };
    sqlx::query_scalar(sql)
        .bind(kind)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn status_of(pool: &PgPool, id: i64) -> (String, i32) {
    sqlx::query_as("SELECT status, attempts FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admins_see_counts_per_kind_and_recent_failures(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, _), (bob, _)) = common::two_users(&instance).await;
    insert_job(&pool, "thumbnails", false).await;
    let failed = insert_job(&pool, "geocode", true).await;

    let overview: Value = alice.get("/api/admin/jobs").await.json();
    assert_eq!(
        overview["kinds"],
        json!([
            {"kind": "geocode", "queued": 0, "running": 0, "failed": 1},
            {"kind": "thumbnails", "queued": 1, "running": 0, "failed": 0}
        ])
    );
    let failures = overview["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["id"], failed);
    assert_eq!(failures[0]["kind"], "geocode");
    assert_eq!(failures[0]["attempts"], 5);
    assert_eq!(failures[0]["last_error"], "boom");
    assert!(failures[0]["updated_at"].is_string());

    let denied = bob.get("/api/admin/jobs").await;
    denied.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(denied.json::<Value>()["error"]["code"], "forbidden");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admins_retry_failed_jobs(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let failed = insert_job(&pool, "geocode", true).await;
    let path = format!("/api/admin/jobs/{failed}/retry");

    let no_csrf = alice.post(&path).await;
    no_csrf.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(no_csrf.json::<Value>()["error"]["code"], "csrf_invalid");

    bob.post(&path)
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    assert_eq!(status_of(&pool, failed).await.0, "failed");

    alice
        .post(&path)
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(status_of(&pool, failed).await, ("queued".into(), 0));

    let again = alice
        .post(&path)
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await;
    again.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(again.json::<Value>()["error"]["code"], "not_found");
    alice
        .post("/api/admin/jobs/999999/retry")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
