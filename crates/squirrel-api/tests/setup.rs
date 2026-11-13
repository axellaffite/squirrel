mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn fresh_instance_needs_setup(pool: PgPool) {
    let server = common::server(pool);
    server
        .get("/api/setup/status")
        .await
        .assert_json(&json!({ "needs_setup": true }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn first_account_is_admin_and_logged_in(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/setup/admin")
        .json(&common::setup_request("Alice"))
        .await;
    response.assert_status(StatusCode::CREATED);
    assert!(
        response
            .cookie("squirrel_session")
            .http_only()
            .unwrap_or(false)
    );

    let me: serde_json::Value = response.json();
    assert_eq!(me["username"], "alice");
    assert_eq!(me["role"], "admin");
    assert!(me["csrf_token"].as_str().unwrap().len() >= 43);
    server
        .get("/api/setup/status")
        .await
        .assert_json(&json!({ "needs_setup": false }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn second_setup_is_rejected(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;

    let other_browser = common::server(pool);
    let response = other_browser
        .post("/api/setup/admin")
        .json(&common::setup_request("mallory"))
        .await;
    response.assert_status(StatusCode::CONFLICT);
    assert_eq!(
        response.json::<serde_json::Value>()["error"]["code"],
        "conflict"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_input_is_a_422_with_message(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/setup/admin")
        .json(
            &json!({ "setup_token": common::SETUP_TOKEN, "username": "al",
                     "display_name": "Al", "password": "correct horse battery" }),
        )
        .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<serde_json::Value>()["error"]["message"],
        "username must be between 3 and 32 characters"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn setup_without_the_setup_token_is_forbidden(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/setup/admin")
        .json(&common::account("mallory"))
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(
        response.json::<serde_json::Value>()["error"]["code"],
        "forbidden"
    );
    server
        .get("/api/setup/status")
        .await
        .assert_json(&json!({ "needs_setup": true }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn setup_with_a_wrong_setup_token_is_forbidden(pool: PgPool) {
    let server = common::server(pool);
    let mut request = common::setup_request("mallory");
    request["setup_token"] = json!("not-the-setup-token");
    server
        .post("/api/setup/admin")
        .json(&request)
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn setup_is_forbidden_when_no_token_is_configured(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        setup_token: None,
        ..common::test_config()
    };
    let server = common::server_with(pool, config);
    server
        .post("/api/setup/admin")
        .json(&common::setup_request("alice"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completed_setup_reports_conflict_even_with_a_wrong_token(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    let mut request = common::setup_request("mallory");
    request["setup_token"] = json!("wrong");
    common::server(pool)
        .post("/api/setup/admin")
        .json(&request)
        .await
        .assert_status(StatusCode::CONFLICT);
}
