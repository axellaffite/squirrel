//! Manual measurement (ignored): exact photo search over synthetic libraries.
//!
//!     cargo test --release -p squirrel-db --test photo_search_perf -- --ignored --nocapture

mod common;

use std::time::{Duration, Instant};

use common::{admin, invited, root};
use sqlx::PgPool;
use squirrel_db::photo_filter::PhotoFilter;
use squirrel_db::photo_search::{self, PhotoQuery};
use uuid::Uuid;

const MODEL: &str = "openai-vit-b32-224";

/// `count` photos of `owner` with random 512-d embeddings (one statement).
async fn library(pool: &PgPool, owner: Uuid, parent: Uuid, count: i64, salt: &str) {
    sqlx::query(
        "WITH blobs AS ( \
             INSERT INTO blobs (sha256, size, mime) \
             SELECT md5($4 || g::text) || md5(g::text || $4), 10, 'image/jpeg' \
               FROM generate_series(1, $3) g RETURNING sha256), \
         media AS ( \
             INSERT INTO media (sha256, kind, metadata_state, thumbs_state, clip_state) \
             SELECT sha256, 'image', 'ready', 'ready', 'ready' FROM blobs RETURNING sha256), \
         nodes AS ( \
             INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             SELECT gen_random_uuid(), $1, $2, sha256 || '.jpg', 'file', sha256, 10, 'image/jpeg' \
               FROM media RETURNING id, blob_sha256), \
         assets AS ( \
             INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
             SELECT id, $1, blob_sha256, 'photo', now() - random() * interval '3 years', 'exif' \
               FROM nodes RETURNING sha256) \
         INSERT INTO clip_embeddings (sha256, model, embedding) \
         SELECT a.sha256, $5, (SELECT array_agg(random() - 0.5) \
                                 FROM generate_series(1, 512) WHERE a.sha256 IS NOT NULL)::vector(512)::halfvec(512) \
           FROM assets a",
    )
    .bind(owner)
    .bind(parent)
    .bind(count)
    .bind(salt)
    .bind(MODEL)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE").execute(pool).await.unwrap();
}

fn query_vector(seed: u32) -> Vec<f32> {
    let raw: Vec<f32> = (0..512u32)
        .map(|i| {
            ((i.wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)) % 1000) as f32 / 1000.0
                - 0.5
        })
        .collect();
    let norm = raw.iter().map(|v| v * v).sum::<f32>().sqrt();
    raw.iter().map(|v| v / norm).collect()
}

async fn measure(pool: &PgPool, owner: Uuid, label: &str) {
    let filter = PhotoFilter::default();
    let mut times = Vec::new();
    for seed in 0..20 {
        let vector = query_vector(seed);
        let query = PhotoQuery {
            model: MODEL,
            embedding: &vector,
            // Random vectors sit near distance 1: keep everything, the worst case.
            max_distance: 2.0,
            relative_cutoff: 2.0,
            filter: &filter,
        };
        let start = Instant::now();
        let page = photo_search::search(pool, owner, &query, 0, 50)
            .await
            .unwrap();
        times.push(start.elapsed());
        assert_eq!(page.items.len(), 50);
    }
    times.sort_unstable();
    let p = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    println!(
        "{label}: p50 {:.1?}, p95 {:.1?}, max {:.1?}",
        p(0.5),
        p(0.95),
        times[times.len() - 1]
    );
    assert!(p(0.95) < Duration::from_millis(300), "p95 over 300 ms");
}

#[ignore = "manual measurement"]
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn exact_search_latency(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let alice_root = root(&pool, alice.id, "Photos").await.id;
    let bob_root = root(&pool, bob.id, "Photos").await.id;
    let start = Instant::now();
    library(&pool, alice.id, alice_root, 10_000, "a").await;
    library(&pool, bob.id, bob_root, 50_000, "b").await;
    println!("seeded 60 000 photos in {:.1?}", start.elapsed());
    measure(&pool, alice.id, "10k photos (beside a 50k user)").await;
    library(&pool, alice.id, alice_root, 40_000, "c").await;
    measure(&pool, alice.id, "50k photos (beside a 50k user)").await;
    timeline_with_a_tag(&pool, alice.id).await;
    let vector = query_vector(1);
    let filter = PhotoFilter::default();
    let plan = photo_search::explain(
        &pool,
        bob.id,
        &PhotoQuery {
            model: MODEL,
            embedding: &vector,
            max_distance: 2.0,
            relative_cutoff: 2.0,
            filter: &filter,
        },
    )
    .await
    .unwrap();
    println!("{plan}");
}

/// The timeline's first page with a tag filter (one photo in ten tagged).
async fn timeline_with_a_tag(pool: &PgPool, owner: Uuid) {
    sqlx::query(
        "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         SELECT sha256, 'dog', 'clip', 0.3, 2 FROM assets \
          WHERE owner_id = $1 AND random() < 0.1",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE").execute(pool).await.unwrap();
    let dog = PhotoFilter {
        tag: Some("dog".into()),
        ..PhotoFilter::default()
    };
    let mut times = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let page = squirrel_db::timeline::page(
            pool,
            owner,
            squirrel_db::timeline::Filter::All,
            &dog,
            None,
            200,
        )
        .await
        .unwrap();
        times.push(start.elapsed());
        assert_eq!(page.items.len(), 200);
    }
    times.sort_unstable();
    println!(
        "timeline page of 200 with a tag filter (50k photos, 10 % tagged): p50 {:.1?}, p95 {:.1?}",
        times[10], times[19]
    );
}
