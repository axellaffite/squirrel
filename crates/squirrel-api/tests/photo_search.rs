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

const MODEL: &str = squirrel_ml::CLIP_MODEL_ID;

/// A unit vector at `angle` radians in the plane of the first two axes.
fn at(angle: f32) -> Vec<f32> {
    let mut v = vec![0.0; squirrel_ml::clip::EMBEDDING_DIM];
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

/// Stores a label bank with `dog` and `cat` (siblings) at these angles.
async fn bank(pool: &PgPool, dog: f32, cat: f32) {
    use squirrel_ml::clip::labels;
    squirrel_db::clip::store_label_embeddings(
        pool,
        MODEL,
        labels::LABELS_VERSION,
        &labels::bank_digest(),
        &[("cat".to_owned(), at(cat)), ("dog".to_owned(), at(dog))],
    )
    .await
    .unwrap();
}

/// The query (angle 0) reads as the label `dog` when the bank's `dog` is
/// close to it: then a photo closer to `cat` than to the query is dropped.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_where_a_sibling_label_wins_is_dropped(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    for (name, angle) in [("dog.jpg", 0.0), ("cat.jpg", 0.25)] {
        let id = photo(&pool, &me, photos, name, when).await;
        embed(&pool, id, angle).await;
    }
    let all = ["dog.jpg", "cat.jpg"];
    assert_eq!(names(&search(&browser, "q=dog").await), all, "no bank yet");
    bank(&pool, 1.2, 0.35).await;
    assert_eq!(
        names(&search(&browser, "q=dog").await),
        all,
        "the query is far from every label: no rival"
    );
    bank(&pool, 0.0, 0.35).await;
    assert_eq!(names(&search(&browser, "q=dog").await), ["dog.jpg"]);
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

fn ids(results: &Value) -> Vec<String> {
    results["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Every page of `query` with `order=date`, following `next_cursor`.
async fn every_page_by_date(browser: &TestServer, query: &str) -> Vec<String> {
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page_query = match &cursor {
            Some(cursor) => format!("{query}&order=date&cursor={cursor}"),
            None => format!("{query}&order=date"),
        };
        let page = search(browser, &page_query).await;
        all.extend(ids(&page));
        match page["next_cursor"].as_str() {
            Some(next) => {
                assert_eq!(page["has_more"], true);
                cursor = Some(next.to_owned());
            }
            None => {
                assert_eq!(page["has_more"], false);
                return all;
            }
        }
    }
}

/// Every page of `query` by offset (relevance), up to the 200 reachable.
async fn every_page_by_relevance(browser: &TestServer, query: &str) -> Vec<String> {
    let mut all = Vec::new();
    loop {
        let page = search(browser, &format!("{query}&offset={}", all.len())).await;
        assert!(page["next_cursor"].is_null(), "relevance pages by offset");
        all.extend(ids(&page));
        if page["has_more"] == false {
            return all;
        }
    }
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// A photo without a capture date: its `taken_at` is its upload time.
async fn undated(pool: &PgPool, node: Uuid, uploaded: time::PrimitiveDateTime) {
    sqlx::query(
        "UPDATE assets SET taken_at = $2, taken_offset_min = NULL, taken_source = 'upload' \
          WHERE node_id = $1",
    )
    .bind(node)
    .bind(uploaded)
    .execute(pool)
    .await
    .unwrap();
}

/// `order=date` returns relevance's results, newest capture first, exactly as
/// the timeline orders them (an undated photo at its upload time, equal times
/// by node id, descending), across pages without gaps or duplicates.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn date_order_returns_the_same_results_as_the_timeline_orders_them(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let same_time = datetime!(2024-07-01 10:00);
    for (name, when, angle) in [
        ("old-best.jpg", datetime!(2019-03-02 08:00), 0.0),
        ("new-close.jpg", datetime!(2025-12-24 18:30), 0.2),
        ("twin-a.jpg", same_time, 0.1),
        ("twin-b.jpg", same_time, 0.15),
        ("mid.jpg", datetime!(2023-01-15 12:00), 0.05),
        ("far-new.jpg", datetime!(2026-01-01 09:00), 1.5),
        ("undated.jpg", same_time, 0.1),
    ] {
        let id = photo(&pool, &me, photos, name, when).await;
        embed(&pool, id, angle).await;
        if name == "undated.jpg" {
            undated(&pool, id, datetime!(2026-11-20 21:00)).await;
        }
    }
    let relevance = every_page_by_relevance(&browser, "q=dog&limit=2").await;
    let by_date = every_page_by_date(&browser, "q=dog&limit=2").await;
    assert_eq!(relevance.len(), 6, "far-new.jpg is past the floor");
    assert_eq!(sorted(relevance.clone()), sorted(by_date.clone()));

    let whole = search(&browser, "q=dog&order=date").await;
    assert_eq!(ids(&whole), by_date, "pages join without gaps");
    assert!(whole["next_cursor"].is_null());
    let order = names(&whole);
    assert_eq!(order[..2], ["undated.jpg", "new-close.jpg"]);
    assert_eq!(order[4..], ["mid.jpg", "old-best.jpg"]);
    let mut twins = order[2..4].to_vec();
    twins.sort_unstable();
    assert_eq!(twins, ["twin-a.jpg", "twin-b.jpg"]);
    assert!(by_date[2] > by_date[3], "equal times: node id descending");

    // The timeline's own order, restricted to the results.
    let timeline: Value = browser.get("/api/photos").await.json();
    let expected: Vec<String> = ids(&timeline)
        .into_iter()
        .filter(|id| relevance.contains(id))
        .collect();
    assert_eq!(by_date, expected);

    let filtered = search(&browser, "q=dog&order=date&kind=screenshot").await;
    assert!(names(&filtered).is_empty());
}

/// Past 200 results, `order=date` orders the 200 that relevance can reach.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn date_order_keeps_the_200_best(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let start = datetime!(2020-01-01 00:00);
    for index in 0..205u16 {
        // The newest photos match worst: a cut by date would keep other ones.
        let when = start + time::Duration::days(i64::from(index));
        let id = photo(&pool, &me, photos, &format!("p{index:03}.jpg"), when).await;
        embed(&pool, id, f32::from(index) * 0.001).await;
    }
    let relevance = every_page_by_relevance(&browser, "q=dog&limit=100").await;
    let by_date = every_page_by_date(&browser, "q=dog&limit=100").await;
    assert_eq!(relevance.len(), 200);
    assert_eq!(sorted(relevance), sorted(by_date));
    let newest = search(&browser, "q=dog&order=date&limit=1").await;
    assert_eq!(names(&newest), ["p199.jpg"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bad_orders_and_cursors_are_refused(pool: PgPool) {
    let instance = common::Instance::new(pool).with_text_encoder(encoder());
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    for query in [
        "q=dog&order=",
        "q=dog&order=newest",
        "q=dog&order=DATE",
        "q=dog&order=date&offset=0",
        "q=dog&order=date&limit=101",
        "q=dog&cursor=abc",
        "q=dog&order=relevance&cursor=abc",
    ] {
        browser
            .get(&format!("/api/search/photos?{query}"))
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    browser
        .get("/api/search/photos?q=dog&order=date&cursor=!!!")
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    browser
        .get("/api/search/photos?q=dog&order=relevance")
        .await
        .assert_status_ok();
}

/// Another user's photos never appear and never change the caller's results
/// (the relative cutoff and the 200 best are taken among the caller's own).
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn date_order_is_scoped_to_the_caller(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_photos = root_id(&instance, &alice_me, "Photos").await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    for (name, when, angle) in [
        ("a-new.jpg", datetime!(2025-05-01 10:00), 0.55),
        ("a-old.jpg", datetime!(2021-05-01 10:00), 0.5),
    ] {
        let id = photo(&pool, &alice_me, alice_photos, name, when).await;
        embed(&pool, id, angle).await;
    }
    let before = search(&alice, "q=dog&order=date").await;
    assert_eq!(names(&before), ["a-new.jpg", "a-old.jpg"]);
    // Bob's exact matches would push Alice's past a shared relative cutoff.
    for index in 0..3 {
        let when = datetime!(2026-01-01 10:00) + time::Duration::days(index);
        let id = photo(&pool, &bob_me, bob_photos, &format!("b{index}.jpg"), when).await;
        embed(&pool, id, 0.0).await;
    }
    let after = search(&alice, "q=dog&order=date").await;
    assert_eq!(ids(&after), ids(&before));
    assert_eq!(
        names(&search(&bob, "q=dog&order=date").await),
        ["b2.jpg", "b1.jpg", "b0.jpg"]
    );
}
