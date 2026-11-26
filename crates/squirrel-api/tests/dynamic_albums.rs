//! Dynamic albums: albums whose photos come from rules.

mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{code, root_id, user_id};
use common::photos::photo;
use common::{CSRF, Instance, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn post(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .post(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

async fn patch(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .patch(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

async fn person(pool: &PgPool, owner: Uuid, photos: &[Uuid]) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    for node in photos {
        sqlx::query(
            "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, \
                                landmarks, score, embedding, person_id) \
             SELECT $1, $2, blob_sha256, 'yunet-sface-2021dec', 0.1, 0.1, 0.2, 0.2, \
                    array_fill(0.2::real, ARRAY[10]), 0.5, \
                    array_fill(0.0::real, ARRAY[128])::halfvec, $4 \
               FROM nodes WHERE id = $3",
        )
        .bind(Uuid::now_v7())
        .bind(owner)
        .bind(node)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    id
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
    lea: Uuid,
    tom: Uuid,
    lea_beach: Uuid,
    both: Uuid,
    tom_city: Uuid,
}

/// Alice: Léa at the beach, Léa and Tom at the beach, Tom in the city.
async fn world(pool: &PgPool) -> World {
    let instance = Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let lea_beach = photo(
        pool,
        &alice_me,
        photos,
        "lea_beach.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let both = photo(
        pool,
        &alice_me,
        photos,
        "both.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    let tom_city = photo(
        pool,
        &alice_me,
        photos,
        "tom_city.jpg",
        datetime!(2024-07-03 10:00),
    )
    .await;
    let owner = user_id(&alice_me);
    let lea = person(pool, owner, &[lea_beach, both]).await;
    let tom = person(pool, owner, &[both, tom_city]).await;
    tag(pool, lea_beach, "beach").await;
    tag(pool, both, "beach").await;
    World {
        alice,
        alice_me,
        bob,
        bob_me,
        lea,
        tom,
        lea_beach,
        both,
        tom_city,
    }
}

async fn create(w: &World, rules: Value) -> Value {
    let created = post(
        &w.alice,
        &w.alice_me,
        "/api/albums",
        json!({ "name": "Kids", "rules": rules }),
    )
    .await;
    created.assert_status(StatusCode::CREATED);
    created.json()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_dynamic_album_holds_the_matching_photos(pool: PgPool) {
    let w = world(&pool).await;
    let album = create(
        &w,
        json!({ "people": [w.lea, w.tom], "people_match": "all", "tags": ["beach"] }),
    )
    .await;
    assert_eq!(album["dynamic"], true);
    assert_eq!(album["item_count"], 1);
    assert_eq!(album["cover"]["id"], w.both.to_string());
    assert_eq!(album["rules"]["people_match"], "all");
    assert_eq!(album["rules"]["tags_match"], "any");
    assert_eq!(album["rules"]["taken_from"], Value::Null);
    let id = album["id"].as_str().unwrap();

    let items: Value = w.alice.get(&format!("/api/albums/{id}/items")).await.json();
    assert_eq!(names(&items), ["both.jpg"]);

    let listed: Value = w.alice.get("/api/albums").await.json();
    assert_eq!(listed[0]["dynamic"], true);
    let details: Value = w.alice.get(&format!("/api/photos/{}", w.both)).await.json();
    assert_eq!(details["albums"][0]["id"], id);

    // Any of the two, then: three photos, and the cover may move to the newest.
    let changed = patch(
        &w.alice,
        &w.alice_me,
        &format!("/api/albums/{id}"),
        json!({ "rules": { "people": [w.lea, w.tom] }, "cover_node_id": w.lea_beach }),
    )
    .await;
    changed.assert_status_ok();
    let changed: Value = changed.json();
    assert_eq!(changed["item_count"], 3);
    assert_eq!(changed["cover"]["id"], w.lea_beach.to_string());
    let items: Value = w.alice.get(&format!("/api/albums/{id}/items")).await.json();
    assert_eq!(names(&items), ["tom_city.jpg", "both.jpg", "lea_beach.jpg"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn its_photos_are_not_added_or_removed_by_hand(pool: PgPool) {
    let w = world(&pool).await;
    let album = create(&w, json!({ "tags": ["beach"] })).await;
    let id = album["id"].as_str().unwrap();
    for url in [
        format!("/api/albums/{id}/items"),
        format!("/api/albums/{id}/items/remove"),
    ] {
        let refused = post(
            &w.alice,
            &w.alice_me,
            &url,
            json!({ "node_ids": [w.tom_city] }),
        )
        .await;
        refused.assert_status(StatusCode::CONFLICT);
    }
    let manual: Value = post(
        &w.alice,
        &w.alice_me,
        "/api/albums",
        json!({ "name": "Manual" }),
    )
    .await
    .json();
    assert_eq!(manual["dynamic"], false);
    assert_eq!(manual["rules"], Value::Null);
    let refused = patch(
        &w.alice,
        &w.alice_me,
        &format!("/api/albums/{}", manual["id"].as_str().unwrap()),
        json!({ "rules": { "tags": ["beach"] } }),
    )
    .await;
    refused.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rules_are_checked(pool: PgPool) {
    let w = world(&pool).await;
    let bobs_person = person(&pool, user_id(&w.bob_me), &[]).await;
    let bad = [
        json!({ "name": "x", "rules": {} }),
        json!({ "name": "x", "rules": { "tags": ["beach"] }, "node_ids": [w.both] }),
        json!({ "name": "x", "rules": { "people": [bobs_person] } }),
        json!({ "name": "x", "rules": { "city": "Paris" } }),
        json!({ "name": "x", "rules": { "taken_from": "2024-02-01", "taken_to": "2024-01-01" } }),
        json!({ "name": "x", "rules": { "taken_from": "yesterday" } }),
    ];
    for body in bad {
        let refused = post(&w.alice, &w.alice_me, "/api/albums", body.clone()).await;
        let status = refused.status_code();
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "{body}: {status}"
        );
    }
    let refused = post(
        &w.alice,
        &w.alice_me,
        "/api/albums",
        json!({ "name": "x", "rules": { "people": [bobs_person] } }),
    )
    .await;
    assert_eq!(code(&refused), "validation_error");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_preview_shows_what_rules_would_hold(pool: PgPool) {
    let w = world(&pool).await;
    let preview = post(
        &w.alice,
        &w.alice_me,
        "/api/albums/preview",
        json!({ "people": [w.tom], "taken_to": "2024-07-02" }),
    )
    .await;
    preview.assert_status_ok();
    let preview: Value = preview.json();
    assert_eq!(preview["count"], 1);
    assert_eq!(preview["newest"][0]["id"], w.both.to_string());
    let empty = post(&w.alice, &w.alice_me, "/api/albums/preview", json!({})).await;
    empty.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shared_for_reading_without_the_rules(pool: PgPool) {
    let w = world(&pool).await;
    let album = create(&w, json!({ "people": [w.lea] })).await;
    let id = album["id"].as_str().unwrap();
    let write = post(
        &w.alice,
        &w.alice_me,
        "/api/shares",
        json!({ "album_id": id, "username": "bob", "permission": "write" }),
    )
    .await;
    write.assert_status(StatusCode::CONFLICT);
    let read = post(
        &w.alice,
        &w.alice_me,
        "/api/shares",
        json!({ "album_id": id, "username": "bob", "permission": "read" }),
    )
    .await;
    read.assert_status(StatusCode::CREATED);

    let seen: Value = w.bob.get(&format!("/api/albums/{id}")).await.json();
    assert_eq!(seen["dynamic"], true);
    assert_eq!(seen["rules"], Value::Null, "Alice's people stay hers");
    assert_eq!(seen["access"], "read");
    let items: Value = w.bob.get(&format!("/api/albums/{id}/items")).await.json();
    assert_eq!(names(&items), ["both.jpg", "lea_beach.jpg"]);
    w.bob
        .get(&format!("/api/nodes/{}", w.lea_beach))
        .await
        .assert_status_ok();
    w.bob
        .get(&format!("/api/nodes/{}", w.tom_city))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let edit = patch(
        &w.bob,
        &w.bob_me,
        &format!("/api/albums/{id}"),
        json!({ "rules": { "tags": ["beach"] } }),
    )
    .await;
    edit.assert_status(StatusCode::FORBIDDEN);
}
