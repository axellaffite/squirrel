mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::files;
use sqlx::PgPool;
use squirrel_api::{ApiConfig, DownloadLimits};

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn downloads_beyond_the_per_user_limit_are_busy(pool: PgPool) {
    let config = ApiConfig {
        download_limits: DownloadLimits {
            concurrency: 4,
            per_user: 1,
            wait: Duration::from_millis(50),
        },
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"limited").await;
    let url = format!("/api/files/{}/content", node["id"].as_str().unwrap());

    let held = instance
        .state
        .downloads
        .acquire(files::user_id(&me))
        .await
        .expect("a free slot");
    let refused = browser.get(&url).await;
    refused.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(files::code(&refused), "busy");

    drop(held);
    let served = browser.get(&url).await;
    served.assert_status_ok();
    // The slot went back once the body was sent.
    browser.get(&url).await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn not_modified_answers_need_no_slot(pool: PgPool) {
    let config = ApiConfig {
        download_limits: DownloadLimits {
            concurrency: 1,
            per_user: 1,
            wait: Duration::from_millis(50),
        },
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"cached").await;
    let url = format!("/api/files/{}/content", node["id"].as_str().unwrap());
    let etag = browser.get(&url).await.header("etag").to_owned();

    let _held = instance.state.downloads.acquire(files::user_id(&me)).await;
    browser
        .get(&url)
        .add_header("if-none-match", etag)
        .await
        .assert_status(StatusCode::NOT_MODIFIED);
}
