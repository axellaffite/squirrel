//! Manual measurement (ignored): listing people over a synthetic library of
//! 50 000 faces (25 000 photos, 2 faces each; 1 000 people).
//!
//!     cargo test --release -p squirrel-db --test people_perf -- --ignored --nocapture

mod common;

use std::time::{Duration, Instant};

use common::{admin, root};
use sqlx::PgPool;
use squirrel_db::people;

async fn library(pool: &PgPool, owner: uuid::Uuid, parent: uuid::Uuid, photos: i64) {
    sqlx::query(
        "WITH blobs AS ( \
             INSERT INTO blobs (sha256, size, mime) \
             SELECT md5(g::text) || md5('x' || g::text), 10, 'image/jpeg' \
               FROM generate_series(1, $3) g RETURNING sha256), \
         media AS ( \
             INSERT INTO media (sha256, kind, metadata_state, thumbs_state, thumbs_version) \
             SELECT sha256, 'image', 'ready', 'ready', 1 FROM blobs RETURNING sha256), \
         nodes AS ( \
             INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             SELECT gen_random_uuid(), $1, $2, sha256 || '.jpg', 'file', sha256, 10, 'image/jpeg' \
               FROM media RETURNING id, blob_sha256) \
         INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         SELECT id, $1, blob_sha256, 'photo', now() - random() * interval '3 years', 'exif' \
           FROM nodes",
    )
    .bind(owner)
    .bind(parent)
    .bind(photos)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO people (id, owner_id, name) \
         SELECT gen_random_uuid(), $1, CASE WHEN g % 10 = 0 THEN 'P' || g END \
           FROM generate_series(1, 1000) g",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "WITH ps AS (SELECT id, row_number() OVER (ORDER BY id) - 1 AS n FROM people WHERE owner_id = $1) \
         INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, landmarks, \
                            score, embedding, person_id) \
         SELECT gen_random_uuid(), $1, a.sha256, 'm', 0.1 * k, 0.1, 0.2, 0.2, \
                array_fill(0.2::real, ARRAY[10]), random(), \
                array_fill(0.01::real, ARRAY[128])::halfvec, ps.id \
           FROM assets a CROSS JOIN generate_series(1, 2) k \
           JOIN ps ON ps.n = (abs(hashtext(a.sha256 || k::text)) % 1000) \
          WHERE a.owner_id = $1",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE").execute(pool).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
#[ignore = "manual measurement"]
async fn listing_people_at_50k_faces(pool: PgPool) {
    let alice = admin(&pool).await;
    let parent = root(&pool, alice.id, "Photos").await.id;
    library(&pool, alice.id, parent, 25_000).await;
    let mut times: Vec<Duration> = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let listed = people::list(&pool, alice.id, false).await.unwrap();
        times.push(started.elapsed());
        assert_eq!(listed.len(), 1000);
    }
    times.sort();
    println!(
        "GET /api/people, 1000 people, 50k faces: p50 {:?}, p95 {:?}",
        times[10], times[18]
    );
}
