mod common;

use sqlx::PgPool;
use squirrel_db::{DbError, blobs, trash};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn entry_ids(pool: &PgPool, owner: Uuid) -> Vec<Uuid> {
    trash::list(pool, owner, None, 100)
        .await
        .unwrap()
        .items
        .iter()
        .map(|n| n.id)
        .collect()
}

async fn age(pool: &PgPool, id: Uuid, days: i32) {
    sqlx::query(
        "UPDATE nodes SET deleted_at = now() - make_interval(days => $2) WHERE trash_root = $1",
    )
    .bind(id)
    .bind(days)
    .execute(pool)
    .await
    .unwrap();
}

fn month_ago() -> OffsetDateTime {
    OffsetDateTime::now_utc() - Duration::days(30)
}

async fn ref_count(pool: &PgPool, fill: char) -> (i64, bool) {
    let blob = blobs::find(pool, &common::hash(fill))
        .await
        .unwrap()
        .unwrap();
    (blob.ref_count, blob.zero_since.is_some())
}

/// Files/F/G/C: C trashed first (own entry), then F.
async fn nested(pool: &PgPool, owner: Uuid) -> (Uuid, Uuid, Uuid) {
    let files = common::root(pool, owner, "Files").await;
    let f = common::raw_folder(pool, owner, files.id, "F").await;
    let g = common::raw_folder(pool, owner, f, "G").await;
    let c = common::raw_file(pool, owner, g, "c.txt", 'c', 3).await;
    trash::trash(pool, owner, c).await.unwrap();
    trash::trash(pool, owner, f).await.unwrap();
    (files.id, f, c)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_forever_keeps_nested_entries_restorable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let (files, f, c) = nested(&pool, alice.id).await;

    trash::delete_forever(&pool, alice.id, f).await.unwrap();

    assert_eq!(entry_ids(&pool, alice.id).await, [c]);
    let restored = trash::restore(&pool, alice.id, c).await.unwrap();
    assert_eq!(restored.parent_id, Some(files));
    assert_eq!(ref_count(&pool, 'c').await, (1, false));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn empty_counts_entries_including_nested_ones(pool: PgPool) {
    let alice = common::admin(&pool).await;
    nested(&pool, alice.id).await;
    assert_eq!(trash::empty(&pool, alice.id).await.unwrap(), 2);
    assert!(entry_ids(&pool, alice.id).await.is_empty());
    assert_eq!(ref_count(&pool, 'c').await, (0, true));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_keeps_recent_entries_nested_in_old_ones(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let (files, f, c) = nested(&pool, alice.id).await;
    age(&pool, f, 31).await;

    assert_eq!(
        trash::purge_older_than(&pool, month_ago(), 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(entry_ids(&pool, alice.id).await, [c]);
    let restored = trash::restore(&pool, alice.id, c).await.unwrap();
    assert_eq!(restored.parent_id, Some(files));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_counts_every_entry_across_batches(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let (_, f, c) = nested(&pool, alice.id).await;
    age(&pool, c, 40).await;
    age(&pool, f, 35).await;
    let files = common::root(&pool, bob.id, "Files").await;
    for (i, fill) in ['a', 'b', 'c'].into_iter().enumerate() {
        let id = common::raw_file(&pool, bob.id, files.id, &format!("{i}"), fill, 1).await;
        trash::trash(&pool, bob.id, id).await.unwrap();
        age(&pool, id, 31).await;
    }
    let keep = common::raw_file(&pool, bob.id, files.id, "keep", 'd', 1).await;
    trash::trash(&pool, bob.id, keep).await.unwrap();

    assert_eq!(
        trash::purge_older_than(&pool, month_ago(), 2)
            .await
            .unwrap(),
        5
    );
    assert!(entry_ids(&pool, alice.id).await.is_empty());
    assert_eq!(entry_ids(&pool, bob.id).await, [keep]);
    assert_eq!(ref_count(&pool, 'c').await, (0, true));
    assert_eq!(ref_count(&pool, 'd').await, (1, false));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_with_a_batch_of_one_terminates(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let (_, f, c) = nested(&pool, alice.id).await;
    age(&pool, c, 40).await;
    age(&pool, f, 35).await;
    assert_eq!(
        trash::purge_older_than(&pool, month_ago(), 1)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        trash::purge_older_than(&pool, month_ago(), 1)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_rejects_a_non_positive_batch(pool: PgPool) {
    for batch in [0, -1] {
        assert!(matches!(
            trash::purge_older_than(&pool, month_ago(), batch).await,
            Err(DbError::InvalidInput(_))
        ));
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bulk_deletes_aggregate_blob_references(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let dir = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let fills = ['a', 'b', 'c'];
    for i in 0..60 {
        let fill = fills[i % fills.len()];
        common::raw_file(&pool, alice.id, dir, &format!("{i}"), fill, 1).await;
    }
    common::raw_file(&pool, alice.id, files.id, "kept", 'a', 1).await;
    assert_eq!(ref_count(&pool, 'a').await, (21, false));

    trash::trash(&pool, alice.id, dir).await.unwrap();
    trash::delete_forever(&pool, alice.id, dir).await.unwrap();

    assert_eq!(ref_count(&pool, 'a').await, (1, false));
    assert_eq!(ref_count(&pool, 'b').await, (0, true));
    assert_eq!(ref_count(&pool, 'c').await, (0, true));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_purge_and_empty_sharing_blobs_do_not_fail(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let fills = ['a', 'b', 'c', 'd', 'e', 'f'];
    for user in [alice.id, bob.id] {
        let files = common::root(&pool, user, "Files").await;
        for batch in 0..6 {
            let dir = common::raw_folder(&pool, user, files.id, &format!("d{batch}")).await;
            for (i, fill) in fills.iter().rev().cycle().skip(batch).take(12).enumerate() {
                common::raw_file(&pool, user, dir, &format!("{i}"), *fill, 1).await;
            }
            trash::trash(&pool, user, dir).await.unwrap();
            age(&pool, dir, 31).await;
        }
    }

    let (purged, emptied_alice, emptied_bob) = tokio::join!(
        trash::purge_older_than(&pool, month_ago(), 1),
        trash::empty(&pool, alice.id),
        trash::empty(&pool, bob.id),
    );
    assert_eq!(
        purged.unwrap() + emptied_alice.unwrap() + emptied_bob.unwrap(),
        12
    );
    for fill in fills {
        assert_eq!(ref_count(&pool, fill).await, (0, true), "blob {fill}");
    }
}
