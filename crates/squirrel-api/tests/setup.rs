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
        .json(&common::account("Alice"))
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
        .json(&common::account("mallory"))
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
            &json!({ "username": "al", "display_name": "Al", "password": "correct horse battery" }),
        )
        .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<serde_json::Value>()["error"]["message"],
        "username must be between 3 and 32 characters"
    );
}
