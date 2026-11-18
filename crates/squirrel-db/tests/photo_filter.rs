mod common;

use common::{admin, invited, photo, photo_of_blob, root};
use sqlx::PgPool;
use squirrel_db::assets::AssetKind;
use squirrel_db::photo_filter::PhotoFilter;
use squirrel_db::timeline::{self, Filter, TimelineCursor};
use time::macros::datetime;
use uuid::Uuid;

/// Sets what geocoding and EXIF would have recorded for a photo's blob.
async fn describe(pool: &PgPool, node: Uuid, place: (&str, &str, &str), make: &str, model: &str) {
    sqlx::query(
        "UPDATE media m SET place_country_code = $2, place_admin1 = $3, place_name = $4, \
                camera_make = $5, camera_model = $6 \
          FROM nodes n WHERE n.id = $1 AND m.sha256 = n.blob_sha256",
    )
    .bind(node)
    .bind(place.0)
    .bind(place.1)
    .bind(place.2)
    .bind(make)
    .bind(model)
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

async fn names(pool: &PgPool, owner: Uuid, filter: &PhotoFilter) -> Vec<String> {
    timeline::page(pool, owner, Filter::All, filter, None, 100)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect()
}

struct Library {
    alice: Uuid,
    bob: Uuid,
}

/// Alice: a Paris dog (Canon), a Lyon beach (Canon), a Paris screenshot
/// (Apple). Bob holds the same blob as Alice's Paris dog.
async fn library(pool: &PgPool) -> Library {
    let alice = admin(pool).await;
    let bob = invited(pool, &alice, "bob").await;
    let photos = root(pool, alice.id, "Photos").await.id;
    let dog = photo(
        pool,
        alice.id,
        photos,
        "dog.jpg",
        datetime!(2024-07-14 10:00),
    )
    .await;
    let beach = photo(
        pool,
        alice.id,
        photos,
        "beach.jpg",
        datetime!(2024-08-01 10:00),
    )
    .await;
    let shot = photo(
        pool,
        alice.id,
        photos,
        "shot.png",
        datetime!(2024-08-02 10:00),
    )
    .await;
    describe(
        pool,
        dog,
        ("FR", "Île-de-France", "Paris"),
        "Canon",
        "EOS R6",
    )
    .await;
    describe(
        pool,
        beach,
        ("FR", "Auvergne-Rhône-Alpes", "Lyon"),
        "Canon",
        "EOS R6",
    )
    .await;
    describe(
        pool,
        shot,
        ("FR", "Île-de-France", "Paris"),
        "Apple",
        "iPhone 15",
    )
    .await;
    sqlx::query("UPDATE assets SET kind = 'screenshot' WHERE node_id = $1")
        .bind(shot)
        .execute(pool)
        .await
        .unwrap();
    tag(pool, dog, "dog").await;
    tag(pool, beach, "beach").await;
    let bob_photos = root(pool, bob.id, "Photos").await.id;
    let sha: String = sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(dog)
        .fetch_one(pool)
        .await
        .unwrap();
    photo_of_blob(
        pool,
        bob.id,
        bob_photos,
        "bobs-dog.jpg",
        &sha,
        datetime!(2024-07-14 10:00),
    )
    .await;
    Library {
        alice: alice.id,
        bob: bob.id,
    }
}

fn filter() -> PhotoFilter {
    PhotoFilter::default()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn each_filter_alone_and_combined(pool: PgPool) {
    let lib = library(&pool).await;
    let a = lib.alice;
    let tag = |t: &str| PhotoFilter {
        tag: Some(t.into()),
        ..filter()
    };
    assert_eq!(names(&pool, a, &tag("dog")).await, ["dog.jpg"]);
    assert!(
        names(&pool, a, &tag("cat")).await.is_empty(),
        "unknown values: empty"
    );
    let paris = PhotoFilter {
        country: Some("FR".into()),
        region: Some("Île-de-France".into()),
        city: Some("Paris".into()),
        ..filter()
    };
    assert_eq!(names(&pool, a, &paris).await, ["shot.png", "dog.jpg"]);
    let no_region = PhotoFilter {
        country: Some("FR".into()),
        region: Some(String::new()),
        ..filter()
    };
    assert!(
        names(&pool, a, &no_region).await.is_empty(),
        "every place has a region"
    );
    sqlx::query("UPDATE media SET place_admin1 = NULL WHERE place_name = 'Lyon'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(names(&pool, a, &no_region).await, ["beach.jpg"]);
    let france = PhotoFilter {
        country: Some("FR".into()),
        ..filter()
    };
    assert_eq!(names(&pool, a, &france).await.len(), 3);
    let canon = PhotoFilter {
        camera_make: Some("Canon".into()),
        ..filter()
    };
    assert_eq!(names(&pool, a, &canon).await, ["beach.jpg", "dog.jpg"]);
    let shots = PhotoFilter {
        kind: Some(AssetKind::Screenshot),
        ..filter()
    };
    assert_eq!(names(&pool, a, &shots).await, ["shot.png"]);
    let paris_canon = PhotoFilter {
        camera_make: Some("Canon".into()),
        ..paris.clone()
    };
    assert_eq!(names(&pool, a, &paris_canon).await, ["dog.jpg"]);
    let paris_beach = PhotoFilter {
        tag: Some("beach".into()),
        ..paris
    };
    assert!(names(&pool, a, &paris_beach).await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn filters_only_ever_see_the_callers_photos(pool: PgPool) {
    let lib = library(&pool).await;
    let dog = PhotoFilter {
        tag: Some("dog".into()),
        ..filter()
    };
    // Bob holds the same tagged blob: he sees his node, never Alice's.
    assert_eq!(names(&pool, lib.bob, &dog).await, ["bobs-dog.jpg"]);
    let beach = PhotoFilter {
        tag: Some("beach".into()),
        ..filter()
    };
    assert!(names(&pool, lib.bob, &beach).await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn months_take_the_same_filters(pool: PgPool) {
    let lib = library(&pool).await;
    let canon = PhotoFilter {
        camera_make: Some("Canon".into()),
        ..filter()
    };
    let months = timeline::months(&pool, lib.alice, &canon).await.unwrap();
    let months: Vec<(String, i64)> = months.into_iter().map(|m| (m.month, m.count)).collect();
    assert_eq!(months, [("2024-08".into(), 1), ("2024-07".into(), 1)]);
}

/// The keyset still pages a filtered timeline without gaps or repeats.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn filtered_pages_resume_after_their_cursor(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let mut tagged = Vec::new();
    for day in 1..=9u8 {
        let at = datetime!(2024-07-01 12:00) + time::Duration::days(i64::from(day));
        let id = photo(&pool, alice.id, photos, &format!("{day}.jpg"), at).await;
        if day % 3 != 0 {
            tag(&pool, id, "dog").await;
            tagged.push(format!("{day}.jpg"));
        }
    }
    let dog = PhotoFilter {
        tag: Some("dog".into()),
        ..filter()
    };
    let mut seen = Vec::new();
    let mut after: Option<TimelineCursor> = None;
    loop {
        let page = timeline::page(&pool, alice.id, Filter::All, &dog, after.as_ref(), 2)
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|a| a.name.clone()));
        if !page.has_more {
            break;
        }
        after = page.items.last().map(|a| a.cursor());
    }
    tagged.reverse();
    assert_eq!(seen, tagged);
}

/// A person of `owner` with one face on the photo `node`'s blob.
async fn person_on(pool: &PgPool, owner: Uuid, node: Uuid) -> Uuid {
    let person = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(person)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, landmarks, \
                            score, embedding, person_id) \
         SELECT gen_random_uuid(), $1, blob_sha256, 'm', 0.1, 0.1, 0.2, 0.2, \
                array_fill(0.2::real, ARRAY[10]), 0.9, \
                array_fill(0.0::real, ARRAY[128])::halfvec, $3 \
           FROM nodes WHERE id = $2",
    )
    .bind(owner)
    .bind(node)
    .bind(person)
    .execute(pool)
    .await
    .unwrap();
    person
}

async fn node_named(pool: &PgPool, owner: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM nodes WHERE owner_id = $1 AND name = $2")
        .bind(owner)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_person_filter_keeps_photos_with_their_face(pool: PgPool) {
    let lib = library(&pool).await;
    let dog = node_named(&pool, lib.alice, "dog.jpg").await;
    let beach = node_named(&pool, lib.alice, "beach.jpg").await;
    let anna = person_on(&pool, lib.alice, dog).await;
    person_on(&pool, lib.alice, beach).await;
    let only = |person| PhotoFilter {
        person: Some(person),
        ..filter()
    };
    assert_eq!(names(&pool, lib.alice, &only(anna)).await, ["dog.jpg"]);
    let with_tag = PhotoFilter {
        tag: Some("beach".into()),
        ..only(anna)
    };
    assert!(names(&pool, lib.alice, &with_tag).await.is_empty());
    assert!(
        names(&pool, lib.bob, &only(anna)).await.is_empty(),
        "Bob holds the same blob, but Anna is Alice's"
    );
    assert!(
        names(&pool, lib.alice, &only(Uuid::now_v7()))
            .await
            .is_empty()
    );
}
