//! Concurrency, session-count, rate and disk-space limits on uploads.

mod common;

use std::time::Duration as StdDuration;

use axum::http::StatusCode;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::{ApiConfig, UploadLimits, UserRate};
use squirrel_storage::BlobStore;
use uuid::Uuid;

fn limited(limits: UploadLimits) -> ApiConfig {
    ApiConfig {
        upload_limits: limits,
        ..common::test_config()
    }
}

fn quick() -> UploadLimits {
    UploadLimits {
        slot_wait: StdDuration::from_millis(50),
        ..UploadLimits::default()
    }
}

async fn alice(instance: &common::Instance) -> (axum_test::TestServer, Value, Uuid) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(instance, &me, "Files").await;
    (browser, me, parent)
}

async fn sent(browser: &axum_test::TestServer, me: &Value, parent: Uuid, content: &[u8]) -> String {
    let session: Value = files::start_upload(browser, me, parent, "a.bin", content)
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap().to_owned();
    files::put_chunk(browser, me, &id, 0, content)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    id
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completions_are_limited_per_user_and_globally(pool: PgPool) {
    let config = limited(UploadLimits {
        completion_concurrency: 2,
        completion_per_user: 1,
        ..quick()
    });
    let instance = common::Instance::with_config(pool, config);
    let (browser, me, parent) = alice(&instance).await;
    let id = sent(&browser, &me, parent, b"abc").await;
    let completions = &instance.state.uploads.completions;

    let mine = completions.acquire(files::user_id(&me)).await.unwrap();
    let busy = files::complete(&browser, &me, &id).await;
    busy.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(files::code(&busy), "busy");
    drop(mine);

    let others = (
        completions.acquire(Uuid::now_v7()).await.unwrap(),
        completions.acquire(Uuid::now_v7()).await.unwrap(),
    );
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE);
    drop(others);
    // Refusals left the session open.
    files::complete(&browser, &me, &id)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunk_writes_are_limited(pool: PgPool) {
    let config = limited(UploadLimits {
        chunk_per_user: 1,
        ..quick()
    });
    let instance = common::Instance::with_config(pool, config);
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap();

    let held = instance
        .state
        .uploads
        .chunks
        .acquire(files::user_id(&me))
        .await
        .unwrap();
    let busy = files::put_chunk(&browser, &me, id, 0, b"abc").await;
    busy.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    drop(held);
    files::put_chunk(&browser, &me, id, 0, b"abc")
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_chunk_waits_while_its_upload_is_being_completed(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap().to_owned();
    let locks = &instance.state.uploads.locks;
    let completing = locks
        .completion(id.parse().unwrap(), StdDuration::from_secs(1))
        .await
        .unwrap();

    let put = files::put_chunk(&browser, &me, &id, 0, b"abc");
    tokio::pin!(put);
    assert!(
        tokio::time::timeout(StdDuration::from_millis(200), &mut put)
            .await
            .is_err(),
        "the chunk must not be written during completion"
    );
    drop(completing);
    put.await.assert_status(StatusCode::NO_CONTENT);
    assert!(locks.is_empty(), "lock entries are cleaned up");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_chunk_gives_up_on_a_long_completion(pool: PgPool) {
    let config = limited(UploadLimits {
        lock_wait: StdDuration::from_millis(100),
        ..quick()
    });
    let instance = common::Instance::with_config(pool, config);
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap().to_owned();
    let locks = &instance.state.uploads.locks;
    let completing = locks
        .completion(id.parse().unwrap(), StdDuration::from_secs(1))
        .await
        .unwrap();

    let put = tokio::time::timeout(
        StdDuration::from_secs(5),
        files::put_chunk(&browser, &me, &id, 0, b"abc"),
    )
    .await
    .expect("the chunk must not wait for the whole completion");
    put.assert_status(StatusCode::CONFLICT);
    assert_eq!(files::code(&put), "conflict");
    drop(completing);
    assert!(locks.is_empty(), "lock entries are cleaned up");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn open_sessions_are_capped(pool: PgPool) {
    let config = limited(UploadLimits {
        max_open_per_user: 2,
        ..quick()
    });
    let instance = common::Instance::with_config(pool, config);
    let (browser, me, parent) = alice(&instance).await;
    for name in ["a.bin", "b.bin"] {
        files::start_upload(&browser, &me, parent, name, b"abc")
            .await
            .assert_status(StatusCode::CREATED);
    }
    let refused = files::start_upload(&browser, &me, parent, "c.bin", b"abc").await;
    refused.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(files::code(&refused), "too_many_uploads");
    // Resuming an existing session is still allowed.
    files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creating_uploads_is_rate_limited_per_user(pool: PgPool) {
    let config = limited(UploadLimits {
        create_rate: Some(UserRate {
            per_second: 0.001,
            burst: 2,
        }),
        ..quick()
    });
    let instance = common::Instance::with_config(pool, config);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_root = files::root_id(&instance, &alice_me, "Files").await;
    let bob_root = files::root_id(&instance, &bob_me, "Files").await;
    for name in ["a.bin", "b.bin"] {
        files::start_upload(&alice, &alice_me, alice_root, name, b"abc")
            .await
            .assert_status(StatusCode::CREATED);
    }
    let limited = files::start_upload(&alice, &alice_me, alice_root, "c.bin", b"abc").await;
    limited.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(files::code(&limited), "rate_limited");
    files::start_upload(&bob, &bob_me, bob_root, "a.bin", b"abc")
        .await
        .assert_status(StatusCode::CREATED);
}

/// Free space on the filesystem holding the test data directories.
fn free_space() -> u64 {
    let dir = tempfile::tempdir().unwrap();
    BlobStore::open(dir.path())
        .unwrap()
        .available_space()
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn open_sessions_reserve_disk_space(pool: PgPool) {
    const MIB: u64 = 1024 * 1024;
    let config = ApiConfig {
        // Leaves ~100 MiB usable; each session declares 60 MiB.
        min_free_bytes: free_space().saturating_sub(100 * MIB),
        upload_chunk_size: MIB,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let (browser, me, parent) = alice(&instance).await;
    let declare = |name: &'static str| {
        let body = serde_json::json!({
            "parent_id": parent, "name": name, "size": 60 * MIB,
            "sha256": files::sha256_hex(name.as_bytes()),
        });
        browser
            .post("/api/uploads")
            .add_header(common::CSRF, common::csrf(&me))
            .json(&body)
    };
    declare("a.bin").await.assert_status(StatusCode::CREATED);
    let full = declare("b.bin").await;
    full.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&full), "storage_full");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn one_user_cannot_reserve_most_of_the_free_space(pool: PgPool) {
    const MIB: u64 = 1024 * 1024;
    let config = ApiConfig {
        // Leaves ~1 GiB usable; each session declares 300 MiB (wide margins, so
        // other processes changing the free space do not matter).
        min_free_bytes: free_space().saturating_sub(1000 * MIB),
        upload_chunk_size: MIB,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let declare = |browser: &axum_test::TestServer, me: &Value, parent: Uuid, name: &str| {
        let body = serde_json::json!({
            "parent_id": parent, "name": name, "size": 300 * MIB,
            "sha256": files::sha256_hex(name.as_bytes()),
        });
        browser
            .post("/api/uploads")
            .add_header(common::CSRF, common::csrf(me))
            .json(&body)
    };
    let alice_files = files::root_id(&instance, &alice_me, "Files").await;
    let bob_files = files::root_id(&instance, &bob_me, "Files").await;
    declare(&alice, &alice_me, alice_files, "a.bin")
        .await
        .assert_status(StatusCode::CREATED);
    // 600 of ~1000 MiB would be more than half the usable space.
    let greedy = declare(&alice, &alice_me, alice_files, "b.bin").await;
    greedy.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(files::code(&greedy), "too_many_uploads");
    // Someone else still gets room.
    declare(&bob, &bob_me, bob_files, "c.bin")
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_are_refused_below_the_free_space_floor(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc")
        .await
        .json();
    let id = session["upload_id"].as_str().unwrap();

    let full_disk = instance.sharing(ApiConfig {
        min_free_bytes: u64::MAX / 2,
        ..common::test_config()
    });
    let other = full_disk.browser();
    let me = common::login(&other, "alice").await;
    let refused = files::put_chunk(&other, &me, id, 0, b"abc").await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&refused), "storage_full");
}
