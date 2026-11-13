#![allow(dead_code)]

use std::net::SocketAddr;

use axum::Extension;
use axum::extract::ConnectInfo;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, AppState, SetupToken};

pub const PASSWORD: &str = "correct horse battery";
pub const CSRF: HeaderName = HeaderName::from_static("x-csrf-token");
pub const SETUP_TOKEN: &str = "test-setup-token";

pub fn test_config() -> ApiConfig {
    ApiConfig {
        cookie_secure: false,
        login_rate_limit: None,
        setup_token: Some(SetupToken::new(SETUP_TOKEN)),
        ..ApiConfig::default()
    }
}

pub fn server(pool: PgPool) -> TestServer {
    server_with(pool, test_config())
}

/// Each TestServer has its own cookie jar, i.e. acts as one browser.
/// A real `ConnectInfo` extension (not `MockConnectInfo`) is inserted, because
/// tower_governor's peer-IP key extractor only reads `ConnectInfo<SocketAddr>`.
pub fn server_with(pool: PgPool, config: ApiConfig) -> TestServer {
    let app = squirrel_api::router(AppState::new(pool, config)).layer(Extension(ConnectInfo(
        SocketAddr::from(([127, 0, 0, 1], 40_000)),
    )));
    TestServer::builder()
        .save_cookies()
        .build(app)
        .expect("test server builds")
}

pub fn account(username: &str) -> Value {
    json!({ "username": username, "display_name": username, "password": PASSWORD })
}

pub fn setup_request(username: &str) -> Value {
    json!({
        "setup_token": SETUP_TOKEN,
        "username": username,
        "display_name": username,
        "password": PASSWORD,
    })
}

pub fn csrf(me: &Value) -> HeaderValue {
    HeaderValue::from_str(me["csrf_token"].as_str().expect("csrf_token present")).unwrap()
}

pub async fn setup_admin(server: &TestServer, username: &str) -> Value {
    let response = server
        .post("/api/setup/admin")
        .json(&setup_request(username))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}
