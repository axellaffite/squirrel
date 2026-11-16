//! Timeline seeding with raw SQL (what the worker would have recorded).

use serde_json::Value;
use sqlx::PgPool;
use time::PrimitiveDateTime;
use uuid::Uuid;

use super::files::user_id;

/// What a seeded photo's blob looks like.
pub struct Seed<'a> {
    pub name: &'a str,
    /// Shared by every photo seeded with the same value (deduplicated content).
    pub sha256: Option<&'a str>,
    pub taken_at: PrimitiveDateTime,
    pub thumbs_state: &'a str,
}

impl<'a> Seed<'a> {
    pub fn new(name: &'a str, taken_at: PrimitiveDateTime) -> Self {
        Self {
            name,
            sha256: None,
            taken_at,
            thumbs_state: "ready",
        }
    }
}

fn fresh_sha() -> String {
    let simple = Uuid::now_v7().simple().to_string();
    format!("{simple}{simple}")
}

/// An image file node in `parent`, its media row and its timeline entry.
pub async fn seed(pool: &PgPool, me: &Value, parent: Uuid, seed: &Seed<'_>) -> Uuid {
    let owner = user_id(me);
    let sha = seed.sha256.map_or_else(fresh_sha, str::to_owned);
    sqlx::query(
        "INSERT INTO blobs (sha256, size, mime) VALUES ($1, 10, 'image/jpeg') \
         ON CONFLICT DO NOTHING",
    )
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, thumbs_state, thumbs_version, \
                            width, height, blurhash) \
         VALUES ($1, 'image', 'ready', $2, 1, 4, 3, 'LEHV6nWB2yk8') ON CONFLICT DO NOTHING",
    )
    .bind(&sha)
    .bind(seed.thumbs_state)
    .execute(pool)
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, 10, 'image/jpeg') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(seed.name)
    .bind(&sha)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_offset_min, \
                             taken_source) \
         VALUES ($1, $2, $3, 'photo', $4, 120, 'exif')",
    )
    .bind(id)
    .bind(owner)
    .bind(&sha)
    .bind(seed.taken_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

pub async fn photo(
    pool: &PgPool,
    me: &Value,
    parent: Uuid,
    name: &str,
    at: PrimitiveDateTime,
) -> Uuid {
    seed(pool, me, parent, &Seed::new(name, at)).await
}

/// The content hash behind a seeded photo.
pub async fn sha_of(pool: &PgPool, node: Uuid) -> String {
    sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap()
}
