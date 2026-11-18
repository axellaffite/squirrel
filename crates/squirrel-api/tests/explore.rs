mod common;

use axum_test::TestServer;
use common::files::root_id;
use common::photos::{Seed, photo, seed, sha_of};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn get(browser: &TestServer, url: &str) -> Value {
    let response = browser.get(url).await;
    response.assert_status_ok();
    response.json()
}

async fn set(pool: &PgPool, node: Uuid, sql: &str) {
    sqlx::query(&format!(
        "UPDATE media m SET {sql} FROM nodes n WHERE n.id = $1 AND m.sha256 = n.blob_sha256"
    ))
    .bind(node)
    .execute(pool)
    .await
    .unwrap();
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

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn tags_places_and_cameras_are_the_callers_own(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let old = photo(
        &pool,
        &alice_me,
        photos,
        "old.jpg",
        datetime!(2023-01-01 10:00),
    )
    .await;
    let new = photo(
        &pool,
        &alice_me,
        photos,
        "new.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    for id in [old, new] {
        tag(&pool, id, "dog").await;
        set(
            &pool,
            id,
            "place_country_code = 'FR', place_country = 'France', place_admin1 = 'Île-de-France', \
             place_name = 'Paris', camera_make = 'Canon', camera_model = 'EOS R6'",
        )
        .await;
    }
    let tags = get(&alice, "/api/tags").await;
    assert_eq!(tags[0]["tag"], "dog");
    assert_eq!(tags[0]["count"], 2);
    assert_eq!(tags[0]["cover"]["name"], "new.jpg");
    let places = get(&alice, "/api/places").await;
    assert_eq!(
        (
            &places[0]["country_code"],
            &places[0]["region"],
            &places[0]["city"],
            &places[0]["count"]
        ),
        (
            &json!("FR"),
            &json!("Île-de-France"),
            &json!("Paris"),
            &json!(2)
        )
    );
    // Located where the cover photo was taken (the offline locator's dot).
    set(&pool, old, "latitude = 45.76, longitude = 4.83").await;
    set(&pool, new, "latitude = 48.86, longitude = 2.35").await;
    let located = get(&alice, "/api/places").await;
    assert_eq!(
        located[0]["location"],
        json!({"latitude": 48.86, "longitude": 2.35, "altitude": null})
    );
    let countries = get(&alice, "/api/places?level=country").await;
    assert_eq!(countries[0]["city"], Value::Null);
    assert_eq!(countries[0]["country"], "France");
    assert_eq!(
        get(&alice, "/api/cameras").await,
        json!([{ "make": "Canon", "model": "EOS R6", "count": 2 }])
    );
    let details = get(&alice, &format!("/api/photos/{new}")).await;
    assert_eq!(details["tags"], json!(["dog"]));

    // Bob sees nothing of Alice's, even holding one of her blobs.
    for url in ["/api/tags", "/api/places", "/api/cameras"] {
        assert_eq!(get(&bob, url).await, json!([]), "{url}");
    }
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    let sha = sha_of(&pool, old).await;
    let copy = Seed {
        sha256: Some(&sha),
        ..Seed::new("copy.jpg", datetime!(2023-01-01 10:00))
    };
    seed(&pool, &bob_me, bob_photos, &copy).await;
    let bob_tags = get(&bob, "/api/tags").await;
    assert_eq!(bob_tags[0]["count"], 1);
    assert_eq!(bob_tags[0]["cover"]["name"], "copy.jpg");
    assert_eq!(
        get(&alice, "/api/tags").await[0]["count"],
        2,
        "Alice's count is unchanged"
    );
    browser_rejects_bad_level(&alice).await;
}

async fn browser_rejects_bad_level(browser: &TestServer) {
    assert!(
        browser
            .get("/api/places?level=planet")
            .await
            .status_code()
            .is_client_error()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn unknown_query_parameters_are_refused(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    for url in [
        "/api/tags?x=1",
        "/api/places?level=city&contry=FR",
        "/api/cameras?limit=5",
        "/api/photos?contry=FR",
        "/api/photos?month=2024-01&limt=5",
        "/api/photos/months?q=dog",
        "/api/search/photos?q=dog&contry=FR",
    ] {
        browser
            .get(url)
            .await
            .assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
    for url in [
        "/api/places?level=country",
        "/api/photos?month=2024-01&limit=5&tag=dog",
    ] {
        get(&browser, url).await;
    }
}
