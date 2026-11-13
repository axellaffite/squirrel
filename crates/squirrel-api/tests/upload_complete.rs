mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::{ContentHash, Username};
use time::{Duration, OffsetDateTime};

async fn alice(instance: &common::Instance) -> (axum_test::TestServer, Value, uuid::Uuid) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(instance, &me, "Files").await;
    (browser, me, parent)
}

async fn started(
    browser: &axum_test::TestServer,
    me: &Value,
    parent: uuid::Uuid,
    content: &[u8],
) -> String {
    let session: Value = files::start_upload(browser, me, parent, "a.bin", content)
        .await
        .json();
    session["upload_id"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_stores_the_blob_and_creates_the_node(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let content: Vec<u8> = (0..2500u32).map(|i| (i % 251) as u8).collect();

    let node = files::upload(&browser, &me, parent, "data.bin", &content).await;
    assert_eq!(
        (node["name"].as_str(), node["size"].as_u64()),
        (Some("data.bin"), Some(2500))
    );
    assert_eq!(node["kind"], "file");

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    assert_eq!(
        std::fs::read(instance.state.storage.blob_path(&hash)).unwrap(),
        content
    );
    let leftovers = std::fs::read_dir(instance.data_dir().join("tmp/uploads"))
        .unwrap()
        .count();
    assert_eq!(leftovers, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn empty_files_complete_without_chunks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let node = files::upload(&browser, &me, parent, "empty.txt", b"").await;
    assert_eq!(node["size"], 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn mime_is_sniffed_from_content_not_the_name(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let pdf = files::upload(&browser, &me, parent, "notes.txt", b"%PDF-1.7\n%...").await;
    assert_eq!(pdf["mime"], "application/pdf");
    let text = files::upload(&browser, &me, parent, "readme.txt", b"hello").await;
    assert_eq!(text["mime"], "text/plain");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_chunks_are_reported_and_the_session_survives(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let content = vec![3u8; 1500];
    let id = started(&browser, &me, parent, &content).await;
    files::put_chunk(&browser, &me, &id, 0, &content[..1024])
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let early = files::complete(&browser, &me, &id).await;
    early.assert_status(StatusCode::CONFLICT);
    assert_eq!(files::code(&early), "upload_incomplete");
    files::put_chunk(&browser, &me, &id, 1, &content[1024..])
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_hash_mismatch_discards_the_upload(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let id = started(&browser, &me, parent, b"abc").await;
    files::put_chunk(&browser, &me, &id, 0, b"abd")
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let response = files::complete(&browser, &me, &id).await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(files::code(&response), "hash_mismatch");
    browser
        .get(&format!("/api/uploads/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    assert!(!instance.data_dir().join("tmp/uploads").join(&id).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_vanished_upload_file_discards_the_session(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let id = started(&browser, &me, parent, b"abc").await;
    files::put_chunk(&browser, &me, &id, 0, b"abc")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    std::fs::remove_file(instance.data_dir().join("tmp/uploads").join(&id)).unwrap();

    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    browser
        .get(&format!("/api/uploads/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn name_clashes_are_renamed(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    files::upload(&browser, &me, parent, "report.pdf", b"one").await;
    let second = files::upload(&browser, &me, parent, "report.pdf", b"two").await;
    assert_eq!(second["name"], "report (1).pdf");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_quota_is_enforced_again_at_completion(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let alice_name = Username::parse("alice").unwrap();
    let id = started(&browser, &me, parent, &[1; 100]).await;
    files::put_chunk(&browser, &me, &id, 0, &[1; 100])
        .await
        .assert_status(StatusCode::NO_CONTENT);

    squirrel_db::users::set_quota(&pool, &alice_name, Some(50))
        .await
        .unwrap();
    let refused = files::complete(&browser, &me, &id).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&refused), "quota_exceeded");

    squirrel_db::users::set_quota(&pool, &alice_name, None)
        .await
        .unwrap();
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_completion_is_refused(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let id = started(&browser, &me, parent, b"abc").await;
    files::put_chunk(&browser, &me, &id, 0, b"abc")
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // Another request holds the session.
    let later = OffsetDateTime::now_utc() + Duration::hours(1);
    squirrel_db::uploads::claim(&pool, files::user_id(&me), id.parse().unwrap(), later)
        .await
        .unwrap();
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_into_a_trashed_folder_fails(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let folder = squirrel_db::nodes::create_folder(
        &pool,
        files::user_id(&me),
        parent,
        &squirrel_core::NodeName::parse("dir").unwrap(),
    )
    .await
    .unwrap();
    let id = started(&browser, &me, folder.id, b"abc").await;
    files::put_chunk(&browser, &me, &id, 0, b"abc")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    squirrel_db::trash::trash(&pool, files::user_id(&me), folder.id)
        .await
        .unwrap();
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn content_stored_before_keeps_the_blobs_mime(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let bob_root = files::root_id(&instance, &bob_me, "Files").await;
    let alice_root = files::root_id(&instance, &alice_me, "Files").await;
    let first = files::upload(&bob, &bob_me, bob_root, "notes.txt", b"plain words").await;
    assert_eq!(first["mime"], "text/plain");

    // Alice's name alone would give application/octet-stream; the shared blob
    // already has a type, and every node of a blob reports that one.
    let second = files::upload(&alice, &alice_me, alice_root, "notes", b"plain words").await;
    assert_eq!(second["mime"], "text/plain");
}
