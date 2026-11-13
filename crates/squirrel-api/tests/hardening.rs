mod common;

use std::time::Duration;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::ApiConfig;

const CSP: &str = "frame-ancestors 'none'; base-uri 'none'; object-src 'none'";

fn header(response: &TestResponse, name: &str) -> Option<String> {
    response
        .maybe_header(name)
        .map(|v| v.to_str().unwrap().to_owned())
}

fn assert_security_headers(response: &TestResponse) {
    assert_eq!(
        header(response, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(header(response, "x-frame-options").as_deref(), Some("DENY"));
    assert_eq!(
        header(response, "referrer-policy").as_deref(),
        Some("no-referrer")
    );
    assert_eq!(
        header(response, "content-security-policy").as_deref(),
        Some(CSP)
    );
}

fn secure_server(pool: PgPool) -> TestServer {
    let config = ApiConfig {
        cookie_secure: true,
        ..common::test_config()
    };
    common::server_with(pool, config)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn api_responses_carry_security_headers_and_no_store(pool: PgPool) {
    let server = common::server(pool);
    for response in [
        server.get("/api/healthz").await,
        server.get("/api/does-not-exist").await,
        server.get("/api/auth/me").await,
    ] {
        assert_security_headers(&response);
        assert_eq!(
            header(&response, "cache-control").as_deref(),
            Some("no-store")
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn spa_responses_carry_security_headers_but_may_be_cached(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/login").await;
    assert_security_headers(&response);
    assert_ne!(
        header(&response, "cache-control").as_deref(),
        Some("no-store")
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn hsts_is_sent_only_with_secure_cookies(pool: PgPool) {
    let plain = common::server(pool.clone()).get("/api/healthz").await;
    assert_eq!(header(&plain, "strict-transport-security"), None);

    let secure = secure_server(pool).get("/api/healthz").await;
    assert_eq!(
        header(&secure, "strict-transport-security").as_deref(),
        Some("max-age=31536000")
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn credential_bodies_are_limited_to_16_kib(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/auth/login")
        .json(&json!({ "username": "alice", "password": "x".repeat(17 * 1024) }))
        .await;
    response.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response.json::<Value>()["error"]["code"],
        "payload_too_large"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_api_bodies_are_limited_to_64_kib(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let invite = |size: usize| {
        server
            .post("/api/invitations")
            .add_header(common::CSRF, common::csrf(&me))
            .json(&json!({ "email_hint": "a", "padding": "x".repeat(size) }))
    };
    invite(32 * 1024).await.assert_status(StatusCode::CREATED);
    invite(65 * 1024)
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn password_hashing_answers_busy_when_no_slot_frees_up(pool: PgPool) {
    let config = ApiConfig {
        hash_concurrency: 0,
        hash_wait: Duration::from_millis(50),
        ..common::test_config()
    };
    let server = common::server_with(pool, config);
    let response = server
        .post("/api/setup/admin")
        .json(&common::setup_request("alice"))
        .await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.json::<Value>()["error"]["code"], "busy");

    server
        .post("/api/auth/login")
        .json(&json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE);
}
