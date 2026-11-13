//! Completion survives a dropped request, resumes find sessions being
//! completed, and chunk writes cannot slip between verification and storage.

mod common;

use std::time::Duration as StdDuration;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use squirrel_api::ApiConfig;
use squirrel_db::uploads;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const BIG_CHUNK: u64 = 4 * 1024 * 1024;

fn big_chunks() -> ApiConfig {
    ApiConfig {
        upload_chunk_size: BIG_CHUNK,
        ..common::test_config()
    }
}

/// Uploads every chunk of `content` and returns the upload id.
async fn sent(
    browser: &axum_test::TestServer,
    me: &Value,
    parent: Uuid,
    name: &str,
    content: &[u8],
) -> String {
    let session: Value = files::start_upload(browser, me, parent, name, content)
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap().to_owned();
    let chunk = usize::try_from(session["chunk_size"].as_u64().unwrap()).unwrap();
    for (index, bytes) in content.chunks(chunk).enumerate() {
        files::put_chunk(browser, me, &id, index, bytes)
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    id
}

async fn wait_until_claimed(pool: &PgPool, owner: Uuid, id: Uuid) {
    loop {
        let session = uploads::find(pool, owner, id).await.unwrap();
        if session.is_none_or(|s| !s.is_open()) {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(1)).await;
    }
}

async fn wait_for_file(instance: &common::Instance, me: &Value, parent: Uuid, name: &str) -> Value {
    for _ in 0..600 {
        let nodes = squirrel_db::nodes::list_children(
            &instance.state.pool,
            files::user_id(me),
            Some(parent),
            None,
            50,
        )
        .await
        .unwrap();
        if let Some(node) = nodes.items.iter().find(|n| n.name == name) {
            return json!({ "id": node.id, "size": node.size });
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    panic!("{name} was never created: the completion was abandoned");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_dropped_completion_still_finishes(pool: PgPool) {
    let instance = common::Instance::with_config(pool.clone(), big_chunks());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let content: Vec<u8> = (0..32 * 1024 * 1024u32).map(|i| (i % 253) as u8).collect();
    let id = sent(&browser, &me, parent, "big.bin", &content).await;
    let upload_id: Uuid = id.parse().unwrap();

    // The client goes away once the completion has claimed the session.
    tokio::select! {
        _ = files::complete(&browser, &me, &id) => {}
        () = wait_until_claimed(&pool, files::user_id(&me), upload_id) => {}
    }

    let node = wait_for_file(&instance, &me, parent, "big.bin").await;
    assert_eq!(node["size"], content.len());
    browser
        .get(&format!("/api/uploads/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resuming_returns_a_session_being_completed(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let id = sent(&browser, &me, parent, "a.bin", b"abc").await;
    let later = OffsetDateTime::now_utc() + Duration::hours(1);
    uploads::claim(&pool, files::user_id(&me), id.parse().unwrap(), later)
        .await
        .unwrap();

    let resumed: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    assert_eq!(resumed["upload_id"].as_str(), Some(id.as_str()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_abandoned_completion_can_be_retried_after_its_lease(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let id = sent(&browser, &me, parent, "a.bin", b"abc").await;
    let later = OffsetDateTime::now_utc() + Duration::hours(1);
    uploads::claim(&pool, files::user_id(&me), id.parse().unwrap(), later)
        .await
        .unwrap();
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CONFLICT);

    sqlx::query("UPDATE upload_sessions SET claimed_at = now() - interval '11 minutes'")
        .execute(&pool)
        .await
        .unwrap();
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resuming_after_the_file_vanished_forgets_received_chunks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let content = vec![5u8; 1500];
    let first: Value = files::start_upload(&browser, &me, parent, "a.bin", &content)
        .await
        .json();
    let id = first["upload_id"].as_str().unwrap().to_owned();
    files::put_chunk(&browser, &me, &id, 0, &content[..1024])
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let path = instance.data_dir().join("tmp/uploads").join(&id);
    std::fs::remove_file(&path).unwrap();

    let resumed: Value = files::start_upload(&browser, &me, parent, "a.bin", &content)
        .await
        .json();
    assert_eq!(resumed["upload_id"].as_str(), Some(id.as_str()));
    assert_eq!(resumed["received_chunks"], json!([]));
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 1500);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunk_writes_racing_a_completion_never_corrupt_a_blob(pool: PgPool) {
    let instance = common::Instance::with_config(pool, big_chunks());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    for round in 0..4u8 {
        let content: Vec<u8> = (0..8 * 1024 * 1024u32)
            .map(|i| (i % 251) as u8 ^ round)
            .collect();
        let name = format!("race{round}.bin");
        let id = sent(&browser, &me, parent, &name, &content).await;
        let other = vec![0xEEu8; usize::try_from(BIG_CHUNK).unwrap()];
        let writer = async {
            for _ in 0..20 {
                files::put_chunk(&browser, &me, &id, 1, &other).await;
                tokio::task::yield_now().await;
            }
        };
        let (_, done) = tokio::join!(writer, files::complete(&browser, &me, &id));
        if done.status_code() != StatusCode::CREATED {
            continue;
        }
        let blobs = instance.data_dir().join("blobs");
        for shard in walk(&blobs) {
            let bytes = std::fs::read(&shard).unwrap();
            let name = shard.file_name().unwrap().to_str().unwrap().to_owned();
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                name,
                "corrupt blob"
            );
        }
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}
