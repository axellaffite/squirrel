mod common;

use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::maintenance;
use squirrel_core::ContentHash;
use time::Duration;
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_sessions_and_orphan_files_are_removed(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let expired: Value = files::start_upload(&browser, &me, parent, "old.bin", b"old")
        .await
        .json();
    let live: Value = files::start_upload(&browser, &me, parent, "new.bin", b"new")
        .await
        .json();
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(
        expired["upload_id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap(),
    )
    .execute(&pool)
    .await
    .unwrap();
    let orphan = Uuid::now_v7();
    instance.state.storage.create_upload(orphan, 1).unwrap();

    let cleanup = maintenance::purge_expired_uploads(&instance.state)
        .await
        .unwrap();
    assert_eq!((cleanup.expired_sessions, cleanup.files_removed), (1, 2));

    let uploads_dir = instance.data_dir().join("tmp/uploads");
    let left: Vec<String> = std::fs::read_dir(uploads_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, [live["upload_id"].as_str().unwrap()]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn old_trash_is_purged_and_unreferenced_blobs_collected(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        trash_retention: Duration::ZERO,
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let gone = files::upload(&browser, &me, parent, "gone.txt", b"gone").await;
    files::upload(&browser, &me, parent, "kept.txt", b"kept").await;
    browser
        .delete(&format!("/api/nodes/{}", gone["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);

    assert_eq!(
        maintenance::purge_old_trash(&instance.state).await.unwrap(),
        1
    );
    assert_eq!(
        maintenance::collect_garbage(&instance.state).await.unwrap(),
        1
    );

    let store = &instance.state.storage;
    assert!(!store.contains(&ContentHash::parse(&files::sha256_hex(b"gone")).unwrap()));
    assert!(store.contains(&ContentHash::parse(&files::sha256_hex(b"kept")).unwrap()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn gc_waits_for_the_grace_period(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"recent").await;
    let id = node["id"].as_str().unwrap();
    for path in [format!("/api/nodes/{id}"), format!("/api/trash/{id}")] {
        browser
            .delete(&path)
            .add_header(common::CSRF, common::csrf(&me))
            .await
            .assert_status(axum::http::StatusCode::NO_CONTENT);
    }
    assert_eq!(
        maintenance::collect_garbage(&instance.state).await.unwrap(),
        0
    );
    assert!(
        instance
            .state
            .storage
            .contains(&ContentHash::parse(&files::sha256_hex(b"recent")).unwrap())
    );
}
