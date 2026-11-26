//! Dynamic albums with a search text: their photos match it as photo search does.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::root_id;
use common::photos::{photo, sha_of};
use common::{CSRF, csrf};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::ml::TextEncoder;
use squirrel_ml::client::{MlBackend, MlClient};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = squirrel_ml::CLIP_MODEL_ID;

/// A unit vector at `angle` radians in the plane of the first two axes.
fn at(angle: f32) -> Vec<f32> {
    let mut v = vec![0.0; squirrel_ml::clip::EMBEDDING_DIM];
    v[0] = angle.cos();
    v[1] = angle.sin();
    v
}

/// Every text encodes to angle 0.
fn encoder() -> TextEncoder {
    TextEncoder::new(MlClient::new(MlBackend::Fake(Arc::new(|op: &MlOp| {
        let MlOp::ClipText { texts } = op else {
            return Ok(MlBody::Ok(MlOutput::Pong {}));
        };
        Ok(MlBody::Ok(MlOutput::Embeddings {
            embeddings: texts.iter().map(|_| at(0.0)).collect(),
        }))
    }))))
}

async fn embed(pool: &PgPool, node: Uuid, angle: f32) {
    squirrel_db::clip::store_embedding(pool, &sha_of(pool, node).await, MODEL, &at(angle))
        .await
        .unwrap();
}

async fn send(
    browser: &TestServer,
    me: &Value,
    patch: bool,
    url: &str,
    body: Value,
) -> TestResponse {
    let request = if patch {
        browser.patch(url)
    } else {
        browser.post(url)
    };
    request.add_header(CSRF, csrf(me)).json(&body).await
}

async fn names(browser: &TestServer, album: &str) -> Vec<String> {
    let page: Value = browser
        .get(&format!("/api/albums/{album}/items"))
        .await
        .json();
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_owned())
        .collect()
}

/// Stores a label bank of two sibling labels far from every photo.
async fn bank(pool: &PgPool) {
    use squirrel_ml::clip::labels;
    squirrel_db::clip::store_label_embeddings(
        pool,
        MODEL,
        labels::LABELS_VERSION,
        &labels::bank_digest(),
        &[("cat".to_owned(), at(2.5)), ("dog".to_owned(), at(3.0))],
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_waits_for_the_label_bank(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let body = json!({ "name": "Shirtless", "rules": { "search": "shirtless" } });
    let early = send(&browser, &me, false, "/api/albums", body.clone()).await;
    early.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(early.json::<Value>()["error"]["code"], "busy");
    bank(&pool).await;
    send(&browser, &me, false, "/api/albums", body)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_fills_the_album_as_search_would(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    bank(&pool).await;
    let photos = root_id(&instance, &me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    for (name, angle) in [("same.jpg", 0.0), ("close.jpg", 0.2), ("far.jpg", 1.5)] {
        let id = photo(&pool, &me, photos, name, when).await;
        embed(&pool, id, angle).await;
    }

    let preview = send(
        &browser,
        &me,
        false,
        "/api/albums/preview",
        json!({ "search": " shirtless " }),
    )
    .await;
    preview.assert_status_ok();
    assert_eq!(
        preview.json::<Value>()["count"],
        2,
        "far.jpg is past the floor"
    );

    let created = send(
        &browser,
        &me,
        false,
        "/api/albums",
        json!({ "name": "Shirtless", "rules": { "search": " shirtless " } }),
    )
    .await;
    created.assert_status(StatusCode::CREATED);
    let album: Value = created.json();
    assert_eq!(album["rules"]["search"], "shirtless");
    assert_eq!(album["item_count"], 2);
    let id = album["id"].as_str().unwrap().to_owned();
    let mut found = names(&browser, &id).await;
    found.sort();
    assert_eq!(found, ["close.jpg", "same.jpg"]);

    // Another condition narrows it (AND); dropping the text widens it again.
    let narrowed = send(
        &browser,
        &me,
        true,
        &format!("/api/albums/{id}"),
        json!({ "rules": { "search": "shirtless", "taken_to": "2024-06-30" } }),
    )
    .await;
    narrowed.assert_status_ok();
    assert!(names(&browser, &id).await.is_empty());
    let widened = send(
        &browser,
        &me,
        true,
        &format!("/api/albums/{id}"),
        json!({ "rules": { "taken_to": "2024-07-01" } }),
    )
    .await;
    widened.assert_status_ok();
    assert_eq!(widened.json::<Value>()["rules"]["search"], Value::Null);
    assert_eq!(names(&browser, &id).await.len(), 3);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_needs_the_text_model(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let created = send(
        &browser,
        &me,
        false,
        "/api/albums",
        json!({ "name": "Shirtless", "rules": { "search": "shirtless" } }),
    )
    .await;
    created.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(created.json::<Value>()["error"]["code"], "ml_unavailable");
    let too_long = send(
        &browser,
        &me,
        false,
        "/api/albums/preview",
        json!({ "search": "a".repeat(201) }),
    )
    .await;
    too_long.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}
