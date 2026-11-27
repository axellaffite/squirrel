mod common;

use std::time::Duration;

use axum::http::header::IF_NONE_MATCH;
use axum::http::{HeaderValue, StatusCode};
use axum_test::{TestResponse, TestServer};
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::previews::PreviewLimits;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media;
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};

/// A real 3000 × 2000 JPEG uploaded by the admin, recorded as ready.
struct Photo {
    browser: TestServer,
    id: String,
    hash: ContentHash,
}

async fn photo(instance: &common::Instance) -> Photo {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    photo_of(instance, browser, me).await
}

async fn photo_of(instance: &common::Instance, browser: TestServer, me: Value) -> Photo {
    let parent = files::root_id(instance, &me, "Photos").await;
    let jpeg = jpeg_sized_with_exif(3000, 2000, &ExifSpec::default());
    let node = files::upload(&browser, &me, parent, "a.jpg", &jpeg).await;
    let hash = ready(instance, &jpeg, MediaKind::Image, (3000, 2000)).await;
    Photo {
        browser,
        id: node["id"].as_str().unwrap().to_owned(),
        hash,
    }
}

/// Records `bytes`' media row as ready, with a stored thumbnail and 1440 preview.
async fn ready(
    instance: &common::Instance,
    bytes: &[u8],
    kind: MediaKind,
    (width, height): (u32, u32),
) -> ContentHash {
    let hash = ContentHash::parse(&files::sha256_hex(bytes)).unwrap();
    let mut conn = instance.state.pool.acquire().await.unwrap();
    media::ensure(&mut conn, hash.as_str(), kind).await.unwrap();
    let derived = instance.state.storage.derived();
    let scratch = derived.scratch().unwrap();
    std::fs::write(scratch.path().join("thumb.webp"), b"thumb bytes").unwrap();
    std::fs::write(scratch.path().join("preview.webp"), b"stored preview").unwrap();
    derived.install(&hash, 1, &scratch).unwrap();
    media::set_thumbnails(
        &instance.state.pool,
        hash.as_str(),
        1,
        width,
        height,
        "LEHV6nWB2yk8",
    )
    .await
    .unwrap();
    hash
}

async fn preview(browser: &TestServer, id: &str, px: Option<u32>) -> TestResponse {
    let px = px.map(|px| format!("&px={px}")).unwrap_or_default();
    browser
        .get(&format!("/api/files/{id}/thumbnail?size=preview&v=1{px}"))
        .await
}

fn dimensions(response: &TestResponse) -> (u32, u32) {
    let image = image::load_from_memory(response.as_bytes()).unwrap();
    (image.width(), image.height())
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn previews_are_rendered_per_bucket_and_cached(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let photo = photo(&instance).await;
    for (px, size) in [
        (None, (1440, 960)),
        (Some(1000), (1440, 960)),
        (Some(1441), (2160, 1440)),
        // Bucket 3200, never above the original.
        (Some(9000), (3000, 2000)),
    ] {
        let response = preview(&photo.browser, &photo.id, px).await;
        response.assert_status_ok();
        assert_eq!(response.header("content-type"), "image/jpeg");
        assert_eq!(dimensions(&response), size, "px {px:?}");
    }
    let cache = instance.state.preview_cache.as_ref().unwrap();
    assert_eq!(cache.stats().entries, 3);

    let response = preview(&photo.browser, &photo.id, Some(2000)).await;
    let etag = format!("\"{}-p2160-r1\"", photo.hash.as_str());
    assert_eq!(response.header("etag"), etag.as_str());
    assert_eq!(response.header("cache-control"), "private, max-age=604800");
    assert_eq!(response.header("x-content-type-options"), "nosniff");
    assert_eq!(
        response.header("cross-origin-resource-policy"),
        "same-origin"
    );
    let cached = photo
        .browser
        .get(&format!(
            "/api/files/{}/thumbnail?size=preview&px=2000",
            photo.id
        ))
        .add_header(IF_NONE_MATCH, etag.parse::<HeaderValue>().unwrap())
        .await;
    cached.assert_status(StatusCode::NOT_MODIFIED);
    assert_eq!(cached.header("cache-control"), "private, max-age=604800");

    let zero = preview(&photo.browser, &photo.id, Some(0)).await;
    zero.assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_users_preview_is_not_found(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let photo = photo_of(&instance, alice, alice_me).await;
    let foreign = preview(&bob, &photo.id, Some(1440)).await;
    foreign.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(files::code(&foreign), "not_found");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_render_without_a_free_slot_is_busy(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        preview_limits: PreviewLimits {
            concurrency: 1,
            slot_wait: Duration::from_millis(50),
            render_rate: None,
            ..PreviewLimits::default()
        },
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let photo = photo(&instance).await;
    let warm = preview(&photo.browser, &photo.id, None).await;
    warm.assert_status_ok();

    let held = instance.state.previews.slot(false).await.unwrap();
    let busy = preview(&photo.browser, &photo.id, Some(2160)).await;
    busy.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(files::code(&busy), "busy");
    assert_eq!(busy.header("retry-after"), "1");
    // Cache hits take no slot.
    preview(&photo.browser, &photo.id, None)
        .await
        .assert_status_ok();
    drop(held);
    preview(&photo.browser, &photo.id, Some(2160))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renders_are_rate_limited_per_user(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        preview_limits: PreviewLimits {
            render_rate: Some(squirrel_api::UserRate {
                per_second: 0.001,
                burst: 1,
            }),
            ..PreviewLimits::default()
        },
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let photo = photo(&instance).await;
    preview(&photo.browser, &photo.id, None)
        .await
        .assert_status_ok();
    let limited = preview(&photo.browser, &photo.id, Some(3000)).await;
    limited.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.header("retry-after"), "1");
    // Cached: no render, no token.
    preview(&photo.browser, &photo.id, None)
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pdf_previews_stay_stored_at_1440(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let pdf = b"%PDF-1.4 a document";
    let node = files::upload(&browser, &me, parent, "a.pdf", pdf).await;
    let hash = ready(&instance, pdf, MediaKind::Pdf, (1018, 1440)).await;
    let response = preview(&browser, node["id"].as_str().unwrap(), Some(3200)).await;
    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), b"stored preview");
    assert_eq!(response.header("content-type"), "image/webp");
    let etag = format!("\"{}-preview-v1\"", hash.as_str());
    assert_eq!(response.header("etag"), etag.as_str());
}

/// A photo that cannot be rendered (here: not an image at all) falls back to
/// the stored preview while one exists.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_unrenderable_photo_falls_back_to_its_stored_preview(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Photos").await;
    let broken = b"\xFF\xD8\xFF\xE0 not really a jpeg";
    let node = files::upload(&browser, &me, parent, "b.jpg", broken).await;
    ready(&instance, broken, MediaKind::Image, (40, 30)).await;
    let response = preview(&browser, node["id"].as_str().unwrap(), None).await;
    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), b"stored preview");
    assert_eq!(response.header("cache-control"), "private, no-cache");
}
