mod common;

use axum::http::StatusCode;
use axum::http::header::IF_NONE_MATCH;
use axum_test::TestServer;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media;
use uuid::Uuid;

const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00 holiday photo bytes";
const SANDBOX_CSP: &str = "sandbox; default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; frame-ancestors 'self'";

/// Records the blob's media row and, when `ready`, installs both variants as
/// the worker would.
async fn with_media(instance: &common::Instance, bytes: &[u8], ready: bool) -> ContentHash {
    let hash = ContentHash::parse(&files::sha256_hex(bytes)).unwrap();
    let mut conn = instance.state.pool.acquire().await.unwrap();
    media::ensure(&mut conn, hash.as_str(), MediaKind::Image)
        .await
        .unwrap();
    if ready {
        let derived = instance.state.storage.derived();
        let scratch = derived.scratch().unwrap();
        std::fs::write(scratch.path().join("thumb.webp"), b"thumb bytes").unwrap();
        std::fs::write(scratch.path().join("preview.webp"), b"preview bytes").unwrap();
        derived.install(&hash, 1, &scratch).unwrap();
        media::set_thumbnails(
            &instance.state.pool,
            hash.as_str(),
            1,
            40,
            30,
            "LEHV6nWB2yk8",
        )
        .await
        .unwrap();
    }
    hash
}

async fn get(browser: &TestServer, id: &str, size: &str) -> axum_test::TestResponse {
    browser
        .get(&format!("/api/files/{id}/thumbnail?size={size}&v=1"))
        .await
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ready_thumbnails_are_served_privately_and_cached(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = files::root_id(&instance, &me, "Photos").await;
    let node = files::upload(&browser, &me, photos, "a.jpg", JPEG).await;
    let id = node["id"].as_str().unwrap();
    let hash = with_media(&instance, JPEG, true).await;

    let response = get(&browser, id, "thumb").await;
    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), b"thumb bytes");
    let etag = format!("\"{}-thumb-v1\"", hash.as_str());
    for (name, value) in [
        ("content-type", "image/webp"),
        ("cache-control", "private, max-age=31536000, immutable"),
        ("x-content-type-options", "nosniff"),
        ("cross-origin-resource-policy", "same-origin"),
        ("content-security-policy", SANDBOX_CSP),
        ("etag", etag.as_str()),
    ] {
        assert_eq!(response.header(name), value, "{name}");
    }
    let cached = browser
        .get(&format!("/api/files/{id}/thumbnail?size=thumb&v=1"))
        .add_header(
            IF_NONE_MATCH,
            etag.parse::<axum::http::HeaderValue>().unwrap(),
        )
        .await;
    cached.assert_status(StatusCode::NOT_MODIFIED);
    assert_eq!(
        cached.header("cache-control"),
        "private, max-age=31536000, immutable"
    );
    assert_eq!(cached.header("content-security-policy"), SANDBOX_CSP);

    let preview = get(&browser, id, "preview").await;
    preview.assert_status_ok();
    assert_eq!(preview.as_bytes().as_ref(), b"preview bytes");
    let huge = get(&browser, id, "huge").await;
    huge.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(files::code(&huge), "bad_request");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_thumbnails_are_not_ready_or_not_found(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = files::root_id(&instance, &me, "Photos").await;
    let node = files::upload(&browser, &me, photos, "a.jpg", JPEG).await;
    with_media(&instance, JPEG, false).await;
    let pending = get(&browser, node["id"].as_str().unwrap(), "thumb").await;
    pending.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(files::code(&pending), "not_ready");

    let zip = files::upload(&browser, &me, photos, "a.zip", b"PK\x03\x04 zip").await;
    for id in [zip["id"].as_str().unwrap(), &photos.to_string()] {
        let response = get(&browser, id, "thumb").await;
        response.assert_status(StatusCode::NOT_FOUND);
        assert_eq!(files::code(&response), "not_found");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn thumbnails_are_scoped_to_the_owner(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_photos = files::root_id(&instance, &alice_me, "Photos").await;
    let bob_photos = files::root_id(&instance, &bob_me, "Photos").await;
    let mine = files::upload(&alice, &alice_me, alice_photos, "a.jpg", JPEG).await;
    with_media(&instance, JPEG, true).await;
    let mine_id = mine["id"].as_str().unwrap();

    let foreign = get(&bob, mine_id, "thumb").await;
    foreign.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(files::code(&foreign), "not_found");
    let theirs = files::upload(&bob, &bob_me, bob_photos, "same.jpg", JPEG).await;
    get(&bob, theirs["id"].as_str().unwrap(), "thumb")
        .await
        .assert_status_ok();

    alice
        .delete(&format!("/api/nodes/{mine_id}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    get(&alice, mine_id, "thumb")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn listings_carry_ready_thumbnails(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = files::root_id(&instance, &me, "Photos").await;
    let pending_bytes = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00 not processed yet";
    files::upload(&browser, &me, photos, "a.jpg", JPEG).await;
    let pending = files::upload(&browser, &me, photos, "b.jpg", pending_bytes).await;
    with_media(&instance, JPEG, true).await;
    with_media(&instance, pending_bytes, false).await;

    let listing: Value = browser
        .get(&format!("/api/nodes?parent_id={photos}"))
        .await
        .json();
    let items = listing["items"].as_array().unwrap();
    let by_name = |name: &str| items.iter().find(|n| n["name"] == name).unwrap().clone();
    assert_eq!(
        by_name("a.jpg")["thumbnail"],
        serde_json::json!({"version": 1, "blurhash": "LEHV6nWB2yk8"})
    );
    assert!(by_name("b.jpg")["thumbnail"].is_null());

    let id: Uuid = by_name("a.jpg")["id"].as_str().unwrap().parse().unwrap();
    let details: Value = browser.get(&format!("/api/nodes/{id}")).await.json();
    assert_eq!(details["node"]["thumbnail"]["version"], 1);
    assert!(pending["thumbnail"].is_null());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn logging_out_clears_the_browser_cache(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let response = browser
        .post("/api/auth/logout")
        .add_header(common::CSRF, common::csrf(&me))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(response.header("clear-site-data"), "\"cache\"");

    let me = common::login(&browser, "alice").await;
    let all = browser
        .post("/api/auth/logout-all")
        .add_header(common::CSRF, common::csrf(&me))
        .await;
    all.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(all.header("clear-site-data"), "\"cache\"");
}
