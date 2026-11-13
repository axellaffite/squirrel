mod common;

use axum::http::StatusCode;
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_SECURITY_POLICY,
    CONTENT_TYPE, ETAG, IF_NONE_MATCH, RANGE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum_test::TestServer;
use common::files;
use serde_json::Value;
use sqlx::PgPool;

async fn alice_with(
    instance: &common::Instance,
    name: &str,
    content: &[u8],
) -> (TestServer, Value, String) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, name, content).await;
    let url = format!("/api/files/{}/content", node["id"].as_str().unwrap());
    (browser, me, url)
}

fn header(response: &axum_test::TestResponse, name: axum::http::HeaderName) -> String {
    response.header(name).to_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn serves_the_bytes_with_safe_headers(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "notes.txt", b"hello squirrel").await;
    let response = browser.get(&url).await;
    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), b"hello squirrel");
    assert_eq!(header(&response, CONTENT_TYPE), "text/plain; charset=utf-8");
    assert_eq!(
        header(&response, CONTENT_DISPOSITION),
        "inline; filename=\"notes.txt\"; filename*=UTF-8''notes.txt"
    );
    assert_eq!(
        header(&response, ETAG),
        format!("\"{}\"", files::sha256_hex(b"hello squirrel"))
    );
    assert_eq!(header(&response, CACHE_CONTROL), "private, no-cache");
    assert_eq!(header(&response, ACCEPT_RANGES), "bytes");
    assert_eq!(header(&response, X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(header(&response, X_FRAME_OPTIONS), "SAMEORIGIN");
    assert!(header(&response, CONTENT_SECURITY_POLICY).starts_with("sandbox;"));
    assert_eq!(
        header(&response, "cross-origin-resource-policy".parse().unwrap()),
        "same-origin"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn honours_single_ranges(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "abc.txt", b"abcdefghij").await;
    let partial = browser.get(&url).add_header(RANGE, "bytes=2-4").await;
    partial.assert_status(StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.as_bytes().as_ref(), b"cde");
    assert_eq!(header(&partial, CONTENT_RANGE), "bytes 2-4/10");

    let tail = browser.get(&url).add_header(RANGE, "bytes=-3").await;
    assert_eq!(tail.as_bytes().as_ref(), b"hij");

    let outside = browser.get(&url).add_header(RANGE, "bytes=50-").await;
    outside.assert_status(StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(header(&outside, CONTENT_RANGE), "bytes */10");
    assert_eq!(files::code(&outside), "range_not_satisfiable");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revalidates_with_etags(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "a.txt", b"etag me").await;
    let etag = header(&browser.get(&url).await, ETAG);
    let cached = browser
        .get(&url)
        .add_header(IF_NONE_MATCH, etag.as_str())
        .await;
    cached.assert_status(StatusCode::NOT_MODIFIED);
    assert!(cached.as_bytes().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn download_forces_attachment_and_keeps_unicode_names(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "café.txt", b"bonjour").await;
    let response = browser.get(&format!("{url}?download=true")).await;
    assert_eq!(
        header(&response, CONTENT_DISPOSITION),
        "attachment; filename=\"caf_.txt\"; filename*=UTF-8''caf%C3%A9.txt"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn active_content_is_never_served_as_itself(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(
        &instance,
        "page.html",
        b"<!DOCTYPE html><script>alert(document.cookie)</script>",
    )
    .await;
    let response = browser.get(&url).await;
    assert_eq!(header(&response, CONTENT_TYPE), "application/octet-stream");
    assert!(header(&response, CONTENT_DISPOSITION).starts_with("attachment;"));
    assert!(header(&response, CONTENT_SECURITY_POLICY).starts_with("sandbox;"));
    assert_eq!(header(&response, X_CONTENT_TYPE_OPTIONS), "nosniff");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pdfs_are_inline_and_frameable_by_the_app(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "doc.pdf", b"%PDF-1.7\n%%EOF\n").await;
    let response = browser.get(&url).await;
    assert_eq!(header(&response, CONTENT_TYPE), "application/pdf");
    assert!(header(&response, CONTENT_DISPOSITION).starts_with("inline;"));
    assert!(!header(&response, CONTENT_SECURITY_POLICY).contains("sandbox"));
    assert!(header(&response, CONTENT_SECURITY_POLICY).contains("frame-ancestors 'self'"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_trash_and_other_users_are_404(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let parent = files::root_id(&instance, &alice_me, "Files").await;
    let node = files::upload(&alice, &alice_me, parent, "a.txt", b"secret").await;
    let id = node["id"].as_str().unwrap();

    bob.get(&format!("/api/files/{id}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    alice
        .get(&format!("/api/files/{parent}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    alice
        .delete(&format!("/api/nodes/{id}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    alice
        .get(&format!("/api/files/{id}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_api_responses_stay_no_store(pool: PgPool) {
    let browser = common::server(pool);
    common::setup_admin(&browser, "alice").await;
    assert_eq!(
        header(&browser.get("/api/nodes").await, CACHE_CONTROL),
        "no-store"
    );
}
