//! Upgrading a database that already holds CLIP B/32 data (the phone server
//! had 17 photos embedded) to SigLIP2.

mod common;

use common::{admin, hash, raw_file, root};
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::{clip, media};

const SIGLIP2: i64 = 20261118000007;
const OLD_MODEL: &str = "openai-vit-b32-224";
const NEW_MODEL: &str = "siglip2-vit-b16-224";

/// Every migration before `version`, as `sqlx` would have run them.
async fn migrate_before(pool: &PgPool, version: i64) {
    for migration in squirrel_db::MIGRATOR.iter() {
        if migration.migration_type.is_down_migration() || migration.version >= version {
            continue;
        }
        sqlx::raw_sql(&migration.sql).execute(pool).await.unwrap();
    }
}

async fn run_migration(pool: &PgPool, version: i64) {
    let migration = squirrel_db::MIGRATOR
        .iter()
        .find(|m| m.version == version && !m.migration_type.is_down_migration())
        .expect("the SigLIP2 migration exists");
    sqlx::raw_sql(&migration.sql).execute(pool).await.unwrap();
}

/// An image blob in the admin's `Files` root, its media row in `clip_state`.
async fn image(
    pool: &PgPool,
    owner: uuid::Uuid,
    files: uuid::Uuid,
    fill: char,
    state: &str,
) -> String {
    raw_file(pool, owner, files, &format!("{fill}.jpg"), fill, 3).await;
    let sha = hash(fill).as_str().to_owned();
    let mut conn = pool.acquire().await.unwrap();
    media::ensure(&mut conn, &sha, MediaKind::Image)
        .await
        .unwrap();
    sqlx::query("UPDATE media SET clip_state = $2 WHERE sha256 = $1")
        .bind(&sha)
        .bind(state)
        .execute(pool)
        .await
        .unwrap();
    sha
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn the_upgrade_drops_b32_data_and_queues_every_image_again(pool: PgPool) {
    migrate_before(&pool, SIGLIP2).await;
    let owner = admin(&pool).await.id;
    let files = root(&pool, owner, "Files").await.id;
    let embedded = image(&pool, owner, files, 'a', "ready").await;
    let failed = image(&pool, owner, files, 'b', "failed").await;
    let broken = image(&pool, owner, files, 'c', "unsupported").await;
    let b32 = format!("[{}]", vec!["0.0441"; 512].join(","));
    sqlx::query(
        "INSERT INTO clip_embeddings (sha256, model, embedding, tagged_labels_version) \
         VALUES ($1, $2, $3::halfvec, 2)",
    )
    .bind(&embedded)
    .bind(OLD_MODEL)
    .bind(&b32)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO clip_label_embeddings (model, labels_version, tag, embedding) \
         VALUES ($1, 2, 'dog', $2::halfvec)",
    )
    .bind(OLD_MODEL)
    .bind(&b32)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO clip_label_banks (model, labels_version, digest) VALUES ($1, 2, $2)")
        .bind(OLD_MODEL)
        .bind("0".repeat(64))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO asset_tags (sha256, tag, source, score, labels_version) \
         VALUES ($1, 'dog', 'clip', 0.3, 2)",
    )
    .bind(&embedded)
    .execute(&pool)
    .await
    .unwrap();

    run_migration(&pool, SIGLIP2).await;

    for table in [
        "clip_embeddings",
        "clip_label_embeddings",
        "clip_label_banks",
        "asset_tags",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    let state = |sha: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT clip_state FROM media WHERE sha256 = $1")
                .bind(sha)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(state(embedded.clone()).await, "pending");
    assert_eq!(state(failed).await, "pending");
    assert_eq!(
        state(broken).await,
        "unsupported",
        "the input is still unusable"
    );

    let unit: Vec<f32> = (0..768).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
    clip::store_embedding(&pool, &embedded, NEW_MODEL, &unit)
        .await
        .unwrap();
    assert!(
        clip::store_embedding(&pool, &embedded, NEW_MODEL, &unit[..512])
            .await
            .is_err(),
        "512-d vectors no longer fit"
    );
}
