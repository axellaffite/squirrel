#![allow(dead_code)]

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::{DisplayName, Username};
use squirrel_db::invitations::{self, NewInvitation};
use squirrel_db::nodes::{self, NodeRecord};
use squirrel_db::users::{self, NewUser, UserRecord};
use squirrel_jobs::JobContext;
use squirrel_media::geo_data::GeoSource;
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;
use squirrel_worker::MediaContext;
use squirrel_worker::exec::Executor;
use squirrel_worker::geocode::GeoCache;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub struct Fixture {
    pub _dir: tempfile::TempDir,
    pub ctx: Arc<MediaContext>,
}

impl Fixture {
    pub fn new(pool: PgPool) -> Self {
        Self::with_executor(pool, Executor::InProcess)
    }

    pub fn with_executor(pool: PgPool, executor: Executor) -> Self {
        Self::build(pool, executor, |_| GeoSource::Disabled)
    }

    /// A fixture whose geocoder reads `source(data_dir)`.
    pub fn with_geo(pool: PgPool, source: impl FnOnce(&Path) -> GeoSource) -> Self {
        Self::build(pool, Executor::InProcess, source)
    }

    fn build(pool: PgPool, executor: Executor, source: impl FnOnce(&Path) -> GeoSource) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let storage = BlobStore::open(dir.path()).unwrap();
        let ctx = MediaContext {
            pool,
            storage,
            executor,
            pdfium_dir: None,
            decode_limits: DecodeLimits::default(),
            geo: GeoCache::new(source(dir.path())),
        };
        Self {
            _dir: dir,
            ctx: Arc::new(ctx),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.ctx.pool
    }

    /// Stores `bytes` and inserts its blob row and a file node with the sniffed MIME.
    pub async fn store_file(
        &self,
        owner: Uuid,
        parent: Uuid,
        name: &str,
        bytes: &[u8],
    ) -> NodeRecord {
        let (hash, size) = self.ctx.storage.put_reader(Cursor::new(bytes)).unwrap();
        let mime = squirrel_storage::mime::detect(bytes, name);
        let size = i64::try_from(size).unwrap();
        sqlx::query(
            "INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(hash.as_str())
        .bind(size)
        .bind(&mime)
        .execute(self.pool())
        .await
        .unwrap();
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, $5, $6) RETURNING id",
        )
        .bind(owner)
        .bind(parent)
        .bind(name)
        .bind(hash.as_str())
        .bind(size)
        .bind(&mime)
        .fetch_one(self.pool())
        .await
        .unwrap();
        nodes::find_any(self.pool(), id).await.unwrap().unwrap()
    }
}

pub fn job(kind: &str, payload: Value) -> JobContext {
    JobContext::for_tests(kind, payload)
}

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

pub async fn root(pool: &PgPool, owner: Uuid, name: &str) -> Uuid {
    nodes::list_children(pool, owner, None, None, 10)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("no root named {name}"))
        .id
}

/// `(kind, priority, payload)` of every queued job, by kind.
pub async fn queued(pool: &PgPool) -> Vec<(String, i16, Value)> {
    sqlx::query_as(
        "SELECT kind, priority, payload FROM jobs WHERE status = 'queued' ORDER BY kind, id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

pub async fn queued_kinds(pool: &PgPool) -> Vec<(String, i16)> {
    queued(pool)
        .await
        .into_iter()
        .map(|(kind, priority, _)| (kind, priority))
        .collect()
}

pub fn zip_bytes() -> Vec<u8> {
    b"PK\x03\x04\x14\x00\x00\x00\x00\x00rest of a zip".to_vec()
}
