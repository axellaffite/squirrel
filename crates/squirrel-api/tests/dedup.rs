mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::ContentHash;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn known_content_is_linked_without_uploading(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let files_root = files::root_id(&instance, &me, "Files").await;
    let docs = files::root_id(&instance, &me, "Documents").await;
    let content = vec![9u8; 3000];
    files::upload(&browser, &me, files_root, "a.bin", &content).await;

    let again = files::start_upload(&browser, &me, docs, "copy.bin", &content).await;
    again.assert_status(StatusCode::OK);
    let body: Value = again.json();
    assert_eq!(body["deduplicated"], true);
    assert_eq!(
        (body["node"]["name"].as_str(), body["node"]["size"].as_u64()),
        (Some("copy.bin"), Some(3000))
    );

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    let blob = squirrel_db::blobs::find(&pool, &hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(blob.ref_count, 2);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_users_content_is_never_revealed_but_still_shared_on_disk(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let content = b"the same holiday photo".to_vec();
    let alice_files = files::root_id(&instance, &alice_me, "Files").await;
    let bob_files = files::root_id(&instance, &bob_me, "Files").await;
    files::upload(&alice, &alice_me, alice_files, "photo.jpg", &content).await;

    // Bob must upload the bytes: the probe answers exactly as for unknown content.
    let probe = files::start_upload(&bob, &bob_me, bob_files, "photo.jpg", &content).await;
    probe.assert_status(StatusCode::CREATED);
    files::upload(&bob, &bob_me, bob_files, "photo.jpg", &content).await;

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    assert_eq!(
        squirrel_db::blobs::find(&pool, &hash)
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        2
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn dedup_counts_against_the_quota(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "a.bin", &[1; 60]).await;
    squirrel_db::users::set_quota(
        &pool,
        &squirrel_core::Username::parse("alice").unwrap(),
        Some(100),
    )
    .await
    .unwrap();
    let response = files::start_upload(&browser, &me, parent, "b.bin", &[1; 60]).await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
}
