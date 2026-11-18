mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files::{self, code, root_id};
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

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_describes_its_place_location_and_camera(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let tagged = photo(
        &pool,
        &alice_me,
        photos,
        "eiffel.jpg",
        datetime!(2024-07-14 18:30),
    )
    .await;
    sqlx::query(
        "UPDATE media SET latitude = 48.8584, longitude = 2.2945, altitude = 35.5, \
                camera_make = 'Canon', camera_model = 'EOS R6', lens_model = 'RF 35mm', \
                exposure_time = '1/250', f_number = 1.8, iso = 200, focal_length_mm = 35, \
                place_name = 'Paris', place_admin1 = 'Île-de-France', place_country = 'France', \
                place_country_code = 'FR', geocoded_with = 'x' \
          WHERE sha256 = $1",
    )
    .bind(sha_of(&pool, tagged).await)
    .execute(&pool)
    .await
    .unwrap();
    let plain = photo(
        &pool,
        &alice_me,
        photos,
        "plain.jpg",
        datetime!(2024-07-15 10:00),
    )
    .await;

    let details = get(&alice, &format!("/api/photos/{tagged}")).await;
    assert_eq!(details["asset"]["id"], tagged.to_string());
    assert_eq!(details["asset"]["name"], "eiffel.jpg");
    assert_eq!(details["size"], 10);
    assert_eq!(details["parent_id"], photos.to_string());
    assert_eq!(
        details["place"],
        json!({ "name": "Paris", "admin1": "Île-de-France", "country": "France", "country_code": "FR" })
    );
    assert_eq!(
        details["location"],
        json!({ "latitude": 48.8584, "longitude": 2.2945, "altitude": 35.5 })
    );
    assert_eq!(
        details["camera"],
        json!({
            "make": "Canon", "model": "EOS R6", "lens": "RF 35mm", "exposure_time": "1/250",
            "f_number": 1.8, "iso": 200, "focal_length_mm": 35.0
        })
    );
    assert_eq!(details["albums"], json!([]));

    let bare = get(&alice, &format!("/api/photos/{plain}")).await;
    assert_eq!(
        (&bare["place"], &bare["location"], &bare["camera"]),
        (&Value::Null, &Value::Null, &Value::Null)
    );

    let stolen = bob.get(&format!("/api/photos/{tagged}")).await;
    stolen.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(code(&stolen), "not_found");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_live_photos_have_details(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let docs = root_id(&instance, &me, "Documents").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let pdf = files::upload(&browser, &me, docs, "a.pdf", b"%PDF-1.4 tiny").await;
    let trashed = photo(&pool, &me, photos, "gone.jpg", datetime!(2024-07-14 12:00)).await;
    browser
        .delete(&format!("/api/nodes/{trashed}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for id in [
        pdf["id"].as_str().unwrap().to_owned(),
        trashed.to_string(),
        photos.to_string(),
        uuid::Uuid::now_v7().to_string(),
    ] {
        let response = browser.get(&format!("/api/photos/{id}")).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
    browser
        .get("/api/photos/not-a-uuid")
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn media_status_counts_the_callers_files(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let docs = root_id(&instance, &alice_me, "Documents").await;
    let at = datetime!(2024-07-14 12:00);
    // Uploaded, not processed yet: no media row.
    files::upload(
        &alice,
        &alice_me,
        photos,
        "new.jpg",
        b"\xFF\xD8\xFF\xE0 new photo",
    )
    .await;
    files::upload(&alice, &alice_me, docs, "notes.txt", b"plain notes").await;
    files::upload(&alice, &alice_me, docs, "a.zip", b"PK\x03\x04 not media").await;
    photo(&pool, &alice_me, photos, "done.jpg", at).await;
    for (name, state) in [("busy.jpg", "pending"), ("broken.jpg", "failed")] {
        seed(
            &pool,
            &alice_me,
            photos,
            &Seed {
                thumbs_state: state,
                ..Seed::new(name, at)
            },
        )
        .await;
    }
    let trashed = seed(
        &pool,
        &alice_me,
        photos,
        &Seed {
            thumbs_state: "failed",
            ..Seed::new("trashed.jpg", at)
        },
    )
    .await;
    alice
        .delete(&format!("/api/nodes/{trashed}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let no_ml = json!({ "state": "unavailable", "reason": "no worker has reported" });
    assert_eq!(
        get(&alice, "/api/media/status").await,
        json!({ "queued": 2, "processing": 1, "failed": 1, "ml": no_ml })
    );
    assert_eq!(
        get(&bob, "/api/media/status").await,
        json!({ "queued": 0, "processing": 0, "failed": 0, "ml": no_ml })
    );
    squirrel_db::ml_status::report(&pool, "box", squirrel_db::ml_status::MlState::Ready, None)
        .await
        .unwrap();
    assert_eq!(
        get(&bob, "/api/media/status").await["ml"],
        json!({ "state": "ready", "reason": null })
    );
    common::server(pool)
        .get("/api/media/status")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn cursors_from_before_postgres_time_are_a_bad_request(pool: PgPool) {
    use base64::Engine;
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    let at = datetime!(-5000-01-01 00:00 UTC).unix_timestamp_nanos();
    let cursor = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{at}\n{}", uuid::Uuid::now_v7()));
    for url in [
        format!("/api/photos?cursor={cursor}"),
        format!("/api/trash?cursor={cursor}"),
    ] {
        let response = browser.get(&url).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        assert_eq!(code(&response), "bad_request", "{url}");
    }
}

async fn describe(pool: &PgPool, node: uuid::Uuid, sql: &str) {
    sqlx::query(&format!(
        "UPDATE media m SET {sql} FROM nodes n WHERE n.id = $1 AND m.sha256 = n.blob_sha256"
    ))
    .bind(node)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_timeline_and_its_months_take_filters(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let paris = photo(&pool, &me, photos, "paris.jpg", datetime!(2024-07-14 10:00)).await;
    let lyon = photo(&pool, &me, photos, "lyon.jpg", datetime!(2024-08-01 10:00)).await;
    describe(
        &pool,
        paris,
        "place_country_code = 'FR', place_admin1 = 'Île-de-France', place_name = 'Paris', \
         camera_make = 'Canon', camera_model = 'EOS R6'",
    )
    .await;
    describe(
        &pool,
        lyon,
        "place_country_code = 'FR', place_name = 'Lyon'",
    )
    .await;
    sqlx::query(
        "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         SELECT blob_sha256, 'dog', 'clip', 0.3, 2 FROM nodes WHERE id = $1",
    )
    .bind(lyon)
    .execute(&pool)
    .await
    .unwrap();

    let page = |q: &'static str| {
        let browser = &browser;
        async move { names(&get(browser, &format!("/api/photos?{q}")).await).join(",") }
    };
    assert_eq!(page("country=FR").await, "lyon.jpg,paris.jpg");
    assert_eq!(
        page("country=FR&region=%C3%8Ele-de-France&city=Paris").await,
        "paris.jpg"
    );
    assert_eq!(
        page("camera_make=Canon&camera_model=EOS%20R6").await,
        "paris.jpg"
    );
    assert_eq!(page("tag=dog").await, "lyon.jpg");
    assert_eq!(page("tag=dog&camera_make=Canon").await, "");
    assert_eq!(page("kind=screenshot").await, "");
    assert_eq!(page("tag=dog&month=2024-08").await, "lyon.jpg");
    let months = get(&browser, "/api/photos/months?tag=dog").await;
    assert_eq!(months, json!([{ "month": "2024-08", "count": 1 }]));
    for bad in [
        "/api/photos?city=Paris",
        "/api/photos?camera_model=X",
        "/api/photos?tag=Not%20a%20slug",
        "/api/photos?kind=video",
        "/api/photos/months?region=Bretagne",
    ] {
        let status = browser.get(bad).await.status_code();
        assert!(status.is_client_error(), "{bad}: {status}");
    }
}
