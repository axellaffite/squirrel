mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn session(
    instance: &common::Instance,
    browser: &axum_test::TestServer,
    me: &Value,
    size: usize,
) -> String {
    let parent = files::root_id(instance, me, "Files").await;
    let content = vec![1u8; size];
    let started: Value = files::start_upload(browser, me, parent, "big.bin", &content)
        .await
        .json();
    started["upload_id"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_arrive_in_any_order_and_repeat_safely(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 2500).await;

    for (index, len) in [(2, 452), (0, 1024), (0, 1024), (1, 1024)] {
        files::put_chunk(&browser, &me, &id, index, &vec![1u8; len])
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let status: Value = browser.get(&format!("/api/uploads/{id}")).await.json();
    assert_eq!(status["received_chunks"], json!([0, 1, 2]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunk_sizes_and_indexes_are_validated(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 2500).await;

    let short = files::put_chunk(&browser, &me, &id, 0, &[1; 1000]).await;
    short.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let past_end = files::put_chunk(&browser, &me, &id, 3, &[1; 10]).await;
    past_end.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let huge = files::put_chunk(&browser, &me, &id, 0, &vec![1; 200 * 1024]).await;
    huge.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(files::code(&huge), "payload_too_large");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_need_csrf_and_the_owners_session(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let id = session(&instance, &alice, &alice_me, 10).await;

    alice
        .put(&format!("/api/uploads/{id}/chunks/0"))
        .bytes(vec![1u8; 10].into())
        .await
        .assert_status(StatusCode::FORBIDDEN);
    files::put_chunk(&bob, &bob_me, &id, 0, &[1; 10])
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_for_an_aborted_upload_are_not_found(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 10).await;
    browser
        .delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::put_chunk(&browser, &me, &id, 0, &[1; 10])
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
