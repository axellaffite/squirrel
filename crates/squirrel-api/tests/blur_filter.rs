//! The blur filter: the caller's photos matching it are listed for clients to blur.

mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{root_id, user_id};
use common::photos::photo;
use common::{CSRF, Instance, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn put(browser: &TestServer, me: &Value, body: Value) -> TestResponse {
    browser
        .put("/api/me/blur-filter")
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

async fn blurred(browser: &TestServer) -> Vec<Uuid> {
    let response = browser.get("/api/photos/blurred").await;
    response.assert_status_ok();
    let body: Value = response.json();
    body["node_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().parse().unwrap())
        .collect()
}

async fn tag(pool: &PgPool, node: Uuid, tag: &str) {
    sqlx::query(
        "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         SELECT blob_sha256, $2, 'clip', 0.3, 2 FROM nodes WHERE id = $1",
    )
    .bind(node)
    .bind(tag)
    .execute(pool)
    .await
    .unwrap();
}

struct World {
    alice: TestServer,
    alice_me: Value,
    bob: TestServer,
    bob_me: Value,
    beach: Uuid,
    city: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let instance = Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let beach = photo(
        pool,
        &alice_me,
        photos,
        "beach.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let city = photo(
        pool,
        &alice_me,
        photos,
        "city.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    tag(pool, beach, "beach").await;
    World {
        alice,
        alice_me,
        bob,
        bob_me,
        beach,
        city,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn without_a_filter_nothing_is_blurred(pool: PgPool) {
    let w = world(&pool).await;
    let filter: Value = w.alice.get("/api/me/blur-filter").await.json();
    assert_eq!(filter, json!({ "enabled": false, "rules": null }));
    assert!(blurred(&w.alice).await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_saved_filter_lists_the_matching_photos(pool: PgPool) {
    let w = world(&pool).await;
    let saved = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": { "tags": ["beach"] } }),
    )
    .await;
    saved.assert_status_ok();
    let body: Value = saved.json();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["rules"]["tags"], json!(["beach"]));
    let read: Value = w.alice.get("/api/me/blur-filter").await.json();
    assert_eq!(read, body);
    assert_eq!(blurred(&w.alice).await, [w.beach]);
    assert!(
        blurred(&w.bob).await.is_empty(),
        "the filter is Alice's own"
    );
    let _ = (w.city, &w.bob_me);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn turning_off_keeps_the_rules_and_null_forgets_them(pool: PgPool) {
    let w = world(&pool).await;
    let rules = json!({ "tags": ["beach"] });
    put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": rules }),
    )
    .await
    .assert_status_ok();
    let off: Value = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": false, "rules": rules }),
    )
    .await
    .json();
    assert_eq!(off["enabled"], false);
    assert_eq!(off["rules"]["tags"], json!(["beach"]));
    assert!(blurred(&w.alice).await.is_empty());
    let forgotten: Value = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": null }),
    )
    .await
    .json();
    assert_eq!(forgotten, json!({ "enabled": false, "rules": null }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rules_are_checked_as_album_rules(pool: PgPool) {
    let w = world(&pool).await;
    let empty = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": {} }),
    )
    .await;
    empty.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let not_a_slug = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": { "tags": ["Not A Slug"] } }),
    )
    .await;
    not_a_slug.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let bobs_person = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(bobs_person)
        .bind(user_id(&w.bob_me))
        .execute(&pool)
        .await
        .unwrap();
    let foreign = put(
        &w.alice,
        &w.alice_me,
        json!({ "enabled": true, "rules": { "people": [bobs_person] } }),
    )
    .await;
    foreign.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn changing_the_filter_needs_the_csrf_token(pool: PgPool) {
    let w = world(&pool).await;
    let response = w
        .alice
        .put("/api/me/blur-filter")
        .json(&json!({ "enabled": true, "rules": { "tags": ["beach"] } }))
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}
