//! The blur filter: a per-user rule whose matching photos clients blur.

mod common;

use common::{admin, invited, photo, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::album_rules::{AlbumRules, Match};
use squirrel_db::assets::AssetKind;
use squirrel_db::blur_filter::{self, BlurFilter};
use squirrel_db::people;
use time::macros::datetime;
use uuid::Uuid;

async fn person(pool: &PgPool, owner: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn face(pool: &PgPool, owner: Uuid, node: Uuid, person: Uuid) {
    sqlx::query(
        "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, landmarks, \
                            score, embedding, person_id) \
         SELECT $1, $2, blob_sha256, 'yunet-sface-2021dec', 0.1, 0.1, 0.2, 0.2, \
                array_fill(0.2::real, ARRAY[10]), 0.5, \
                array_fill(0.0::real, ARRAY[128])::halfvec, $4 \
           FROM nodes WHERE id = $3",
    )
    .bind(Uuid::now_v7())
    .bind(owner)
    .bind(node)
    .bind(person)
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

/// Alice: a beach photo with Léa, a mountain photo, a screenshot. Bob: a beach photo.
struct World {
    alice: Uuid,
    bob: Uuid,
    lea: Uuid,
    beach: Uuid,
    mountain: Uuid,
    shot: Uuid,
    bob_beach: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let alice_user = admin(pool).await;
    let alice = alice_user.id;
    let bob = invited(pool, &alice_user, "bob").await.id;
    let photos = root(pool, alice, "Photos").await.id;
    let beach = photo(
        pool,
        alice,
        photos,
        "beach.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let mountain = photo(
        pool,
        alice,
        photos,
        "mountain.jpg",
        datetime!(2024-08-01 10:00),
    )
    .await;
    let shot = photo(pool, alice, photos, "shot.png", datetime!(2024-08-02 10:00)).await;
    sqlx::query("UPDATE assets SET kind = 'screenshot' WHERE node_id = $1")
        .bind(shot)
        .execute(pool)
        .await
        .unwrap();
    let lea = person(pool, alice).await;
    face(pool, alice, beach, lea).await;
    tag(pool, beach, "beach").await;
    tag(pool, mountain, "mountain").await;
    let bob_photos = root(pool, bob, "Photos").await.id;
    let bob_beach = photo(
        pool,
        bob,
        bob_photos,
        "bob.jpg",
        datetime!(2024-07-03 10:00),
    )
    .await;
    tag(pool, bob_beach, "beach").await;
    World {
        alice,
        bob,
        lea,
        beach,
        mountain,
        shot,
        bob_beach,
    }
}

fn filter(rules: AlbumRules) -> BlurFilter {
    BlurFilter {
        enabled: true,
        rules,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn without_a_filter_nothing_is_blurred(pool: PgPool) {
    let w = world(&pool).await;
    assert_eq!(blur_filter::find(&pool, w.alice).await.unwrap(), None);
    assert!(
        blur_filter::blurred_ids(&pool, w.alice)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_filter_blurs_only_its_owners_matching_photos(pool: PgPool) {
    let w = world(&pool).await;
    let saved = filter(AlbumRules {
        tags: vec!["beach".into(), "mountain".into()],
        ..AlbumRules::default()
    });
    blur_filter::save(&pool, w.alice, &saved).await.unwrap();
    assert_eq!(
        blur_filter::blurred_ids(&pool, w.alice).await.unwrap(),
        [w.mountain, w.beach],
        "newest first, the screenshot is not tagged"
    );
    assert!(
        blur_filter::blurred_ids(&pool, w.bob)
            .await
            .unwrap()
            .is_empty(),
        "Bob's beach photo is not Alice's to blur, and Bob has no filter"
    );
    assert_eq!(
        blur_filter::find(&pool, w.alice).await.unwrap(),
        Some(saved)
    );
    let _ = w.bob_beach;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn conditions_combine_as_in_dynamic_albums(pool: PgPool) {
    let w = world(&pool).await;
    let people_and_tag = filter(AlbumRules {
        people: vec![w.lea],
        tags: vec!["beach".into()],
        tags_match: Match::All,
        ..AlbumRules::default()
    });
    blur_filter::save(&pool, w.alice, &people_and_tag)
        .await
        .unwrap();
    assert_eq!(
        blur_filter::blurred_ids(&pool, w.alice).await.unwrap(),
        [w.beach]
    );
    let screenshots = filter(AlbumRules {
        kind: Some(AssetKind::Screenshot),
        ..AlbumRules::default()
    });
    blur_filter::save(&pool, w.alice, &screenshots)
        .await
        .unwrap();
    assert_eq!(
        blur_filter::blurred_ids(&pool, w.alice).await.unwrap(),
        [w.shot]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_disabled_filter_is_kept_but_blurs_nothing(pool: PgPool) {
    let w = world(&pool).await;
    let off = BlurFilter {
        enabled: false,
        rules: AlbumRules {
            tags: vec!["beach".into()],
            ..AlbumRules::default()
        },
    };
    blur_filter::save(&pool, w.alice, &off).await.unwrap();
    assert!(
        blur_filter::blurred_ids(&pool, w.alice)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(blur_filter::find(&pool, w.alice).await.unwrap(), Some(off));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashed_photos_are_not_listed_and_delete_forgets(pool: PgPool) {
    let w = world(&pool).await;
    let beach = filter(AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    });
    blur_filter::save(&pool, w.alice, &beach).await.unwrap();
    raw_trash(&pool, w.beach).await;
    assert!(
        blur_filter::blurred_ids(&pool, w.alice)
            .await
            .unwrap()
            .is_empty()
    );
    blur_filter::delete(&pool, w.alice).await.unwrap();
    assert_eq!(blur_filter::find(&pool, w.alice).await.unwrap(), None);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn merging_people_rewrites_the_filter(pool: PgPool) {
    let w = world(&pool).await;
    let kept = person(&pool, w.alice).await;
    let lea = filter(AlbumRules {
        people: vec![w.lea],
        ..AlbumRules::default()
    });
    blur_filter::save(&pool, w.alice, &lea).await.unwrap();
    people::merge(&pool, w.alice, kept, &[w.lea]).await.unwrap();
    let stored = blur_filter::find(&pool, w.alice).await.unwrap().unwrap();
    assert_eq!(stored.rules.people, [kept]);
    assert_eq!(
        blur_filter::blurred_ids(&pool, w.alice).await.unwrap(),
        [w.beach]
    );
}
