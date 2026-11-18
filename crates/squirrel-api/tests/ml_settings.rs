mod common;

use axum::http::StatusCode;
use common::files::root_id;
use common::photos::{Seed, photo, seed, sha_of};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = "yunet-sface-2021dec";

fn user_id(me: &Value) -> Uuid {
    me["user"]["id"]
        .as_str()
        .or(me["id"].as_str())
        .unwrap()
        .parse()
        .unwrap()
}

/// One stored face, a person and a scan for `owner` on `sha`, and a queued
/// clustering job.
async fn face_data(pool: &PgPool, owner: Uuid, sha: &str) {
    let mut embedding = vec![0.0f32; 128];
    embedding[0] = 1.0;
    let face = squirrel_db::faces::NewFace {
        bbox: [0.1, 0.1, 0.2, 0.2],
        landmarks: [0.15; 10],
        score: 0.9,
        embedding,
    };
    squirrel_db::faces::store_scan(pool, owner, sha, MODEL, &[face])
        .await
        .unwrap();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(Uuid::now_v7())
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO jobs (kind, payload, priority, dedupe_key) \
         VALUES ('cluster_faces', '{}', -35, 'cluster_faces:' || $1::text)",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn turning_faces_off_deletes_only_the_callers_face_data(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let (alice_id, bob_id) = (user_id(&alice_me), user_id(&bob_me));
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let node = photo(
        &pool,
        &alice_me,
        photos,
        "a.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    let sha = sha_of(&pool, node).await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    seed(
        &pool,
        &bob_me,
        bob_photos,
        &Seed {
            sha256: Some(&sha),
            ..Seed::new("b.jpg", datetime!(2024-01-01 10:00))
        },
    )
    .await;
    face_data(&pool, alice_id, &sha).await;
    face_data(&pool, bob_id, &sha).await;

    let settings: Value = alice.get("/api/me/ml-settings").await.json();
    assert_eq!(
        settings,
        json!({ "faces_enabled": true, "faces_count": 1, "people_count": 1 })
    );

    alice
        .put("/api/me/ml-settings")
        .json(&json!({ "faces_enabled": false }))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    let off = alice
        .put("/api/me/ml-settings")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .json(&json!({ "faces_enabled": false }))
        .await;
    off.assert_status_ok();
    assert_eq!(
        off.json::<Value>(),
        json!({ "faces_enabled": false, "faces_count": 0, "people_count": 0 })
    );
    let alice_left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM face_scans WHERE owner_id = $1) \
              + (SELECT count(*) FROM jobs WHERE dedupe_key = 'cluster_faces:' || $1::text)",
    )
    .bind(alice_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(alice_left, 0);
    let bob_settings: Value = bob.get("/api/me/ml-settings").await.json();
    assert_eq!(
        bob_settings,
        json!({ "faces_enabled": true, "faces_count": 1, "people_count": 1 })
    );
    let bob_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE dedupe_key = 'cluster_faces:' || $1::text",
    )
    .bind(bob_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bob_jobs, 1);

    let on = alice
        .put("/api/me/ml-settings")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .json(&json!({ "faces_enabled": true }))
        .await;
    on.assert_status_ok();
    let backfill: Vec<String> =
        sqlx::query_scalar("SELECT dedupe_key FROM jobs WHERE kind = 'face_backfill'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(backfill, [format!("face_backfill:{alice_id}")]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bad_bodies_are_refused(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let response = browser
        .put("/api/me/ml-settings")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&json!({ "faces_enabled": "nope" }))
        .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}
