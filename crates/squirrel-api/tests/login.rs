mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, RateLimit};

fn creds(username: &str, password: &str) -> Value {
    json!({ "username": username, "password": password })
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn login_succeeds_with_correct_password(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool);
    let me: Value = browser
        .post("/api/auth/login")
        .json(&creds("Alice", common::PASSWORD))
        .await
        .json();
    assert_eq!(me["username"], "alice");
    browser.get("/api/auth/me").await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn wrong_password_and_unknown_user_look_identical(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool);

    let wrong = browser
        .post("/api/auth/login")
        .json(&creds("alice", "wrong password!"))
        .await;
    let unknown = browser
        .post("/api/auth/login")
        .json(&creds("nobody", "wrong password!"))
        .await;
    wrong.assert_status(StatusCode::UNAUTHORIZED);
    unknown.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.text(), unknown.text());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn account_locks_after_repeated_failures(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool.clone());
    for _ in 0..5 {
        browser
            .post("/api/auth/login")
            .json(&creds("alice", "wrong password!"))
            .await;
    }
    browser
        .post("/api/auth/login")
        .json(&creds("alice", common::PASSWORD))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    sqlx::query("UPDATE users SET locked_until = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    browser
        .post("/api/auth/login")
        .json(&creds("alice", common::PASSWORD))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn credential_endpoints_are_rate_limited_per_ip(pool: PgPool) {
    let config = ApiConfig {
        login_rate_limit: Some(RateLimit {
            replenish_every_secs: 60,
            burst: 3,
        }),
        ..common::test_config()
    };
    let browser = common::server_with(pool, config);
    for _ in 0..3 {
        browser
            .post("/api/auth/login")
            .json(&creds("x", "y"))
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
    browser
        .post("/api/auth/login")
        .json(&creds("x", "y"))
        .await
        .assert_status(StatusCode::TOO_MANY_REQUESTS);
}
