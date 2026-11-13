mod common;

use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::maintenance;
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
