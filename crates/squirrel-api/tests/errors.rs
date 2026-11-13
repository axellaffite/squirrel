mod common;

use axum::http::{StatusCode, header};
use axum_test::TestResponse;
use serde_json::{Value, json};
use sqlx::PgPool;

fn assert_envelope(response: &TestResponse, status: StatusCode, code: &str) {
    response.assert_status(status);
    let body: Value = response.json();
    assert_eq!(body["error"]["code"], code, "{body}");
    assert!(body["error"]["message"].is_string(), "{body}");
}

/// Pins the login-CSRF defense: a cross-site form can only send
/// `text/plain`/form bodies, which must never reach the handler.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn non_json_login_body_is_rejected_with_415(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/auth/login")
        .text(r#"{"username":"alice","password":"correct horse battery"}"#)
        .content_type("text/plain")
        .await;
    assert_envelope(
        &response,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn malformed_json_is_a_400_envelope(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/auth/login")
        .text("{not json")
        .content_type("application/json")
        .await;
    assert_envelope(&response, StatusCode::BAD_REQUEST, "bad_request");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_json_field_is_a_422_envelope(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/auth/login")
        .json(&json!({ "username": "alice" }))
        .await;
    assert_envelope(
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_path_parameter_is_a_400_envelope(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let response = server
        .delete("/api/invitations/not-a-uuid")
        .add_header(common::CSRF, common::csrf(&me))
        .await;
    assert_envelope(&response, StatusCode::BAD_REQUEST, "bad_request");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn unknown_method_on_api_route_is_a_405_envelope(pool: PgPool) {
    let server = common::server(pool);
    let response = server.delete("/api/healthz").await;
    assert_envelope(
        &response,
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn non_get_request_outside_api_is_not_the_spa(pool: PgPool) {
    let server = common::server(pool);
    let response = server.post("/foo").await;
    assert_envelope(
        &response,
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
    );
    assert!(!response.text().contains("<html"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_asset_is_a_404_not_index_html(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/_app/immutable/missing.js").await;
    response.assert_status(StatusCode::NOT_FOUND);
    let content_type = response
        .maybe_header(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    assert!(!content_type.starts_with("text/html"), "{content_type}");
}
