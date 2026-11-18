mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::Username;

async fn delete(browser: &TestServer, me: &Value, path: &str) -> axum_test::TestResponse {
    browser
        .delete(path)
        .add_header(common::CSRF, common::csrf(me))
        .await
}

async fn post(browser: &TestServer, me: &Value, path: &str) -> axum_test::TestResponse {
    browser
        .post(path)
        .add_header(common::CSRF, common::csrf(me))
        .await
}

fn ids(listing: &Value) -> Vec<String> {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["node"]["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_moves_to_trash_and_restore_brings_back(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"hello").await;
    let id = node["id"].as_str().unwrap();

    delete(&browser, &me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    browser
        .get(&format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let trash: Value = browser.get("/api/trash").await.json();
    assert_eq!(ids(&trash), [id]);
    assert!(trash["items"][0]["deleted_at"].is_string());

    let restored: Value = post(&browser, &me, &format!("/api/trash/{id}/restore"))
        .await
        .json();
    assert_eq!(restored["name"], "a.txt");
    browser
        .get(&format!("/api/nodes/{id}"))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_listing_paginates(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let mut deleted = Vec::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        let node = files::upload(&browser, &me, parent, name, name.as_bytes()).await;
        let id = node["id"].as_str().unwrap().to_owned();
        delete(&browser, &me, &format!("/api/nodes/{id}"))
            .await
            .assert_status(StatusCode::NO_CONTENT);
        deleted.push(id);
    }
    let first: Value = browser.get("/api/trash?limit=2").await.json();
    let cursor = first["next_cursor"].as_str().unwrap();
    let second: Value = browser
        .get(&format!("/api/trash?limit=2&cursor={cursor}"))
        .await
        .json();
    assert_eq!(second["next_cursor"], Value::Null);
    let listed: Vec<String> = ids(&first).into_iter().chain(ids(&second)).collect();
    deleted.reverse();
    assert_eq!(listed, deleted);
    browser
        .get("/api/trash?cursor=garbage!")
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_renames_when_the_name_was_reused(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let old = files::upload(&browser, &me, parent, "a.txt", b"old").await;
    let id = old["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::upload(&browser, &me, parent, "a.txt", b"new").await;

    let restored: Value = post(&browser, &me, &format!("/api/trash/{id}/restore"))
        .await
        .json();
    assert_eq!(restored["name"], "a (1).txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_forever_and_empty(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let a = files::upload(&browser, &me, parent, "a.txt", b"a").await;
    let b = files::upload(&browser, &me, parent, "b.txt", b"b").await;
    for node in [&a, &b] {
        let id = node["id"].as_str().unwrap();
        delete(&browser, &me, &format!("/api/nodes/{id}"))
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let a_id = a["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/trash/{a_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    delete(&browser, &me, &format!("/api/trash/{a_id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    delete(&browser, &me, "/api/trash")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let trash: Value = browser.get("/api/trash").await.json();
    assert!(ids(&trash).is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn top_level_folders_cannot_be_deleted(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    delete(&browser, &me, &format!("/api/nodes/{parent}"))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_over_quota_is_refused(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let old = files::upload(&browser, &me, parent, "old.bin", &[1; 60]).await;
    let id = old["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::upload(&browser, &me, parent, "new.bin", &[2; 60]).await;
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100))
        .await
        .unwrap();
    let refused = post(&browser, &me, &format!("/api/trash/{id}/restore")).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&refused), "quota_exceeded");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_is_private(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let parent = files::root_id(&instance, &alice_me, "Files").await;
    let node = files::upload(&alice, &alice_me, parent, "a.txt", b"a").await;
    let id = node["id"].as_str().unwrap();
    delete(&bob, &bob_me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    delete(&alice, &alice_me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let bobs: Value = bob.get("/api/trash").await.json();
    assert!(ids(&bobs).is_empty());
    post(&bob, &bob_me, &format!("/api/trash/{id}/restore"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    delete(&bob, &bob_me, &format!("/api/trash/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_restored_photo_without_a_face_scan_is_queued_for_faces(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Photos").await;
    let node = files::upload(&browser, &me, parent, "a.jpg", b"not really a jpeg").await;
    let id = node["id"].as_str().unwrap();
    // Rendered and analysed while the photo sat in the trash: no scan for Alice.
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, thumbs_state, thumbs_version) \
         SELECT blob_sha256, 'image', 'ready', 'ready', 1 FROM nodes WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         SELECT id, owner_id, blob_sha256, 'photo', now(), 'upload' FROM nodes WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    delete(&browser, &me, &format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    sqlx::query("DELETE FROM jobs")
        .execute(&pool)
        .await
        .unwrap();

    post(&browser, &me, &format!("/api/trash/{id}/restore"))
        .await
        .assert_status_ok();
    let queued: Vec<(String, bool)> = sqlx::query_as(
        "SELECT j.kind, j.payload->>'sha256' = n.blob_sha256 FROM jobs j, nodes n \
          WHERE n.id = $1::uuid ORDER BY j.id",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(queued, [("faces".to_owned(), true)]);
}
