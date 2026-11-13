mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_core::Username;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn usage_counts_live_files_and_trash(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "a.bin", &[1; 100]).await;
    let b = files::upload(&browser, &me, parent, "b.bin", &[2; 30]).await;
    browser
        .delete(&format!("/api/nodes/{}", b["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(1000))
        .await
        .unwrap();

    let usage: Value = browser.get("/api/storage").await.json();
    assert_eq!(
        usage,
        json!({ "used_bytes": 100, "trash_bytes": 30, "quota_bytes": 1000 })
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn usage_requires_a_session(pool: PgPool) {
    let browser = common::server(pool);
    browser
        .get("/api/storage")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
