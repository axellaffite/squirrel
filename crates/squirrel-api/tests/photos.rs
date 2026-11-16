mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files::{code, root_id};
use common::photos::{Seed, photo, seed, sha_of};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;

async fn get(browser: &TestServer, url: &str) -> Value {
    let response = browser.get(url).await;
    response.assert_status_ok();
    response.json()
}

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pages_the_timeline_newest_first(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    photo(&pool, &me, photos, "a.jpg", datetime!(2024-06-01 10:00)).await;
    photo(&pool, &me, photos, "b.jpg", datetime!(2024-07-14 18:30:05)).await;
    photo(&pool, &me, photos, "c.jpg", datetime!(2024-07-20 09:00)).await;

    let first = get(&browser, "/api/photos?limit=2").await;
    assert_eq!(names(&first), ["c.jpg", "b.jpg"]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = get(&browser, &format!("/api/photos?limit=2&cursor={cursor}")).await;
    assert_eq!(names(&second), ["a.jpg"]);
    assert_eq!(second["next_cursor"], Value::Null);

    let b = &first["items"][1];
    assert_eq!(b["taken_at"], "2024-07-14T18:30:05");
    assert_eq!(b["taken_offset"], "+02:00");
    assert_eq!(b["kind"], "photo");
    assert_eq!(b["mime"], "image/jpeg");
    assert_eq!(
        (b["width"].as_u64(), b["height"].as_u64()),
        (Some(4), Some(3))
    );
    assert_eq!(b["blurhash"], "LEHV6nWB2yk8");
    assert_eq!(b["thumbnail"], json!({ "state": "ready", "version": 1 }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn filters_by_month_and_counts_months(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    photo(&pool, &me, photos, "june.jpg", datetime!(2024-06-30 23:59)).await;
    photo(&pool, &me, photos, "july.jpg", datetime!(2024-07-01 00:00)).await;
    seed(
        &pool,
        &me,
        photos,
        &Seed {
            thumbs_state: "pending",
            ..Seed::new("july2.jpg", datetime!(2024-07-31 23:59))
        },
    )
    .await;

    let july = get(&browser, "/api/photos?month=2024-07").await;
    assert_eq!(names(&july), ["july2.jpg", "july.jpg"]);
    assert_eq!(july["items"][0]["thumbnail"]["state"], "pending");
    assert_eq!(
        get(&browser, "/api/photos/months").await,
        json!([{ "month": "2024-07", "count": 2 }, { "month": "2024-06", "count": 1 }])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bad_months_and_cursors_are_refused(pool: PgPool) {
    let browser = common::server(pool);
    common::setup_admin(&browser, "alice").await;
    for month in ["2024-13", "x", "2024-7", "2024-00"] {
        let response = browser.get(&format!("/api/photos?month={month}")).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{month}");
    }
    let response = browser.get("/api/photos?cursor=garbage").await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(code(&response), "bad_request");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_timeline_requires_a_session(pool: PgPool) {
    let browser = common::server(pool);
    for url in ["/api/photos", "/api/photos/months"] {
        let response = browser.get(url).await;
        response.assert_status(StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn users_see_only_their_own_live_photos(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_photos = root_id(&instance, &alice_me, "Photos").await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    let at = datetime!(2024-07-14 12:00);
    let mine = photo(&pool, &alice_me, alice_photos, "alice.jpg", at).await;
    // Bob holds the same content (deduplicated blob) under his own name.
    let sha = sha_of(&pool, mine).await;
    seed(
        &pool,
        &bob_me,
        bob_photos,
        &Seed {
            sha256: Some(&sha),
            ..Seed::new("bob.jpg", at)
        },
    )
    .await;
    let trashed = photo(&pool, &alice_me, alice_photos, "gone.jpg", at).await;
    alice
        .delete(&format!("/api/nodes/{trashed}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    assert_eq!(names(&get(&alice, "/api/photos").await), ["alice.jpg"]);
    assert_eq!(names(&get(&bob, "/api/photos").await), ["bob.jpg"]);
    assert_eq!(
        get(&bob, "/api/photos/months").await,
        json!([{ "month": "2024-07", "count": 1 }])
    );
    assert_eq!(
        get(&alice, "/api/photos/months").await,
        json!([{ "month": "2024-07", "count": 1 }])
    );
}
