mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::DbError;
use squirrel_db::uploads::{self, ChunkWrite, NewUpload};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn file_name() -> NodeName {
    NodeName::parse("big.bin").unwrap()
}

async fn open(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    size: i64,
) -> Result<uploads::UploadRecord, DbError> {
    let (name, sha) = (file_name(), common::hash('f'));
    uploads::create(
        pool,
        &NewUpload {
            owner_id: owner,
            parent_id: parent,
            name: &name,
            sha256: &sha,
            size,
            chunk_size: 4,
            chunk_count: i32::try_from(u64::try_from(size).unwrap().div_ceil(4)).unwrap(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(24),
            max_open: 100,
            client_taken: None,
            device_asset: None,
        },
    )
    .await
}

fn later() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::hours(24)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_need_a_live_folder_of_the_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    assert!(session.is_open());
    assert_eq!(session.chunk_count, 3);
    assert!(matches!(
        open(&pool, bob.id, files.id, 10).await,
        Err(DbError::NotFound)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn open_sessions_count_against_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::set_quota(&pool, alice.id, Some(100)).await;
    open(&pool, alice.id, files.id, 60).await.unwrap();
    assert!(matches!(
        open(&pool, alice.id, files.id, 60).await,
        Err(DbError::QuotaExceeded)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_are_recorded_once_and_listed_in_order(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    for index in [2, 0, 2] {
        assert_eq!(
            uploads::record_chunk(&pool, alice.id, session.id, index, later())
                .await
                .unwrap(),
            ChunkWrite::Recorded
        );
    }
    assert_eq!(
        uploads::received_chunks(&pool, session.id).await.unwrap(),
        [0, 2]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn claiming_is_exclusive_until_released(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();

    let claimed = uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();
    assert!(!claimed.is_open());
    assert!(matches!(
        uploads::claim(&pool, alice.id, session.id, later()).await,
        Err(DbError::UploadBusy)
    ));
    assert_eq!(
        uploads::record_chunk(&pool, alice.id, session.id, 0, later())
            .await
            .unwrap(),
        ChunkWrite::Busy
    );
    assert!(matches!(
        uploads::abort(&pool, alice.id, session.id).await,
        Err(DbError::UploadBusy)
    ));

    assert!(
        uploads::release(&pool, alice.id, session.id, later())
            .await
            .unwrap()
    );
    uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resumable_sessions_match_the_exact_file(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    let (name, sha) = (file_name(), common::hash('f'));
    let found = uploads::find_resumable(&pool, alice.id, files.id, &name, &sha, 10)
        .await
        .unwrap();
    assert_eq!(found.map(|s| s.id), Some(session.id));
    assert!(
        uploads::find_resumable(&pool, alice.id, files.id, &name, &sha, 11)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_are_private_and_abortable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    assert!(
        uploads::find(&pool, bob.id, session.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        uploads::abort(&pool, bob.id, session.id).await,
        Err(DbError::NotFound)
    ));
    uploads::abort(&pool, alice.id, session.id).await.unwrap();
    assert!(
        uploads::find(&pool, alice.id, session.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_sessions_are_hidden_and_purged(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    let live = open(&pool, alice.id, files.id, 4).await.unwrap();
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(session.id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        uploads::find(&pool, alice.id, session.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(uploads::delete_expired(&pool).await.unwrap(), [session.id]);
    assert_eq!(
        uploads::existing(&pool, &[session.id, live.id])
            .await
            .unwrap(),
        [live.id]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn release_is_owner_scoped_and_extends_the_expiry(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();

    assert!(
        !uploads::release(&pool, bob.id, session.id, later())
            .await
            .unwrap()
    );
    assert!(
        !uploads::find(&pool, alice.id, session.id)
            .await
            .unwrap()
            .unwrap()
            .is_open()
    );
    let until = OffsetDateTime::now_utc() + Duration::hours(48);
    assert!(
        uploads::release(&pool, alice.id, session.id, until)
            .await
            .unwrap()
    );
    let reopened = uploads::find(&pool, alice.id, session.id)
        .await
        .unwrap()
        .unwrap();
    assert!(reopened.is_open());
    assert!((reopened.expires_at - until).abs() < Duration::seconds(1));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn claims_get_a_generous_expiry(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    let soon = OffsetDateTime::now_utc() + Duration::minutes(1);
    let claimed = uploads::claim(&pool, alice.id, session.id, soon)
        .await
        .unwrap();
    assert!(claimed.expires_at > OffsetDateTime::now_utc() + Duration::hours(5));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_cleanup_skips_sessions_being_completed(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    uploads::claim(&pool, alice.id, session.id, later())
        .await
        .unwrap();
    let expire = |ago: &'static str| {
        sqlx::query("UPDATE upload_sessions SET expires_at = now() - $2::interval WHERE id = $1")
            .bind(session.id)
            .bind(ago)
    };
    expire("1 minute").execute(&pool).await.unwrap();
    assert!(uploads::delete_expired(&pool).await.unwrap().is_empty());
    expire("2 days").execute(&pool).await.unwrap();
    assert_eq!(uploads::delete_expired(&pool).await.unwrap(), [session.id]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunk_indexes_must_be_in_range(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    for index in [-1, 3] {
        assert!(matches!(
            uploads::record_chunk(&pool, alice.id, session.id, index, later()).await,
            Err(DbError::InvalidInput(_))
        ));
    }
    assert!(
        uploads::received_chunks(&pool, session.id)
            .await
            .unwrap()
            .is_empty()
    );
}
