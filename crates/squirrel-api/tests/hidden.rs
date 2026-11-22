//! A photo hidden as a duplicate appears in no photo listing: one test per
//! route. Its owner can still open it by id; another user is never affected.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::devices::{access, bearer, device_login};
use common::files::{root_id, user_id};
use common::photos::{Seed, photo, seed, sha_of};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::ml::TextEncoder;
use squirrel_ml::client::{MlBackend, MlClient};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use time::macros::datetime;
use uuid::Uuid;

/// Alice's kept photo and its hidden duplicate, in one resolved group.
struct World {
    instance: common::Instance,
    browser: TestServer,
    me: Value,
    kept: Uuid,
    hidden: Uuid,
}

async fn world(pool: &PgPool) -> World {
    world_with(common::Instance::new(pool.clone()), pool).await
}

async fn world_with(instance: common::Instance, pool: &PgPool) -> World {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    let kept = photo(pool, &me, photos, "kept.jpg", when).await;
    let hidden = photo(pool, &me, photos, "hidden.jpg", when).await;
    hide_as_duplicate(pool, user_id(&me), kept, hidden).await;
    World {
        instance,
        browser,
        me,
        kept,
        hidden,
    }
}

/// What "keep `kept`" records.
async fn hide_as_duplicate(pool: &PgPool, owner: Uuid, kept: Uuid, hidden: Uuid) {
    let group = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO duplicate_groups (id, owner_id, anchor_phash, anchor_taken_at, \
                                       kept_node_id, resolved_at) \
         VALUES ($1, $2, 0, '2024-07-01 10:00', $3, now())",
    )
    .bind(group)
    .bind(owner)
    .bind(kept)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO duplicate_members (node_id, owner_id, group_id, distance) \
         VALUES ($1, $3, $4, 0), ($2, $3, $4, 1)",
    )
    .bind(kept)
    .bind(hidden)
    .bind(owner)
    .bind(group)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE assets SET hidden_at = now(), hidden_reason = 'duplicate' WHERE node_id = $1",
    )
    .bind(hidden)
    .execute(pool)
    .await
    .unwrap();
}

async fn get(browser: &TestServer, url: &str) -> Value {
    let response = browser.get(url).await;
    response.assert_status_ok();
    response.json()
}

fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

fn only(id: Uuid) -> Vec<String> {
    vec![id.to_string()]
}

/// Runs `sql` with `$1` bound to every photo of the world.
async fn for_both(pool: &PgPool, w: &World, sql: &str) {
    for node in [w.kept, w.hidden] {
        sqlx::query(sql).bind(node).execute(pool).await.unwrap();
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_timeline_leaves_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    assert_eq!(ids(&get(&w.browser, "/api/photos").await), only(w.kept));
    let july = get(&w.browser, "/api/photos?month=2024-07").await;
    assert_eq!(ids(&july), only(w.kept));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn month_counts_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    assert_eq!(
        get(&w.browser, "/api/photos/months").await,
        json!([{ "month": "2024-07", "count": 1 }])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_hidden_photo_still_opens_by_id_for_its_owner_only(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    let kept = photo(&pool, &alice_me, photos, "kept.jpg", when).await;
    let hidden = photo(&pool, &alice_me, photos, "hidden.jpg", when).await;
    hide_as_duplicate(&pool, user_id(&alice_me), kept, hidden).await;
    let details = get(&alice, &format!("/api/photos/{hidden}")).await;
    assert_eq!(details["asset"]["name"], "hidden.jpg");
    alice
        .get(&format!("/api/photos/{hidden}/faces"))
        .await
        .assert_status_ok();
    bob.get(&format!("/api/photos/{hidden}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn albums_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    let created: Value = w
        .browser
        .post("/api/albums")
        .add_header(common::CSRF, common::csrf(&w.me))
        .json(&json!({ "name": "Trip", "node_ids": [w.hidden, w.kept] }))
        .await
        .json();
    let album = created["id"].as_str().unwrap().to_owned();
    let items = get(&w.browser, &format!("/api/albums/{album}/items")).await;
    assert_eq!(ids(&items), only(w.kept));
    let listed = get(&w.browser, "/api/albums").await;
    let entry = &listed.as_array().unwrap()[0];
    assert_eq!(entry["item_count"], 1);
    assert_eq!(entry["cover"]["id"], json!(w.kept));
}

/// Every query encodes to the same vector as every photo.
fn encoder() -> TextEncoder {
    TextEncoder::new(MlClient::new(MlBackend::Fake(Arc::new(|op: &MlOp| {
        let MlOp::ClipText { texts } = op else {
            return Ok(MlBody::Ok(MlOutput::Pong {}));
        };
        Ok(MlBody::Ok(MlOutput::Embeddings {
            embeddings: texts.iter().map(|_| unit()).collect(),
        }))
    }))))
}

fn unit() -> Vec<f32> {
    let mut v = vec![0.0; squirrel_ml::clip::EMBEDDING_DIM];
    v[0] = 1.0;
    v
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn search_leaves_hidden_photos_out_in_both_orders(pool: PgPool) {
    let instance = common::Instance::new(pool.clone()).with_text_encoder(encoder());
    let w = world_with(instance, &pool).await;
    for node in [w.kept, w.hidden] {
        let sha = sha_of(&pool, node).await;
        squirrel_db::clip::store_embedding(&pool, &sha, squirrel_ml::CLIP_MODEL_ID, &unit())
            .await
            .unwrap();
    }
    for order in ["relevance", "date"] {
        let found = get(
            &w.browser,
            &format!("/api/search/photos?q=dog&order={order}"),
        )
        .await;
        assert_eq!(ids(&found), only(w.kept), "{order}");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn people_and_their_faces_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    let owner = user_id(&w.me);
    let person = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id, name) VALUES ($1, $2, 'Ada')")
        .bind(person)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    for node in [w.kept, w.hidden] {
        sqlx::query(
            "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, \
                                landmarks, score, embedding, person_id) \
             SELECT gen_random_uuid(), $2, blob_sha256, 'yunet-sface-2021dec', 0.1, 0.1, 0.2, \
                    0.2, array_fill(0.2::real, ARRAY[10]), 0.9, \
                    array_fill(0.0::real, ARRAY[128])::halfvec, $3 \
               FROM nodes WHERE id = $1",
        )
        .bind(node)
        .bind(owner)
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
    }
    let people = get(&w.browser, "/api/people").await;
    assert_eq!(people[0]["face_count"], 1, "{people}");
    let faces = get(&w.browser, &format!("/api/people/{person}/faces")).await;
    let photos: Vec<&Value> = faces["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["photo_id"])
        .collect();
    assert_eq!(photos, vec![&json!(w.kept)]);
    let filtered = get(&w.browser, &format!("/api/photos?person={person}")).await;
    assert_eq!(ids(&filtered), only(w.kept));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn places_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    let set_place = "UPDATE media SET latitude = 48.85, longitude = 2.35, \
         place_name = 'Paris', place_admin1 = 'Île-de-France', place_country = 'France', \
         place_country_code = 'FR' WHERE sha256 = (SELECT blob_sha256 FROM nodes WHERE id = $1)";
    for_both(&pool, &w, set_place).await;
    let places = get(&w.browser, "/api/places").await;
    assert_eq!(places[0]["count"], 1, "{places}");
    assert_eq!(places[0]["cover"]["id"], json!(w.kept));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn tags_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    let tag = "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         SELECT blob_sha256, 'dog', 'clip', 0.3, 2 FROM nodes WHERE id = $1";
    for_both(&pool, &w, tag).await;
    let tags = get(&w.browser, "/api/tags").await;
    assert_eq!(tags[0]["count"], 1, "{tags}");
    assert_eq!(tags[0]["cover"]["id"], json!(w.kept));
    let filtered = get(&w.browser, "/api/photos?tag=dog").await;
    assert_eq!(ids(&filtered), only(w.kept));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn cameras_leave_hidden_photos_out(pool: PgPool) {
    let w = world(&pool).await;
    let camera = "UPDATE media SET camera_make = 'Fujifilm', camera_model = 'X100' \
         WHERE sha256 = (SELECT blob_sha256 FROM nodes WHERE id = $1)";
    for_both(&pool, &w, camera).await;
    let cameras = get(&w.browser, "/api/cameras").await;
    assert_eq!(cameras[0]["count"], 1, "{cameras}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_phone_does_not_list_hidden_photos_nor_upload_them_again(pool: PgPool) {
    let w = world(&pool).await;
    let phone = device_login(&w.instance, "alice", "Pixel").await;
    let (name, value) = bearer(&access(&phone));
    let photos: Value = w
        .instance
        .browser()
        .get("/api/photos")
        .add_header(name.clone(), value.clone())
        .await
        .json();
    assert_eq!(ids(&photos), only(w.kept));
    let months: Value = w
        .instance
        .browser()
        .get("/api/photos/months")
        .add_header(name.clone(), value.clone())
        .await
        .json();
    assert_eq!(months[0]["count"], 1);
    // Hidden is still backed up: the phone must not upload it again.
    let sha = sha_of(&pool, w.hidden).await;
    let checked: Value = w
        .instance
        .browser()
        .post("/api/sync/check")
        .add_header(name, value)
        .json(&json!({ "items": [{ "device_asset_id": "images:1", "sha256": sha }] }))
        .await
        .json();
    assert_eq!(checked["missing"], json!([]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_users_hidden_photo_never_affects_the_caller(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let when = datetime!(2024-07-01 10:00);
    let alice_photos = root_id(&instance, &alice_me, "Photos").await;
    let kept = photo(&pool, &alice_me, alice_photos, "kept.jpg", when).await;
    let hidden = photo(&pool, &alice_me, alice_photos, "hidden.jpg", when).await;
    hide_as_duplicate(&pool, user_id(&alice_me), kept, hidden).await;
    // Bob holds the same bytes as Alice's hidden photo.
    let sha = sha_of(&pool, hidden).await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    let mine = seed(
        &pool,
        &bob_me,
        bob_photos,
        &Seed {
            sha256: Some(&sha),
            ..Seed::new("same.jpg", when)
        },
    )
    .await;
    assert_eq!(ids(&get(&bob, "/api/photos").await), only(mine));
    assert_eq!(ids(&get(&alice, "/api/photos").await), only(kept));
}
