mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::blobs;
use squirrel_db::nodes::{self, NewFile};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn link(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    fill: char,
) -> nodes::NodeRecord {
    let hash = common::hash(fill);
    let mut tx = pool.begin().await.unwrap();
    squirrel_db::quota::lock_owner(&mut tx, owner)
        .await
        .unwrap();
    blobs::insert_if_absent(&mut tx, &hash, 5, "text/plain")
        .await
        .unwrap();
    let node = nodes::insert_file(
        &mut tx,
        &NewFile {
            owner_id: owner,
            parent_id: parent,
            name: &NodeName::parse(name).unwrap(),
            blob: &hash,
            size: 5,
            mime: "text/plain",
            client_taken: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    node
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn insert_if_absent_is_idempotent(pool: PgPool) {
    let hash = common::hash('a');
    let mut conn = pool.acquire().await.unwrap();
    let first = blobs::insert_if_absent(&mut conn, &hash, 5, "text/plain")
        .await
        .unwrap();
    let second = blobs::insert_if_absent(&mut conn, &hash, 5, "image/png")
        .await
        .unwrap();
    assert_eq!(
        (first.size, second.mime.as_str(), second.ref_count),
        (5, "text/plain", 0)
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn file_nodes_get_free_names_and_count_references(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    let b = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    let c = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    assert_eq!(
        [a.name, b.name, c.name],
        ["a.txt", "a (1).txt", "a (2).txt"]
    );
    assert_eq!(
        blobs::find(&pool, &common::hash('a'))
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        3
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashed_files_do_not_block_names(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let first = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        link(&pool, alice.id, files.id, "a.txt", 'a').await.name,
        "a.txt"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn find_owned_only_sees_the_callers_content(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    link(&pool, alice.id, files.id, "a.txt", 'a').await;

    let mut conn = pool.acquire().await.unwrap();
    assert!(
        blobs::find_owned(&mut conn, alice.id, &common::hash('a'))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        blobs::find_owned(&mut conn, bob.id, &common::hash('a'))
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_long_unreferenced_blobs_are_collectable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let kept = link(&pool, alice.id, files.id, "kept.txt", 'c').await;
    let dropped = link(&pool, alice.id, files.id, "gone.txt", 'd').await;
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(dropped.id)
        .execute(&pool)
        .await
        .unwrap();

    let past = OffsetDateTime::now_utc() - Duration::hours(1);
    let future = OffsetDateTime::now_utc() + Duration::hours(1);
    assert!(
        blobs::unreferenced_since(&pool, past, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        blobs::unreferenced_since(&pool, future, 10).await.unwrap(),
        [common::hash('d')]
    );

    let mut conn = pool.acquire().await.unwrap();
    assert!(
        !blobs::delete_if_unreferenced(&mut conn, &common::hash('d'), past)
            .await
            .unwrap()
    );
    assert!(
        !blobs::delete_if_unreferenced(&mut conn, &common::hash('c'), future)
            .await
            .unwrap()
    );
    assert!(
        blobs::delete_if_unreferenced(&mut conn, &common::hash('d'), future)
            .await
            .unwrap()
    );
    assert!(
        blobs::find(&pool, &common::hash('d'))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        blobs::find(&pool, &common::hash('c'))
            .await
            .unwrap()
            .unwrap()
            .ref_count,
        1
    );
    assert_eq!(kept.name, "kept.txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_locks_exclude_other_connections_until_released(pool: PgPool) {
    let hash = common::hash('a');
    let mut first = pool.acquire().await.unwrap();
    blobs::lock_session(&mut first, &hash).await.unwrap();
    let mut second = pool.acquire().await.unwrap();
    let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(hash.lock_key())
        .fetch_one(&mut *second)
        .await
        .unwrap();
    assert!(!taken, "another connection must not get the lock");
    blobs::unlock_session(&mut first, &hash).await.unwrap();
    let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(hash.lock_key())
        .fetch_one(&mut *second)
        .await
        .unwrap();
    assert!(taken);
}
