mod common;

use axum::http::StatusCode;
use common::devices::{access, device_login, sync_check};
use common::files::{code, root_id, sha256_hex, upload};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, UserRate};

// Timing: whether another user has some content never changes the answer or
// the work done, because the check reads only the caller's own nodes
// (`nodes.owner_id = caller`); asserted structurally below, not by timing.

fn missing(response: axum_test::TestResponse) -> Vec<String> {
    response.assert_status_ok();
    serde_json::from_value(response.json::<Value>()["missing"].clone()).unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_check_reports_what_the_callers_library_lacks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_files = root_id(&instance, &alice_me, "Files").await;
    let bob_files = root_id(&instance, &bob_me, "Files").await;
    upload(&alice, &alice_me, alice_files, "web.jpg", b"from the web").await;
    upload(&bob, &bob_me, bob_files, "bobs.jpg", b"only bob").await;
    let phone = device_login(&instance, "alice", "Pixel").await;

    let (web, bobs, nobody) = (
        sha256_hex(b"from the web"),
        sha256_hex(b"only bob"),
        sha256_hex(b"nobody"),
    );
    let items = [
        ("images:1", web.as_str()),
        ("images:2", bobs.as_str()),
        ("images:3", nobody.as_str()),
    ];
    let first = missing(sync_check(&instance, &access(&phone), &items).await);
    assert_eq!(first, ["images:2", "images:3"]);
    let second = missing(sync_check(&instance, &access(&phone), &items).await);
    assert_eq!(second, ["images:2", "images:3"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_devices_may_check(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let body = json!({ "items": [{ "device_asset_id": "a", "sha256": "0".repeat(64) }] });
    let by_session = browser
        .post("/api/sync/check")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&body)
        .await;
    by_session.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(code(&by_session), "forbidden");
    instance
        .browser()
        .post("/api/sync/check")
        .json(&body)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_batches_are_refused_whole(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel").await;
    let good = "0".repeat(64);
    let many: Vec<String> = (0..501).map(|i| format!("images:{i}")).collect();
    let long_id = "x".repeat(257);
    let upper = "A".repeat(64);
    let short = "0".repeat(63);
    let batches: Vec<Vec<(&str, &str)>> = vec![
        vec![],
        many.iter().map(|id| (id.as_str(), good.as_str())).collect(),
        vec![("dup", good.as_str()), ("dup", good.as_str())],
        vec![("ok", good.as_str()), (long_id.as_str(), good.as_str())],
        vec![("ok", good.as_str()), ("bad\nid", good.as_str())],
        vec![("ok", good.as_str()), ("upper", upper.as_str())],
        vec![("ok", good.as_str()), ("short", short.as_str())],
    ];
    for batch in batches {
        let response = sync_check(&instance, &access(&phone), &batch).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error");
    }
    let recorded: i64 = sqlx::query_scalar("SELECT count(*) FROM device_assets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(recorded, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_full_batch_fits_the_body_limit(pool: PgPool) {
    let instance = common::Instance::new(pool);
    common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel").await;
    let ids: Vec<String> = (0..500).map(|i| format!("{i:0>256}")).collect();
    let hashes: Vec<String> = (0..500).map(|i| format!("{i:064x}")).collect();
    let items: Vec<(&str, &str)> = ids
        .iter()
        .zip(&hashes)
        .map(|(id, sha)| (id.as_str(), sha.as_str()))
        .collect();
    assert_eq!(
        missing(sync_check(&instance, &access(&phone), &items).await).len(),
        500
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn checks_are_rate_limited_per_device(pool: PgPool) {
    let instance = common::Instance::with_config(
        pool,
        ApiConfig {
            sync_check_rate: Some(UserRate {
                per_second: 0.01,
                burst: 20,
            }),
            ..common::test_config()
        },
    );
    common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel").await;
    let other = device_login(&instance, "alice", "iPhone").await;
    let item = [("images:1", "0".repeat(64))];
    let items: Vec<(&str, &str)> = item.iter().map(|(a, b)| (*a, b.as_str())).collect();
    for _ in 0..20 {
        sync_check(&instance, &access(&phone), &items)
            .await
            .assert_status_ok();
    }
    let limited = sync_check(&instance, &access(&phone), &items).await;
    limited.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(code(&limited), "rate_limited");
    sync_check(&instance, &access(&other), &items)
        .await
        .assert_status_ok();
}

async fn sync_folder(
    instance: &common::Instance,
    token: &str,
    year: Value,
) -> axum_test::TestResponse {
    let (name, value) = common::devices::bearer(token);
    instance
        .browser()
        .post("/api/sync/folder")
        .add_header(name, value)
        .json(&json!({ "year": year }))
        .await
}

async fn folder_of(instance: &common::Instance, token: &str, year: i32) -> uuid::Uuid {
    let response = sync_folder(instance, token, json!(year)).await;
    response.assert_status_ok();
    response.json::<Value>()["folder_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// `(name, parent name)` of a node, read from the database.
async fn placed(pool: &PgPool, id: uuid::Uuid) -> (String, String) {
    sqlx::query_as(
        "SELECT n.name, p.name FROM nodes n JOIN nodes p ON p.id = n.parent_id WHERE n.id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn folders_named(pool: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM nodes WHERE name = $1 AND deleted_at IS NULL")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_year_folder_is_created_once_under_the_devices_folder(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let year = folder_of(&instance, &access(&phone), 2024).await;
    assert_eq!(placed(&pool, year).await, ("2024".into(), "Pixel 8".into()));
    let device_folder: uuid::Uuid = sqlx::query_scalar("SELECT parent_id FROM nodes WHERE id = $1")
        .bind(year)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(placed(&pool, device_folder).await.1, "Photos");
    let recorded: Option<uuid::Uuid> = sqlx::query_scalar("SELECT backup_folder_id FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(recorded, Some(device_folder));

    assert_eq!(folder_of(&instance, &access(&phone), 2024).await, year);
    assert_eq!(folders_named(&pool, "Pixel 8").await, 1);
    let other = folder_of(&instance, &access(&phone), 2023).await;
    assert_eq!(
        placed(&pool, other).await,
        ("2023".into(), "Pixel 8".into())
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_folder_the_user_made_is_reused(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let made: Value = browser
        .post("/api/folders")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&json!({ "parent_id": photos, "name": "Pixel 8" }))
        .await
        .json();
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let year = folder_of(&instance, &access(&phone), 2024).await;
    let parent: uuid::Uuid = sqlx::query_scalar("SELECT parent_id FROM nodes WHERE id = $1")
        .bind(year)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(parent.to_string(), made["id"].as_str().unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_trashed_device_folder_is_replaced_not_resurrected(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let year = folder_of(&instance, &access(&phone), 2024).await;
    let old: uuid::Uuid = sqlx::query_scalar("SELECT backup_folder_id FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    browser
        .delete(&format!("/api/nodes/{old}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let fresh = folder_of(&instance, &access(&phone), 2024).await;
    assert_ne!(fresh, year);
    let recorded: uuid::Uuid = sqlx::query_scalar("SELECT backup_folder_id FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(recorded, old);
    let still_trashed: bool =
        sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM nodes WHERE id = $1")
            .bind(old)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(still_trashed);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_calls_create_one_folder(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let token = access(&phone);
    let ids = tokio::join!(
        folder_of(&instance, &token, 2024),
        folder_of(&instance, &token, 2024),
        folder_of(&instance, &token, 2024),
        folder_of(&instance, &token, 2024),
    );
    assert!(ids.0 == ids.1 && ids.1 == ids.2 && ids.2 == ids.3);
    assert_eq!(folders_named(&pool, "Pixel 8").await, 1);
    assert_eq!(folders_named(&pool, "2024").await, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_folder_route_validates_and_wants_a_device(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    for year in [json!(1899), json!(10000), json!("2024")] {
        let response = sync_folder(&instance, &access(&phone), year.clone()).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{year}");
    }
    browser
        .post("/api/sync/folder")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&json!({ "year": 2024 }))
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

/// Uploads `content` from the phone as `asset` into `Photos/<device>/2024`; returns the node id.
async fn phone_upload(
    instance: &common::Instance,
    token: &str,
    asset: &str,
    content: &[u8],
) -> String {
    let folder = folder_of(instance, token, 2024).await;
    let (name, value) = common::devices::bearer(token);
    let browser = instance.browser();
    let started = browser
        .post("/api/uploads")
        .add_header(name.clone(), value.clone())
        .json(&json!({
            "parent_id": folder, "name": format!("{asset}.jpg"), "size": content.len(),
            "sha256": sha256_hex(content), "device_asset_id": asset,
        }))
        .await;
    started.assert_status(StatusCode::CREATED);
    let id = started.json::<Value>()["upload_id"]
        .as_str()
        .unwrap()
        .to_owned();
    browser
        .put(&format!("/api/uploads/{id}/chunks/0"))
        .add_header(name.clone(), value.clone())
        .bytes(axum::body::Bytes::copy_from_slice(content))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let done = browser
        .post(&format!("/api/uploads/{id}/complete"))
        .add_header(name, value)
        .await;
    done.assert_status(StatusCode::CREATED);
    done.json::<Value>()["id"].as_str().unwrap().to_owned()
}

async fn device_logout(instance: &common::Instance, token: &str) {
    let (name, value) = common::devices::bearer(token);
    instance
        .browser()
        .post("/api/auth/device/logout")
        .add_header(name, value)
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn content_deleted_for_good_is_never_asked_for_again(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let node = phone_upload(&instance, &access(&phone), "images:1", b"a holiday photo").await;

    for request in [
        browser.delete(&format!("/api/nodes/{node}")),
        browser.delete(&format!("/api/trash/{node}")),
    ] {
        request
            .add_header(common::CSRF, common::csrf(&me))
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    device_logout(&instance, &access(&phone)).await;

    // Signed in again (a reinstall): a new device asks by hash.
    let again = device_login(&instance, "alice", "Pixel 8").await;
    let sha = sha256_hex(b"a holiday photo");
    let other = sha256_hex(b"a new photo");
    let items = [("images:1", sha.as_str()), ("images:2", other.as_str())];
    assert_eq!(
        missing(sync_check(&instance, &access(&again), &items).await),
        ["images:2"]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashed_content_is_not_asked_for_again(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let phone = device_login(&instance, "alice", "Pixel 8").await;
    let node = phone_upload(&instance, &access(&phone), "images:1", b"trashed photo").await;
    browser
        .delete(&format!("/api/nodes/{node}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    device_logout(&instance, &access(&phone)).await;

    let again = device_login(&instance, "alice", "Pixel 8").await;
    let sha = sha256_hex(b"trashed photo");
    assert!(
        missing(sync_check(&instance, &access(&again), &[("images:1", sha.as_str())]).await)
            .is_empty()
    );
}
