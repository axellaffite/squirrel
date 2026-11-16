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
