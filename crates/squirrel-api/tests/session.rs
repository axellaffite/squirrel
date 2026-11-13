mod common;

use axum::http::StatusCode;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_requires_a_session(pool: PgPool) {
    let server = common::server(pool);
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_returns_the_logged_in_user(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let fetched: serde_json::Value = server.get("/api/auth/me").await.json();
    assert_eq!(fetched["username"], "alice");
    assert_eq!(fetched["csrf_token"], me["csrf_token"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn stale_session_is_extended_on_use(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() - interval '2 hours', \
         expires_at = now() + interval '1 minute'",
    )
    .execute(&pool)
    .await
    .unwrap();

    server.get("/api/auth/me").await.assert_status_ok();
    let extended: bool =
        sqlx::query_scalar("SELECT expires_at > now() + interval '29 days' FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(extended);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn state_changing_requests_need_the_csrf_header(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;

    server
        .post("/api/auth/logout")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    server
        .post("/api/auth/logout")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
#[ignore = "needs login (Task 13)"]
async fn logout_all_revokes_other_browsers(pool: PgPool) {
    let first = common::server(pool.clone());
    let me = common::setup_admin(&first, "alice").await;

    let second = common::server(pool);
    second
        .post("/api/auth/login")
        .json(&serde_json::json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status_ok();

    first
        .post("/api/auth/logout-all")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    second
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
