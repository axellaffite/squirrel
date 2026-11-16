//! Phones: device sign-in and bearer requests.

use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::{TestResponse, TestServer};
use serde_json::{Value, json};

use super::{Instance, PASSWORD};

pub fn device_body(username: &str, name: &str) -> Value {
    json!({
        "username": username,
        "password": PASSWORD,
        "device": { "name": name, "platform": "android", "model": "Pixel 8", "app_version": "0.1.0" },
    })
}

pub async fn try_device_login(phone: &TestServer, username: &str, name: &str) -> TestResponse {
    phone
        .post("/api/auth/device/login")
        .json(&device_body(username, name))
        .await
}

/// Signs `username` in as a new device named `name`; returns the login body
/// (`device`, `account`, `tokens`).
pub async fn device_login(instance: &Instance, username: &str, name: &str) -> Value {
    let response = try_device_login(&instance.browser(), username, name).await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}

pub fn access(login: &Value) -> String {
    login["tokens"]["access_token"]
        .as_str()
        .expect("access token")
        .to_owned()
}

pub fn refresh_token(login: &Value) -> String {
    login["tokens"]["refresh_token"]
        .as_str()
        .expect("refresh token")
        .to_owned()
}

pub fn device_id(login: &Value) -> String {
    login["device"]["id"]
        .as_str()
        .expect("device id")
        .to_owned()
}

pub fn bearer(token: &str) -> (HeaderName, HeaderValue) {
    (
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).expect("header value"),
    )
}

pub async fn refresh(phone: &TestServer, token: &str) -> TestResponse {
    phone
        .post("/api/auth/device/refresh")
        .json(&json!({ "refresh_token": token }))
        .await
}

/// A cookie-less client sending `token` as a bearer on `GET /api/nodes`.
pub async fn list_roots(instance: &Instance, token: &str) -> TestResponse {
    let (name, value) = bearer(token);
    instance
        .browser()
        .get("/api/nodes")
        .add_header(name, value)
        .await
}

/// Whether `token` is a live access token, read from the database.
pub async fn access_is_live(pool: &sqlx::PgPool, token: &str) -> bool {
    squirrel_db::devices::find_by_access_hash(pool, &squirrel_auth::hash_token(token))
        .await
        .expect("lookup")
        .is_some()
}

/// `POST /api/sync/check` with `items` as `(device_asset_id, sha256)` pairs.
pub async fn sync_check(instance: &Instance, token: &str, items: &[(&str, &str)]) -> TestResponse {
    let items: Vec<Value> = items
        .iter()
        .map(|(id, sha)| json!({ "device_asset_id": id, "sha256": sha }))
        .collect();
    let (name, value) = bearer(token);
    instance
        .browser()
        .post("/api/sync/check")
        .add_header(name, value)
        .json(&json!({ "items": items }))
        .await
}
