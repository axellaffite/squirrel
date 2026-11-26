mod common;

use common::{admin, photo, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::album_rules::{self, AlbumRules, SearchMatch};
use squirrel_db::albums::{self, AlbumUpdate};
use squirrel_db::clip;
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = "siglip2-vit-b16-224";

/// A unit vector at `angle` radians in the plane of the first two axes: the
/// cosine distance between two of them is `1 − cos(difference)`.
fn at(angle: f32) -> Vec<f32> {
    let mut v = vec![0.0; 768];
    v[0] = angle.cos();
    v[1] = angle.sin();
    v
}

async fn embed(pool: &PgPool, node: Uuid, model: &str, angle: f32) {
    let sha: String = sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap();
    clip::store_embedding(pool, &sha, model, &at(angle))
        .await
        .unwrap();
}

async fn tag(pool: &PgPool, node: Uuid, tag: &str) {
    sqlx::query(
        "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         SELECT blob_sha256, $2, 'clip', 0.3, 3 FROM nodes WHERE id = $1",
    )
    .bind(node)
    .bind(tag)
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

/// The text at angle 0, kept within cosine distance 0.2 (≈ 0.64 rad).
fn search(rivals: &[f32]) -> AlbumRules {
    AlbumRules {
        search: Some("shirtless".into()),
        search_match: Some(SearchMatch {
            model: MODEL.into(),
            embedding: at(0.0),
            rivals: rivals.iter().map(|&angle| at(angle)).collect(),
            max_distance: 0.2,
            rival_margin: 0.01,
        }),
        ..AlbumRules::default()
    }
}

/// Alice's photos: near the text (0.1 rad), farther but within reach
/// (0.5 rad), out of reach (1.2 rad), near but of another model, and one the
/// worker has not embedded yet.
struct World {
    alice: Uuid,
    near: Uuid,
    mid: Uuid,
    far: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let alice = admin(pool).await.id;
    let photos = root(pool, alice, "Photos").await.id;
    let near = photo(pool, alice, photos, "near.jpg", datetime!(2024-07-01 10:00)).await;
    let mid = photo(pool, alice, photos, "mid.jpg", datetime!(2024-07-02 10:00)).await;
    let far = photo(pool, alice, photos, "far.jpg", datetime!(2024-07-03 10:00)).await;
    let other = photo(
        pool,
        alice,
        photos,
        "other.jpg",
        datetime!(2024-07-04 10:00),
    )
    .await;
    photo(
        pool,
        alice,
        photos,
        "pending.jpg",
        datetime!(2024-07-05 10:00),
    )
    .await;
    embed(pool, near, MODEL, 0.1).await;
    embed(pool, mid, MODEL, 0.5).await;
    embed(pool, far, MODEL, 1.2).await;
    embed(pool, other, "another-model", 0.0).await;
    World {
        alice,
        near,
        mid,
        far,
    }
}

async fn dynamic(pool: &PgPool, owner: Uuid, rules: &AlbumRules) -> Uuid {
    albums::create_dynamic(pool, owner, &AlbumName::parse("Shirtless").unwrap(), rules)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_keeps_the_photos_close_to_it(pool: PgPool) {
    let w = world(&pool).await;
    let album = dynamic(&pool, w.alice, &search(&[])).await;
    assert_eq!(item_names(&pool, album).await, ["mid.jpg", "near.jpg"]);
    let record = albums::find(&pool, w.alice, album).await.unwrap().unwrap();
    assert_eq!(record.item_count, 2);
    let preview = album_rules::preview(&pool, w.alice, &search(&[]), 10)
        .await
        .unwrap();
    assert_eq!(preview.count, 2, "the preview matches as the album does");

    let stored = album_rules::find_many(&pool, &[album]).await.unwrap();
    assert_eq!(stored[0].1.search.as_deref(), Some("shirtless"));
    assert_eq!(stored[0].1.search_match, None, "never read back");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_closer_to_a_rival_does_not_match(pool: PgPool) {
    let w = world(&pool).await;
    // The rival at 0.6 rad is closer to "mid" (0.5) than the text is.
    let album = dynamic(&pool, w.alice, &search(&[0.6])).await;
    assert_eq!(item_names(&pool, album).await, ["near.jpg"]);
    let albums_of_mid = albums::albums_for_node(&pool, w.alice, w.mid)
        .await
        .unwrap();
    assert!(albums_of_mid.is_empty());
    let albums_of_near = albums::albums_for_node(&pool, w.alice, w.near)
        .await
        .unwrap();
    assert_eq!(albums_of_near.len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_combines_with_the_other_conditions(pool: PgPool) {
    let w = world(&pool).await;
    tag(&pool, w.mid, "beach").await;
    tag(&pool, w.far, "beach").await;
    let on_the_beach = AlbumRules {
        tags: vec!["beach".into()],
        ..search(&[])
    };
    let album = dynamic(&pool, w.alice, &on_the_beach).await;
    assert_eq!(item_names(&pool, album).await, ["mid.jpg"]);

    // Dropping the text from the rule: every beach photo again.
    let beach_only = AlbumUpdate {
        rules: Some(AlbumRules {
            tags: vec!["beach".into()],
            ..AlbumRules::default()
        }),
        ..AlbumUpdate::default()
    };
    albums::update(&pool, album, w.alice, &beach_only)
        .await
        .unwrap();
    assert_eq!(item_names(&pool, album).await, ["far.jpg", "mid.jpg"]);
    let stored = album_rules::find_many(&pool, &[album]).await.unwrap();
    assert_eq!(stored[0].1.search, None);
    let embedding: Option<String> =
        sqlx::query_scalar("SELECT search_embedding::text FROM album_rules WHERE album_id = $1")
            .bind(album)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(embedding, None, "the text's embedding goes with it");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_search_text_cannot_be_saved_without_its_match(pool: PgPool) {
    let w = world(&pool).await;
    let text_only = AlbumRules {
        search: Some("shirtless".into()),
        ..AlbumRules::default()
    };
    let created = albums::create_dynamic(
        &pool,
        w.alice,
        &AlbumName::parse("Broken").unwrap(),
        &text_only,
    )
    .await;
    assert!(created.is_err(), "{created:?}");
}
