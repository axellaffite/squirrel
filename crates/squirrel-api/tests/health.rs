mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn healthz_reports_ok(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/api/healthz").await;
    response.assert_status_ok();
    response.assert_json(&json!({ "status": "ok" }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn unknown_api_route_returns_json_404(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/api/does-not-exist").await;
    response.assert_status(StatusCode::NOT_FOUND);
    response.assert_json(&json!({ "error": { "code": "not_found", "message": "not found" } }));
}
