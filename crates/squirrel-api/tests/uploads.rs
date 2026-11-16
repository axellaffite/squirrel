mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_core::Username;
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creating_an_upload_opens_a_session_and_a_sparse_file(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;

    let response = files::start_upload(&browser, &me, parent, "big.bin", &[7; 2500]).await;
    response.assert_status(StatusCode::CREATED);
    let session: Value = response.json();
    assert_eq!(session["chunk_size"], common::TEST_CHUNK_SIZE);
    assert_eq!(session["chunk_count"], 3);
    assert_eq!(session["received_chunks"], json!([]));
    let file = instance
        .data_dir()
        .join("tmp/uploads")
        .join(session["upload_id"].as_str().unwrap());
    assert_eq!(std::fs::metadata(file).unwrap().len(), 2500);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_same_file_resumes_the_same_session(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let first: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let second: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    assert!(first["upload_id"].is_string());
    assert_eq!(first["upload_id"], second["upload_id"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_requests_are_rejected(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;

    let bad_name = files::start_upload(&browser, &me, parent, "a/b", b"x").await;
    bad_name.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let bad_hash = browser
        .post("/api/uploads")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&json!({ "parent_id": parent, "name": "a", "size": 1, "sha256": "ABC" }))
        .await;
    bad_hash.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let unknown_parent = files::start_upload(&browser, &me, Uuid::now_v7(), "a", b"x").await;
    unknown_parent.assert_status(StatusCode::NOT_FOUND);
    let no_csrf = browser
        .post("/api/uploads")
        .json(&json!({ "parent_id": parent, "name": "a", "size": 1, "sha256": files::sha256_hex(b"x") }))
        .await;
    no_csrf.assert_status(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn quota_is_checked_before_any_byte_is_sent(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100))
        .await
        .unwrap();
    let response = files::start_upload(&browser, &me, parent, "big.bin", &[0; 101]).await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&response), "quota_exceeded");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn uploads_that_would_fill_the_disk_are_refused(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        min_free_bytes: 1024,
        ..common::test_config()
    };
    // 1026 bytes free: a 3-byte upload would leave less than 1024.
    let instance = common::Instance::with_config(pool, config).with_free_space(1026);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let response = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&response), "storage_full");

    // One byte more and it fits exactly.
    let roomier = instance
        .sharing(squirrel_api::ApiConfig {
            min_free_bytes: 1024,
            ..common::test_config()
        })
        .with_free_space(1027);
    let other = roomier.browser();
    let me = common::login(&other, "alice").await;
    files::start_upload(&other, &me, parent, "a.bin", b"abc")
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn status_and_abort(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap();

    let status: Value = browser.get(&format!("/api/uploads/{id}")).await.json();
    assert_eq!(status["upload_id"], id);
    browser
        .delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    browser
        .get(&format!("/api/uploads/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    assert!(!instance.data_dir().join("tmp/uploads").join(id).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn uploads_and_folders_are_private(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alices_files = files::root_id(&instance, &alice_me, "Files").await;

    files::start_upload(&bob, &bob_me, alices_files, "a.bin", b"abc")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let session: Value = files::start_upload(&alice, &alice_me, alices_files, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap();
    bob.get(&format!("/api/uploads/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    bob.delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
