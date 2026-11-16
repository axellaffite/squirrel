#![allow(dead_code)]

use sqlx::PgPool;
use squirrel_core::{ContentHash, DisplayName, Username};
use squirrel_db::invitations::{self, NewInvitation};
use squirrel_db::nodes::{self, NodeRecord};
use squirrel_db::users::{self, NewUser, UserRecord};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

pub async fn admin(pool: &PgPool) -> UserRecord {
    users::create_first_admin(pool, &new_user("alice"))
        .await
        .unwrap()
}

/// A regular user invited by `inviter`, through the real join path.
pub async fn invited(pool: &PgPool, inviter: &UserRecord, name: &str) -> UserRecord {
    let token_hash = format!("token-{name}").into_bytes();
    invitations::create(
        pool,
        &NewInvitation {
            created_by: inviter.id,
            token_hash: &token_hash,
            email_hint: None,
            expires_at: OffsetDateTime::now_utc() + Duration::days(1),
        },
    )
    .await
    .unwrap();
    users::create_with_invitation(pool, &new_user(name), &token_hash)
        .await
        .unwrap()
}

pub async fn root(pool: &PgPool, owner: Uuid, name: &str) -> NodeRecord {
    nodes::list_children(pool, owner, None, None, 10)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("no root named {name}"))
}

pub fn hash(fill: char) -> ContentHash {
    ContentHash::parse(&fill.to_string().repeat(64)).unwrap()
}

/// Inserts a folder with raw SQL (no validation, no lock), for read-side tests.
pub async fn raw_folder(pool: &PgPool, owner: Uuid, parent: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'folder') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Inserts a blob row (if missing) and a file node referencing it.
pub async fn raw_file(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    fill: char,
    size: i64,
) -> Uuid {
    let sha = hash(fill);
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'text/plain') ON CONFLICT DO NOTHING")
        .bind(sha.as_str())
        .bind(size)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, $5, 'text/plain') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .bind(sha.as_str())
    .bind(size)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn set_quota(pool: &PgPool, user: Uuid, quota: Option<i64>) {
    sqlx::query("UPDATE users SET quota_bytes = $2 WHERE id = $1")
        .bind(user)
        .bind(quota)
        .execute(pool)
        .await
        .unwrap();
}

/// Inserts, with raw SQL, an image file node of blob `sha` (created if
/// missing, with ready thumbnails) and its timeline entry.
pub async fn photo_of_blob(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    sha: &str,
    taken_at: time::PrimitiveDateTime,
) -> Uuid {
    sqlx::query(
        "INSERT INTO blobs (sha256, size, mime) VALUES ($1, 10, 'image/jpeg') \
         ON CONFLICT DO NOTHING",
    )
    .bind(sha)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, thumbs_state, thumbs_version, \
                            width, height, blurhash) \
         VALUES ($1, 'image', 'ready', 'ready', 1, 4, 3, 'LEHV6nWB2yk8') ON CONFLICT DO NOTHING",
    )
    .bind(sha)
    .execute(pool)
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, 10, 'image/jpeg') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .bind(sha)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         VALUES ($1, $2, $3, 'photo', $4, 'exif')",
    )
    .bind(id)
    .bind(owner)
    .bind(sha)
    .bind(taken_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// [`photo_of_blob`] with a blob of its own.
pub async fn photo(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    taken_at: time::PrimitiveDateTime,
) -> Uuid {
    let simple = Uuid::now_v7().simple().to_string();
    let sha = format!("{simple}{simple}");
    photo_of_blob(pool, owner, parent, name, &sha, taken_at).await
}

/// Moves a node to the trash (raw SQL, as its own trash entry).
pub async fn raw_trash(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// Brings a node back from the trash (raw SQL).
pub async fn raw_restore(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE nodes SET deleted_at = NULL, trash_root = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}
