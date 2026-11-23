//! Claim leases, resume of sessions being completed, chunk refusal reasons,
//! the per-owner session cap and the disk reservation of open sessions.

mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::DbError;
use squirrel_db::uploads::{self, ChunkWrite, NewUpload, UploadRecord};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn later() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::hours(24)
}

async fn open_named(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    size: i64,
    max_open: i64,
) -> Result<UploadRecord, DbError> {
    let (name, sha) = (NodeName::parse(name).unwrap(), common::hash('f'));
    uploads::create(
        pool,
        &NewUpload {
            owner_id: owner,
            created_by: owner,
            parent_id: parent,
            name: &name,
            sha256: &sha,
            size,
            chunk_size: 4,
            chunk_count: i32::try_from(u64::try_from(size).unwrap().div_ceil(4)).unwrap(),
            expires_at: later(),
            max_open,
            client_taken: None,
            device_asset: None,
        },
    )
    .await
}

async fn backdate_claim(pool: &PgPool, id: Uuid, ago: &str) {
    sqlx::query("UPDATE upload_sessions SET claimed_at = now() - $2::interval WHERE id = $1")
        .bind(id)
        .bind(ago)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_claim_whose_lease_expired_can_be_taken_again(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open_named(&pool, alice.id, files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();

    backdate_claim(&pool, session.id, "5 minutes").await;
    assert!(matches!(
        uploads::claim(&pool, alice.id, session.id, later()).await,
        Err(DbError::UploadBusy)
    ));
    backdate_claim(&pool, session.id, "11 minutes").await;
    let reclaimed = uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();
    assert!(!reclaimed.is_open());
    // The new claim starts a fresh lease.
    assert!(matches!(
        uploads::claim(&pool, alice.id, session.id, later()).await,
        Err(DbError::UploadBusy)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_being_completed_are_resumable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open_named(&pool, alice.id, files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();
    let name = NodeName::parse("a.bin").unwrap();
    let found = uploads::find_resumable(&pool, alice.id, files.id, &name, &common::hash('f'), 10)
        .await
        .unwrap();
    assert_eq!(found.map(|s| s.id), Some(session.id));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refused_chunks_say_whether_the_session_is_busy_or_gone(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let busy = open_named(&pool, alice.id, files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    uploads::claim(&pool, alice.id, busy.id, later())
        .await
        .unwrap();
    let record = |id| uploads::record_chunk(&pool, alice.id, id, 0, later());
    assert_eq!(record(busy.id).await.unwrap(), ChunkWrite::Busy);

    let expired = open_named(&pool, alice.id, files.id, "b.bin", 10, 100)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(expired.id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(record(expired.id).await.unwrap(), ChunkWrite::Gone);

    let aborted = open_named(&pool, alice.id, files.id, "c.bin", 10, 100)
        .await
        .unwrap();
    uploads::abort(&pool, alice.id, aborted.id).await.unwrap();
    assert_eq!(record(aborted.id).await.unwrap(), ChunkWrite::Gone);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn open_sessions_are_capped_per_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let alice_files = common::root(&pool, alice.id, "Files").await;
    let bob_files = common::root(&pool, bob.id, "Files").await;
    for name in ["a.bin", "b.bin"] {
        open_named(&pool, alice.id, alice_files.id, name, 10, 2)
            .await
            .unwrap();
    }
    assert!(matches!(
        open_named(&pool, alice.id, alice_files.id, "c.bin", 10, 2).await,
        Err(DbError::TooManyUploads)
    ));
    open_named(&pool, bob.id, bob_files.id, "a.bin", 10, 2)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_can_be_forgotten(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open_named(&pool, alice.id, files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    for index in [0, 1] {
        uploads::record_chunk(&pool, alice.id, session.id, index, later())
            .await
            .unwrap();
    }
    uploads::clear_chunks(&pool, session.id).await.unwrap();
    assert!(
        uploads::received_chunks(&pool, session.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pending_bytes_count_what_open_sessions_still_have_to_write(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    assert_eq!(uploads::pending_bytes(&pool).await.unwrap(), 0);
    let first = open_named(&pool, alice.id, files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    open_named(&pool, alice.id, files.id, "b.bin", 7, 100)
        .await
        .unwrap();
    assert_eq!(uploads::pending_bytes(&pool).await.unwrap(), 17);
    uploads::record_chunk(&pool, alice.id, first.id, 0, later())
        .await
        .unwrap();
    assert_eq!(uploads::pending_bytes(&pool).await.unwrap(), 13);
    for index in [1, 2] {
        uploads::record_chunk(&pool, alice.id, first.id, index, later())
            .await
            .unwrap();
    }
    assert_eq!(uploads::pending_bytes(&pool).await.unwrap(), 7);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pending_bytes_can_be_counted_per_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let alice_files = common::root(&pool, alice.id, "Files").await;
    let bob_files = common::root(&pool, bob.id, "Files").await;
    open_named(&pool, alice.id, alice_files.id, "a.bin", 10, 100)
        .await
        .unwrap();
    open_named(&pool, bob.id, bob_files.id, "b.bin", 7, 100)
        .await
        .unwrap();
    assert_eq!(
        uploads::pending_bytes_of(&pool, alice.id).await.unwrap(),
        10
    );
    assert_eq!(uploads::pending_bytes_of(&pool, bob.id).await.unwrap(), 7);
    assert_eq!(uploads::pending_bytes(&pool).await.unwrap(), 17);
}
