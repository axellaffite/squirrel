mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;

/// `(kind, priority, payload)` of every job.
async fn jobs(pool: &PgPool) -> Vec<(String, i16, Value)> {
    sqlx::query_as("SELECT kind, priority, payload FROM jobs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00 a jpeg as far as sniffing goes";

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn uploads_and_dedup_links_of_media_queue_their_processing(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = files::root_id(&instance, &me, "Photos").await;
    let docs = files::root_id(&instance, &me, "Documents").await;

    let node = files::upload(&browser, &me, photos, "photo.jpg", JPEG).await;
    assert_eq!(
        jobs(&pool).await,
        vec![(
            "process_media".to_owned(),
            100,
            json!({"node_id": node["id"], "background": false})
        )]
    );

    let linked = files::start_upload(&browser, &me, docs, "copy.jpg", JPEG).await;
    linked.assert_status(StatusCode::OK);
    let copy = linked.json::<Value>()["node"].clone();
    let queued = jobs(&pool).await;
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[1].2["node_id"], copy["id"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn failed_completions_and_other_files_queue_nothing(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;

    let started: Value = files::start_upload(&browser, &me, parent, "bad.jpg", JPEG)
        .await
        .json();
    let id = started["upload_id"].as_str().unwrap();
    let mut wrong = JPEG.to_vec();
    wrong[20] ^= 1;
    files::put_chunk(&browser, &me, id, 0, &wrong)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::complete(&browser, &me, id)
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    files::upload(&browser, &me, parent, "a.zip", b"PK\x03\x04\x14\x00 a zip").await;
    assert!(jobs(&pool).await.is_empty());
}
