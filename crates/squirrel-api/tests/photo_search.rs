mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::devices::{bearer, device_login};
use common::files::root_id;
use common::photos::{Seed, photo, seed, sha_of};
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::ml::TextEncoder;
use squirrel_ml::client::{MlBackend, MlClient};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = "openai-vit-b32-224";

/// A unit vector at `angle` radians in the plane of the first two axes.
fn at(angle: f32) -> Vec<f32> {
    let mut v = vec![0.0; 512];
    v[0] = angle.cos();
    v[1] = angle.sin();
    v
}

/// Every query encodes to angle 0.
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

async fn search(browser: &TestServer, query: &str) -> Value {
    let response = browser.get(&format!("/api/search/photos?{query}")).await;
    response.assert_status_ok();
    response.json()
}

fn names(results: &Value) -> Vec<&str> {
    results["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ranks_the_callers_photos_within_the_floor(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    for (name, angle) in [("close.jpg", 0.2), ("same.jpg", 0.0), ("far.jpg", 1.5)] {
        let id = photo(&pool, &me, photos, name, when).await;
        embed(&pool, id, angle).await;
    }
    let results = search(&browser, "q=dog").await;
    assert_eq!(
        names(&results),
        ["same.jpg", "close.jpg"],
        "far.jpg is past the floor"
    );
    assert_eq!(results["has_more"], false);
    let page = search(&browser, "q=dog&limit=1&offset=1").await;
    assert_eq!(names(&page), ["close.jpg"]);
    let none = search(&browser, "q=dog&kind=screenshot").await;
    assert!(names(&none).is_empty());
}

/// Bob holds one of Alice's blobs: he finds his node and nothing else of hers.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn users_never_see_each_others_photos(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_photos = root_id(&instance, &alice_me, "Photos").await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    let shared = photo(&pool, &alice_me, alice_photos, "alice-shared.jpg", when).await;
    let hers = photo(&pool, &alice_me, alice_photos, "alice-only.jpg", when).await;
    embed(&pool, shared, 0.0).await;
    embed(&pool, hers, 0.0).await;
    let sha = sha_of(&pool, shared).await;
    let seeded = Seed {
        sha256: Some(&sha),
        ..Seed::new("bob-copy.jpg", when)
    };
    seed(&pool, &bob_me, bob_photos, &seeded).await;
    assert_eq!(names(&search(&bob, "q=dog").await), ["bob-copy.jpg"]);
    let alice_names = search(&alice, "q=dog").await;
    assert_eq!(names(&alice_names).len(), 2);
    assert!(!names(&alice_names).contains(&"bob-copy.jpg"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bad_queries_and_pages_are_refused(pool: PgPool) {
    let instance = common::Instance::new(pool).with_text_encoder(encoder());
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    let long = "x".repeat(201);
    for query in [
        "q=".to_owned(),
        "q=%20%20".to_owned(),
        format!("q={long}"),
        "q=dog&offset=150&limit=60".to_owned(),
        "q=dog&limit=0".to_owned(),
        "q=dog&city=Paris".to_owned(),
    ] {
        browser
            .get(&format!("/api/search/photos?{query}"))
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn without_a_text_model_search_is_unavailable(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let response = browser.get("/api/search/photos?q=dog").await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.json::<Value>()["error"]["code"], "ml_unavailable");
    browser
        .post("/api/search/warmup")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn warm_up_answers_at_once_and_is_rate_limited(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        search_rate_limit: Some(squirrel_api::RateLimit {
            replenish_every_secs: 60,
            burst: 1,
        }),
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let warm = || {
        browser
            .post("/api/search/warmup")
            .add_header(common::CSRF, common::csrf(&me))
    };
    warm().await.assert_status(StatusCode::NO_CONTENT);
    warm().await.assert_status(StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_phone_searches_with_its_bearer(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let id = photo(&pool, &me, photos, "dog.jpg", datetime!(2024-07-01 10:00)).await;
    embed(&pool, id, 0.0).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let (name, value) = bearer(common::devices::access(&login).as_str());
    let response = instance
        .browser()
        .get("/api/search/photos?q=dog")
        .add_header(name, value)
        .await;
    response.assert_status_ok();
    assert_eq!(names(&response.json()), ["dog.jpg"]);
}

/// The text child is starting (or busy): the search answers 503 `busy` at
/// once, for the client to retry.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_busy_text_model_answers_503_busy(pool: PgPool) {
    let busy = TextEncoder::new(MlClient::new(MlBackend::Fake(Arc::new(|_: &MlOp| {
        Err(squirrel_ml::client::MlClientError::Busy(
            std::time::Duration::ZERO,
        ))
    }))));
    let instance = common::Instance::new(pool).with_text_encoder(busy);
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    let response = browser.get("/api/search/photos?q=dog").await;
    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.json::<Value>()["error"]["code"], "busy");
}

/// A filtered search never returns a photo whose folder is in the trash.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_filtered_search_skips_photos_of_a_trashed_folder(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let folder: Value = browser
        .post("/api/folders")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&serde_json::json!({ "parent_id": photos, "name": "Trip" }))
        .await
        .json();
    let folder_id: Uuid = folder["id"].as_str().unwrap().parse().unwrap();
    let when = datetime!(2024-07-01 10:00);
    let inside = photo(&pool, &me, folder_id, "inside.jpg", when).await;
    let outside = photo(&pool, &me, photos, "outside.jpg", when).await;
    for id in [inside, outside] {
        embed(&pool, id, 0.0).await;
        sqlx::query(
            "UPDATE media m SET place_country_code = 'FR' FROM nodes n \
              WHERE n.id = $1 AND m.sha256 = n.blob_sha256",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(names(&search(&browser, "q=dog&country=FR").await).len(), 2);
    browser
        .delete(&format!("/api/nodes/{folder_id}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        names(&search(&browser, "q=dog&country=FR").await),
        ["outside.jpg"]
    );
}
