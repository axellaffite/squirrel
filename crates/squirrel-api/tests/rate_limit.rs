mod common;

use axum::http::{HeaderName, StatusCode};
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, RateLimit};

const XFF: HeaderName = HeaderName::from_static("x-forwarded-for");

fn one_per_minute() -> Option<RateLimit> {
    Some(RateLimit {
        replenish_every_secs: 60,
        burst: 1,
    })
}

fn server(pool: PgPool, trusted: &str) -> TestServer {
    let config = ApiConfig {
        login_rate_limit: one_per_minute(),
        trusted_proxies: trusted.parse().unwrap(),
        ..common::test_config()
    };
    common::server_with(pool, config)
}

async fn login_from(server: &TestServer, forwarded_for: &str) -> StatusCode {
    server
        .post("/api/auth/login")
        .add_header(
            XFF,
            forwarded_for.parse::<axum::http::HeaderValue>().unwrap(),
        )
        .json(&json!({ "username": "x", "password": "y" }))
        .await
        .status_code()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn forwarded_for_is_ignored_when_the_peer_is_not_trusted(pool: PgPool) {
    let server = server(pool, "");
    assert_eq!(
        login_from(&server, "1.1.1.1").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_from(&server, "2.2.2.2").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn forwarded_for_is_honored_when_the_peer_is_a_trusted_proxy(pool: PgPool) {
    let server = server(pool, "127.0.0.0/8, 10.0.0.1");
    assert_eq!(
        login_from(&server, "1.1.1.1").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_from(&server, "2.2.2.2, 10.0.0.1").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login_from(&server, "1.1.1.1").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // A client cannot escape its bucket by prepending a fake address.
    assert_eq!(
        login_from(&server, "9.9.9.9, 2.2.2.2").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rate_limited_response_is_the_error_envelope(pool: PgPool) {
    let server = server(pool, "");
    login_from(&server, "1.1.1.1").await;
    let response = server
        .post("/api/auth/login")
        .json(&json!({ "username": "x", "password": "y" }))
        .await;
    response.assert_status(StatusCode::TOO_MANY_REQUESTS);
    let body: Value = response.json();
    assert_eq!(body["error"]["code"], "rate_limited");
    assert!(response.maybe_header("retry-after").is_some());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invitation_checks_have_their_own_bucket(pool: PgPool) {
    let config = ApiConfig {
        login_rate_limit: one_per_minute(),
        join_check_rate_limit: Some(RateLimit {
            replenish_every_secs: 60,
            burst: 2,
        }),
        ..common::test_config()
    };
    let server = common::server_with(pool, config);
    server
        .post("/api/auth/login")
        .json(&json!({ "username": "x", "password": "y" }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    for _ in 0..2 {
        server.get("/api/join/some-token").await.assert_status_ok();
    }
    server
        .get("/api/join/some-token")
        .await
        .assert_status(StatusCode::TOO_MANY_REQUESTS);
}
