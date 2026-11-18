mod common;

use common::{admin, invited, photo, photo_of_blob, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::clip;
use squirrel_db::photo_filter::PhotoFilter;
use squirrel_db::photo_search::{self, PhotoQuery};
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = "openai-vit-b32-224";

/// A unit vector at `angle` radians in the plane of the first two axes.
fn at(angle: f32) -> Vec<f32> {
    let mut v = vec![0.0; 512];
    v[0] = angle.cos();
    v[1] = angle.sin();
    v
}

async fn sha(pool: &PgPool, node: Uuid) -> String {
    sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn embed(pool: &PgPool, node: Uuid, angle: f32) {
    clip::store_embedding(pool, &sha(pool, node).await, MODEL, &at(angle))
        .await
        .unwrap();
}

async fn search(
    pool: &PgPool,
    owner: Uuid,
    filter: &PhotoFilter,
    max: f64,
    offset: u32,
    limit: u32,
) -> (Vec<String>, bool) {
    let query = at(0.0);
    let page = photo_search::search(
        pool,
        owner,
        &PhotoQuery {
            model: MODEL,
            embedding: &query,
            max_distance: max,
            relative_cutoff: 2.0,
            filter,
        },
        offset,
        limit,
    )
    .await
    .unwrap();
    (
        page.items.iter().map(|h| h.summary.name.clone()).collect(),
        page.has_more,
    )
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn nearest_first_within_the_distance_floor(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let at_time = datetime!(2024-07-01 10:00);
    for (name, angle) in [
        ("near.jpg", 0.1),
        ("nearest.jpg", 0.0),
        ("mid.jpg", 0.5),
        ("far.jpg", 1.5),
    ] {
        let id = photo(&pool, alice.id, photos, name, at_time).await;
        embed(&pool, id, angle).await;
    }
    photo(&pool, alice.id, photos, "unembedded.jpg", at_time).await;
    let all = PhotoFilter::default();
    // 1 − cos(0.5) ≈ 0.12; 1 − cos(1.5) ≈ 0.93.
    let (names, more) = search(&pool, alice.id, &all, 0.5, 0, 10).await;
    assert_eq!(names, ["nearest.jpg", "near.jpg", "mid.jpg"]);
    assert!(!more);
    let (page, more) = search(&pool, alice.id, &all, 0.5, 1, 1).await;
    assert_eq!((page, more), (vec!["near.jpg".to_owned()], true));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn filters_combine_and_trashed_photos_are_left_out(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let at_time = datetime!(2024-07-01 10:00);
    let shot = photo(&pool, alice.id, photos, "shot.png", at_time).await;
    let dog = photo(&pool, alice.id, photos, "dog.jpg", at_time).await;
    let gone = photo(&pool, alice.id, photos, "gone.jpg", at_time).await;
    for id in [shot, dog, gone] {
        embed(&pool, id, 0.0).await;
    }
    raw_trash(&pool, gone).await;
    sqlx::query("UPDATE assets SET kind = 'screenshot' WHERE node_id = $1")
        .bind(shot)
        .execute(&pool)
        .await
        .unwrap();
    let screenshots = PhotoFilter {
        kind: Some(squirrel_db::assets::AssetKind::Screenshot),
        ..PhotoFilter::default()
    };
    assert_eq!(
        search(&pool, alice.id, &screenshots, 2.0, 0, 10).await.0,
        ["shot.png"]
    );
    let (all, _) = search(&pool, alice.id, &PhotoFilter::default(), 2.0, 0, 10).await;
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(!all.contains(&"gone.jpg".to_owned()));
}

/// Bob holds one of Alice's blobs: he finds his own node, never hers, and
/// his results do not move when she adds a thousand photos.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_user_only_ever_finds_their_own_photos(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let alice_photos = root(&pool, alice.id, "Photos").await.id;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let at_time = datetime!(2024-07-01 10:00);
    let shared = photo(&pool, alice.id, alice_photos, "alice-dog.jpg", at_time).await;
    embed(&pool, shared, 0.0).await;
    let other = photo(&pool, alice.id, alice_photos, "alice-only.jpg", at_time).await;
    embed(&pool, other, 0.05).await;
    let sha = sha(&pool, shared).await;
    photo_of_blob(&pool, bob.id, bob_photos, "bob-dog.jpg", &sha, at_time).await;
    let all = PhotoFilter::default();
    let before = search(&pool, bob.id, &all, 2.0, 0, 10).await;
    assert_eq!(before, (vec!["bob-dog.jpg".to_owned()], false));
    sqlx::query(
        "WITH blobs AS ( \
             INSERT INTO blobs (sha256, size, mime) \
             SELECT md5(g::text) || md5(g::text || 'x'), 10, 'image/jpeg' \
               FROM generate_series(1, 1000) g RETURNING sha256), \
         media AS ( \
             INSERT INTO media (sha256, kind, metadata_state, thumbs_state) \
             SELECT sha256, 'image', 'ready', 'ready' FROM blobs RETURNING sha256), \
         nodes AS ( \
             INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             SELECT gen_random_uuid(), $1, $2, sha256 || '.jpg', 'file', sha256, 10, 'image/jpeg' \
               FROM media RETURNING id, blob_sha256), \
         assets AS ( \
             INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
             SELECT id, $1, blob_sha256, 'photo', now(), 'exif' FROM nodes RETURNING sha256) \
         INSERT INTO clip_embeddings (sha256, model, embedding) \
         SELECT sha256, $3, $4 FROM assets",
    )
    .bind(alice.id)
    .bind(alice_photos)
    .bind(MODEL)
    .bind(pgvector::HalfVector::from_f32_slice(&at(0.0)))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(search(&pool, bob.id, &all, 2.0, 0, 10).await, before);
    let (alice_names, more) = search(&pool, alice.id, &all, 2.0, 0, 10).await;
    assert!(more && !alice_names.iter().any(|n| n.starts_with("bob")));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_weak_tail_far_behind_the_best_hit_is_dropped(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let when = datetime!(2024-07-01 10:00);
    // Distances 1 − cos(angle): 0, ≈ 0.02, ≈ 0.12.
    for (name, angle) in [("best.jpg", 0.0), ("close.jpg", 0.2), ("tail.jpg", 0.5)] {
        let id = photo(&pool, alice.id, photos, name, when).await;
        embed(&pool, id, angle).await;
    }
    let vector = at(0.0);
    let filter = PhotoFilter::default();
    let page = photo_search::search(
        &pool,
        alice.id,
        &PhotoQuery {
            model: MODEL,
            embedding: &vector,
            max_distance: 0.5,
            relative_cutoff: 0.06,
            filter: &filter,
        },
        0,
        10,
    )
    .await
    .unwrap();
    let names: Vec<&str> = page.items.iter().map(|h| h.summary.name.as_str()).collect();
    assert_eq!(names, ["best.jpg", "close.jpg"]);
}
