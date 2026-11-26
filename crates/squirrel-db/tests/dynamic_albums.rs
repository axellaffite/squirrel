mod common;

use common::{admin, invited, photo, raw_trash, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::album_rules::{self, AlbumRules, Match};
use squirrel_db::albums::{self, AlbumUpdate, CoverChange, UpdateOutcome};
use squirrel_db::assets::AssetKind;
use squirrel_db::shares::{self, Permission, Subject};
use squirrel_db::{DbError, access, people};
use time::macros::{date, datetime};
use uuid::Uuid;

fn name(raw: &str) -> AlbumName {
    AlbumName::parse(raw).unwrap()
}

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

async fn place(pool: &PgPool, node: Uuid, country: &str, region: Option<&str>, city: &str) {
    sqlx::query(
        "UPDATE media m SET place_country_code = $2, place_admin1 = $3, place_name = $4 \
           FROM nodes n WHERE n.id = $1 AND m.sha256 = n.blob_sha256",
    )
    .bind(node)
    .bind(country)
    .bind(region)
    .bind(city)
    .execute(pool)
    .await
    .unwrap();
}

async fn item_names(pool: &PgPool, album: Uuid) -> Vec<String> {
    albums::items(pool, album, None, 100)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect()
}

/// Alice's library: Léa alone at the beach, Léa and Tom at the beach, Tom
/// alone in the mountains, a screenshot. Bob has a photo with the same tag.
struct World {
    alice: Uuid,
    bob: Uuid,
    lea: Uuid,
    tom: Uuid,
    lea_beach: Uuid,
    both_beach: Uuid,
    tom_mountain: Uuid,
    shot: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let alice_user = admin(pool).await;
    let alice = alice_user.id;
    let bob = invited(pool, &alice_user, "bob").await.id;
    let photos = root(pool, alice, "Photos").await.id;
    let lea_beach = photo(
        pool,
        alice,
        photos,
        "lea_beach.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let both_beach = photo(
        pool,
        alice,
        photos,
        "both_beach.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    let tom_mountain = photo(
        pool,
        alice,
        photos,
        "tom_mountain.jpg",
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
    let tom = person(pool, alice).await;
    face(pool, alice, lea_beach, lea).await;
    face(pool, alice, both_beach, lea).await;
    face(pool, alice, both_beach, tom).await;
    face(pool, alice, tom_mountain, tom).await;
    tag(pool, lea_beach, "beach").await;
    tag(pool, both_beach, "beach").await;
    tag(pool, both_beach, "sea").await;
    tag(pool, tom_mountain, "mountain").await;
    place(pool, lea_beach, "FR", Some("Bretagne"), "Brest").await;
    place(pool, both_beach, "FR", Some("Bretagne"), "Quimper").await;
    place(pool, tom_mountain, "CH", None, "Zermatt").await;
    let bob_photos = root(pool, bob, "Photos").await.id;
    let bobs = photo(
        pool,
        bob,
        bob_photos,
        "bob_beach.jpg",
        datetime!(2024-07-03 10:00),
    )
    .await;
    tag(pool, bobs, "beach").await;
    World {
        alice,
        bob,
        lea,
        tom,
        lea_beach,
        both_beach,
        tom_mountain,
        shot,
    }
}

async fn dynamic(pool: &PgPool, owner: Uuid, rules: &AlbumRules) -> Uuid {
    albums::create_dynamic(pool, owner, &name("Dynamic"), rules)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn people_match_any_or_all(pool: PgPool) {
    let w = world(&pool).await;
    let any = dynamic(
        &pool,
        w.alice,
        &AlbumRules {
            people: vec![w.lea, w.tom],
            ..AlbumRules::default()
        },
    )
    .await;
    assert_eq!(
        item_names(&pool, any).await,
        ["tom_mountain.jpg", "both_beach.jpg", "lea_beach.jpg"]
    );
    let all = dynamic(
        &pool,
        w.alice,
        &AlbumRules {
            people: vec![w.lea, w.tom, w.lea],
            people_match: Match::All,
            ..AlbumRules::default()
        },
    )
    .await;
    assert_eq!(item_names(&pool, all).await, ["both_beach.jpg"]);
    let stored = album_rules::find_many(&pool, &[all]).await.unwrap();
    assert_eq!(stored[0].1.people, [w.lea, w.tom], "duplicates dropped");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn groups_combine_with_and(pool: PgPool) {
    let w = world(&pool).await;
    let tom_and_beach_or_sea = AlbumRules {
        people: vec![w.tom],
        tags: vec!["beach".into(), "sea".into()],
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &tom_and_beach_or_sea).await;
    assert_eq!(item_names(&pool, album).await, ["both_beach.jpg"]);

    let both_tags = AlbumRules {
        tags: vec!["beach".into(), "sea".into()],
        tags_match: Match::All,
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &both_tags).await;
    assert_eq!(item_names(&pool, album).await, ["both_beach.jpg"]);

    let brest = AlbumRules {
        country: Some("FR".into()),
        region: Some("Bretagne".into()),
        city: Some("Brest".into()),
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &brest).await;
    assert_eq!(item_names(&pool, album).await, ["lea_beach.jpg"]);

    let no_region = AlbumRules {
        country: Some("CH".into()),
        region: Some(String::new()),
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &no_region).await;
    assert_eq!(item_names(&pool, album).await, ["tom_mountain.jpg"]);

    let screenshots = AlbumRules {
        kind: Some(AssetKind::Screenshot),
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &screenshots).await;
    assert_eq!(item_names(&pool, album).await, ["shot.png"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn date_bounds_are_inclusive_days(pool: PgPool) {
    let w = world(&pool).await;
    let july_2nd_to_aug_1st = AlbumRules {
        taken_from: Some(date!(2024 - 07 - 02)),
        taken_to: Some(date!(2024 - 08 - 01)),
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &july_2nd_to_aug_1st).await;
    assert_eq!(
        item_names(&pool, album).await,
        ["tom_mountain.jpg", "both_beach.jpg"]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn contents_follow_the_library_and_only_the_owners(pool: PgPool) {
    let w = world(&pool).await;
    let beach = AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &beach).await;
    let record = albums::find(&pool, w.alice, album).await.unwrap().unwrap();
    assert!(record.dynamic);
    assert_eq!(record.item_count, 2, "Bob's beach photo is not Alice's");
    assert_eq!(record.cover_id, Some(w.both_beach), "newest match");

    raw_trash(&pool, w.both_beach).await;
    assert_eq!(item_names(&pool, album).await, ["lea_beach.jpg"]);
    tag(&pool, w.tom_mountain, "beach").await;
    assert_eq!(
        item_names(&pool, album).await,
        ["tom_mountain.jpg", "lea_beach.jpg"]
    );
    let refs = albums::albums_for_node(&pool, w.alice, w.lea_beach)
        .await
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].id, album);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn items_cannot_be_added_or_removed(pool: PgPool) {
    let w = world(&pool).await;
    let beach = AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &beach).await;
    let added = albums::add_items(&pool, album, w.alice, &[w.shot]).await;
    assert!(matches!(added, Err(DbError::DynamicAlbum)), "{added:?}");
    let removed =
        albums::remove_items(&pool, album, albums::Remover::AlbumOwner, &[w.lea_beach]).await;
    assert!(matches!(removed, Err(DbError::DynamicAlbum)), "{removed:?}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rules_and_cover_are_updated_together(pool: PgPool) {
    let w = world(&pool).await;
    let beach = AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &beach).await;
    let mountain_cover = AlbumUpdate {
        cover: CoverChange::Set(w.tom_mountain),
        ..AlbumUpdate::default()
    };
    assert_eq!(
        albums::update(&pool, album, w.alice, &mountain_cover)
            .await
            .unwrap(),
        UpdateOutcome::CoverNotInAlbum
    );
    let tom_with_cover = AlbumUpdate {
        cover: CoverChange::Set(w.tom_mountain),
        rules: Some(AlbumRules {
            people: vec![w.tom],
            ..AlbumRules::default()
        }),
        ..AlbumUpdate::default()
    };
    assert_eq!(
        albums::update(&pool, album, w.alice, &tom_with_cover)
            .await
            .unwrap(),
        UpdateOutcome::Updated
    );
    let record = albums::find(&pool, w.alice, album).await.unwrap().unwrap();
    assert_eq!(record.cover_id, Some(w.tom_mountain));
    assert_eq!(
        item_names(&pool, album).await,
        ["tom_mountain.jpg", "both_beach.jpg"]
    );

    let manual = albums::create(&pool, w.alice, &name("Manual"), &[])
        .await
        .unwrap();
    let rules_on_manual = AlbumUpdate {
        rules: Some(beach),
        ..AlbumUpdate::default()
    };
    assert_eq!(
        albums::update(&pool, manual, w.alice, &rules_on_manual)
            .await
            .unwrap(),
        UpdateOutcome::NotDynamic
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn merging_people_keeps_the_rule_meaning(pool: PgPool) {
    let w = world(&pool).await;
    let album = dynamic(
        &pool,
        w.alice,
        &AlbumRules {
            people: vec![w.lea],
            ..AlbumRules::default()
        },
    )
    .await;
    people::merge(&pool, w.alice, w.tom, &[w.lea])
        .await
        .unwrap();
    let stored = album_rules::find_many(&pool, &[album]).await.unwrap();
    assert_eq!(stored[0].1.people, [w.tom]);
    assert_eq!(
        item_names(&pool, album).await,
        ["tom_mountain.jpg", "both_beach.jpg", "lea_beach.jpg"]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shared_for_reading_only_and_matches_are_visible(pool: PgPool) {
    let w = world(&pool).await;
    let beach = AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    };
    let album = dynamic(&pool, w.alice, &beach).await;
    let write = shares::upsert(
        &pool,
        w.alice,
        Subject::Album(album),
        w.bob,
        Permission::Write,
    )
    .await;
    assert!(matches!(write, Err(DbError::DynamicAlbum)), "{write:?}");
    assert!(
        access::node(&pool, w.bob, w.lea_beach)
            .await
            .unwrap()
            .is_none()
    );
    shares::upsert(
        &pool,
        w.alice,
        Subject::Album(album),
        w.bob,
        Permission::Read,
    )
    .await
    .unwrap();
    let seen = access::node(&pool, w.bob, w.lea_beach)
        .await
        .unwrap()
        .expect("a matching photo of an album shared with Bob");
    assert_eq!(seen.access, access::Access::Read);
    assert!(
        access::node(&pool, w.bob, w.tom_mountain)
            .await
            .unwrap()
            .is_none(),
        "not in the album"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_preview_counts_and_lists_the_newest(pool: PgPool) {
    let w = world(&pool).await;
    let beach = AlbumRules {
        tags: vec!["beach".into()],
        ..AlbumRules::default()
    };
    let preview = album_rules::preview(&pool, w.alice, &beach, 1)
        .await
        .unwrap();
    assert_eq!(preview.count, 2);
    assert_eq!(preview.newest.len(), 1);
    assert_eq!(preview.newest[0].node_id, w.both_beach);
    let unknown_person = AlbumRules {
        people: vec![Uuid::now_v7()],
        ..AlbumRules::default()
    };
    let preview = album_rules::preview(&pool, w.alice, &unknown_person, 5)
        .await
        .unwrap();
    assert_eq!(preview.count, 0);
}
