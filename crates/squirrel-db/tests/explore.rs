mod common;

use common::{admin, invited, photo, photo_of_blob, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::explore::{self, PlaceLevel};
use time::macros::datetime;
use uuid::Uuid;

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

async fn sha(pool: &PgPool, node: Uuid) -> String {
    sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn tags_count_the_callers_live_photos_with_the_newest_as_cover(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let old = photo(
        &pool,
        alice.id,
        photos,
        "old.jpg",
        datetime!(2023-01-01 10:00),
    )
    .await;
    let new = photo(
        &pool,
        alice.id,
        photos,
        "new.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    let gone = photo(
        &pool,
        alice.id,
        photos,
        "gone.jpg",
        datetime!(2025-01-01 10:00),
    )
    .await;
    let beach = photo(
        &pool,
        alice.id,
        photos,
        "beach.jpg",
        datetime!(2024-02-01 10:00),
    )
    .await;
    for id in [old, new, gone] {
        tag(&pool, id, "dog").await;
    }
    tag(&pool, beach, "beach").await;
    raw_trash(&pool, gone).await;
    // Bob's copy of Alice's dog photo: counted for Bob, once.
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let bobs = photo_of_blob(
        &pool,
        bob.id,
        bob_photos,
        "copy.jpg",
        &sha(&pool, old).await,
        datetime!(2023-01-01 10:00),
    )
    .await;
    let tags = explore::tags(&pool, alice.id).await.unwrap();
    let summary: Vec<(&str, i64, Uuid)> = tags
        .iter()
        .map(|t| (t.tag.as_str(), t.count, t.cover))
        .collect();
    assert_eq!(summary, [("dog", 2, new), ("beach", 1, beach)]);
    let bob_tags = explore::tags(&pool, bob.id).await.unwrap();
    assert_eq!(bob_tags.len(), 1);
    assert_eq!((bob_tags[0].count, bob_tags[0].cover), (1, bobs));
    assert_eq!(
        explore::tags_of_photo(&pool, alice.id, new).await.unwrap(),
        ["dog"]
    );
    assert!(
        explore::tags_of_photo(&pool, bob.id, new)
            .await
            .unwrap()
            .is_empty(),
        "not Bob's node"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn places_group_by_city_or_country_and_cameras_skip_unknown_makes(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let when = datetime!(2024-01-01 10:00);
    let p1 = photo(&pool, alice.id, photos, "p1.jpg", when).await;
    let p2 = photo(&pool, alice.id, photos, "p2.jpg", when).await;
    let lyon = photo(&pool, alice.id, photos, "lyon.jpg", when).await;
    let rome = photo(&pool, alice.id, photos, "rome.jpg", when).await;
    photo(&pool, alice.id, photos, "nowhere.jpg", when).await;
    let fr_paris = "place_country_code = 'FR', place_country = 'France', \
                    place_admin1 = 'Île-de-France', place_name = 'Paris'";
    set(
        &pool,
        p1,
        &format!("{fr_paris}, camera_make = 'Canon', camera_model = 'EOS R6'"),
    )
    .await;
    set(
        &pool,
        p2,
        &format!("{fr_paris}, camera_make = 'Canon', camera_model = 'EOS R6'"),
    )
    .await;
    set(
        &pool,
        lyon,
        "place_country_code = 'FR', place_country = 'France', place_name = 'Lyon', \
                      camera_make = 'Apple'",
    )
    .await;
    set(
        &pool,
        rome,
        "place_country_code = 'IT', place_country = 'Italy', place_admin1 = 'Lazio', \
                      place_name = 'Rome'",
    )
    .await;
    let cities = explore::places(&pool, alice.id, PlaceLevel::City)
        .await
        .unwrap();
    let cities: Vec<(&str, Option<&str>, Option<&str>, i64)> = cities
        .iter()
        .map(|p| {
            (
                p.country_code.as_str(),
                p.region.as_deref(),
                p.city.as_deref(),
                p.count,
            )
        })
        .collect();
    assert_eq!(
        cities,
        [
            ("FR", Some("Île-de-France"), Some("Paris"), 2),
            ("FR", None, Some("Lyon"), 1),
            ("IT", Some("Lazio"), Some("Rome"), 1),
        ]
    );
    let countries = explore::places(&pool, alice.id, PlaceLevel::Country)
        .await
        .unwrap();
    let countries: Vec<(&str, Option<&str>, i64)> = countries
        .iter()
        .map(|p| (p.country_code.as_str(), p.country.as_deref(), p.count))
        .collect();
    assert_eq!(
        countries,
        [("FR", Some("France"), 3), ("IT", Some("Italy"), 1)]
    );
    let cameras = explore::cameras(&pool, alice.id).await.unwrap();
    let cameras: Vec<(&str, Option<&str>, i64)> = cameras
        .iter()
        .map(|c| (c.make.as_str(), c.model.as_deref(), c.count))
        .collect();
    assert_eq!(cameras, [("Canon", Some("EOS R6"), 2), ("Apple", None, 1)]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ml_counts_are_the_callers_pending_and_failed_images(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let when = datetime!(2024-01-01 10:00);
    let a = photo(&pool, alice.id, photos, "a.jpg", when).await;
    let b = photo(&pool, alice.id, photos, "b.jpg", when).await;
    photo(&pool, alice.id, photos, "c.jpg", when).await;
    set(&pool, a, "clip_state = 'ready'").await;
    set(&pool, b, "clip_state = 'failed'").await;
    let counts = explore::ml_counts(&pool, alice.id).await.unwrap();
    assert_eq!((counts.queued, counts.failed), (1, 1));
    // A trashed pending photo and Bob's own photos do not count for Alice.
    let gone = photo(&pool, alice.id, photos, "gone.jpg", when).await;
    raw_trash(&pool, gone).await;
    let bob = invited(&pool, &alice, "bob").await;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let bobs = photo(&pool, bob.id, bob_photos, "bob.jpg", when).await;
    set(&pool, bobs, "clip_state = 'failed'").await;
    let counts = explore::ml_counts(&pool, alice.id).await.unwrap();
    assert_eq!((counts.queued, counts.failed), (1, 1));
    let bob_counts = explore::ml_counts(&pool, bob.id).await.unwrap();
    assert_eq!((bob_counts.queued, bob_counts.failed), (0, 1));
}
