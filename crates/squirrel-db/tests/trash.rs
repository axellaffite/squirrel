mod common;

use sqlx::PgPool;
use squirrel_db::nodes;
use squirrel_db::{DbError, blobs, quota, trash};
use time::{Duration, OffsetDateTime};

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashing_a_folder_trashes_its_subtree_as_one_entry(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let inner = common::raw_folder(&pool, alice.id, folder, "inner").await;
    let a = common::raw_file(&pool, alice.id, folder, "a.txt", 'a', 10).await;
    let b = common::raw_file(&pool, alice.id, inner, "b.txt", 'b', 5).await;

    trash::trash(&pool, alice.id, folder).await.unwrap();

    let entries = trash::list(&pool, alice.id, None, 10).await.unwrap();
    assert_eq!(
        entries.items.iter().map(|n| n.id).collect::<Vec<_>>(),
        [folder]
    );
    for id in [folder, inner, a, b] {
        assert!(
            nodes::find_live(&pool, alice.id, id)
                .await
                .unwrap()
                .is_none()
        );
    }
    let usage = quota::usage(&pool, alice.id).await.unwrap();
    assert_eq!((usage.used_bytes, usage.trash_bytes), (0, 15));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn roots_cannot_be_trashed(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    assert!(matches!(
        trash::trash(&pool, alice.id, files.id).await,
        Err(DbError::ProtectedRoot)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_returns_the_batch_where_it_was(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let a = common::raw_file(&pool, alice.id, folder, "a.txt", 'a', 10).await;
    trash::trash(&pool, alice.id, folder).await.unwrap();

    let restored = trash::restore(&pool, alice.id, folder).await.unwrap();
    assert_eq!(
        (restored.name.as_str(), restored.parent_id),
        ("dir", Some(files.id))
    );
    assert!(
        nodes::find_live(&pool, alice.id, a)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        trash::list(&pool, alice.id, None, 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_renames_on_conflict_and_falls_back_to_files(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let folder = common::raw_folder(&pool, alice.id, docs.id, "dir").await;
    let note = common::raw_file(&pool, alice.id, folder, "note.txt", 'd', 1).await;
    trash::trash(&pool, alice.id, note).await.unwrap();
    trash::trash(&pool, alice.id, folder).await.unwrap();
    common::raw_file(&pool, alice.id, files.id, "note.txt", 'e', 1).await;

    // The note's folder is in the trash: it goes to Files/, renamed.
    let restored = trash::restore(&pool, alice.id, note).await.unwrap();
    assert_eq!(
        (restored.name.as_str(), restored.parent_id),
        ("note (1).txt", Some(files.id))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn earlier_trashed_children_stay_in_the_trash(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let note = common::raw_file(&pool, alice.id, folder, "note.txt", 'd', 1).await;
    trash::trash(&pool, alice.id, note).await.unwrap();
    trash::trash(&pool, alice.id, folder).await.unwrap();
    trash::restore(&pool, alice.id, folder).await.unwrap();
    assert!(
        nodes::find_live(&pool, alice.id, note)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        trash::list(&pool, alice.id, None, 10)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_respects_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let old = common::raw_file(&pool, alice.id, files.id, "old.bin", 'f', 60).await;
    trash::trash(&pool, alice.id, old).await.unwrap();
    common::raw_file(&pool, alice.id, files.id, "new.bin", 'd', 60).await;
    common::set_quota(&pool, alice.id, Some(100)).await;
    assert!(matches!(
        trash::restore(&pool, alice.id, old).await,
        Err(DbError::QuotaExceeded)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_forever_and_empty_release_blobs(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let b = common::raw_file(&pool, alice.id, files.id, "b.txt", 'b', 1).await;
    trash::trash(&pool, alice.id, a).await.unwrap();
    trash::trash(&pool, alice.id, b).await.unwrap();

    trash::delete_forever(&pool, alice.id, a).await.unwrap();
    assert!(matches!(
        trash::delete_forever(&pool, alice.id, a).await,
        Err(DbError::NotFound)
    ));
    assert_eq!(trash::empty(&pool, alice.id).await.unwrap(), 1);
    for fill in ['a', 'b'] {
        assert_eq!(
            blobs::find(&pool, &common::hash(fill))
                .await
                .unwrap()
                .unwrap()
                .ref_count,
            0
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_only_removes_entries_older_than_the_cutoff(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let old = common::raw_file(&pool, alice.id, files.id, "old.txt", 'f', 1).await;
    let recent = common::raw_file(&pool, alice.id, files.id, "recent.txt", '1', 1).await;
    trash::trash(&pool, alice.id, old).await.unwrap();
    trash::trash(&pool, alice.id, recent).await.unwrap();
    sqlx::query("UPDATE nodes SET deleted_at = now() - interval '31 days' WHERE id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();

    let cutoff = OffsetDateTime::now_utc() - Duration::days(30);
    assert_eq!(
        trash::purge_older_than(&pool, cutoff, 100).await.unwrap(),
        1
    );
    let left = trash::list(&pool, alice.id, None, 10).await.unwrap();
    assert_eq!(
        left.items.iter().map(|n| n.id).collect::<Vec<_>>(),
        [recent]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_is_private(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    assert!(matches!(
        trash::trash(&pool, bob.id, a).await,
        Err(DbError::NotFound)
    ));
    trash::trash(&pool, alice.id, a).await.unwrap();
    assert!(
        trash::list(&pool, bob.id, None, 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(matches!(
        trash::restore(&pool, bob.id, a).await,
        Err(DbError::NotFound)
    ));
    assert!(matches!(
        trash::delete_forever(&pool, bob.id, a).await,
        Err(DbError::NotFound)
    ));
}

async fn all_pages(pool: &PgPool, owner: uuid::Uuid, limit: u32) -> Vec<uuid::Uuid> {
    let mut seen = Vec::new();
    let mut cursor: Option<trash::TrashCursor> = None;
    loop {
        let page = trash::list(pool, owner, cursor.as_ref(), limit)
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|n| n.id));
        if !page.has_more {
            return seen;
        }
        let last = page.items.last().unwrap();
        cursor = Some(trash::TrashCursor {
            deleted_at: last.deleted_at.unwrap(),
            id: last.id,
        });
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_listing_paginates_newest_first(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let mut ids = Vec::new();
    for (i, fill) in ['a', 'b', 'c'].into_iter().enumerate() {
        let id = common::raw_file(&pool, alice.id, files.id, &format!("{i}.txt"), fill, 1).await;
        trash::trash(&pool, alice.id, id).await.unwrap();
        sqlx::query(
            "UPDATE nodes SET deleted_at = now() - make_interval(mins => $2) WHERE id = $1",
        )
        .bind(id)
        .bind(10 - i32::try_from(i).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        ids.push(id);
    }
    ids.reverse();
    assert_eq!(all_pages(&pool, alice.id, 2).await, ids);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_listing_breaks_deleted_at_ties_by_id_desc(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let mut ids = Vec::new();
    for (i, fill) in ['a', 'b', 'c', 'd', 'e'].into_iter().enumerate() {
        let id = common::raw_file(&pool, alice.id, files.id, &format!("{i}.txt"), fill, 1).await;
        trash::trash(&pool, alice.id, id).await.unwrap();
        ids.push(id);
    }
    sqlx::query("UPDATE nodes SET deleted_at = '2026-11-01T00:00:00Z' WHERE owner_id = $1 AND trash_root IS NOT NULL")
        .bind(alice.id)
        .execute(&pool)
        .await
        .unwrap();
    ids.sort();
    ids.reverse();
    for limit in [1, 2, 3] {
        assert_eq!(
            all_pages(&pool, alice.id, limit).await,
            ids,
            "limit {limit}"
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_goes_to_files_when_the_original_folder_is_in_the_trash(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let folder = common::raw_folder(&pool, alice.id, docs.id, "dir").await;
    let note = common::raw_file(&pool, alice.id, folder, "note.txt", 'd', 1).await;
    trash::trash(&pool, alice.id, note).await.unwrap();
    trash::trash(&pool, alice.id, folder).await.unwrap();

    let restored = trash::restore(&pool, alice.id, note).await.unwrap();
    assert_eq!(
        (restored.name.as_str(), restored.parent_id),
        ("note.txt", Some(files.id))
    );
    assert!(
        nodes::find_live(&pool, alice.id, folder)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_sanitizes_a_stored_name_that_is_no_longer_valid(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let bad = common::raw_file(&pool, alice.id, files.id, "a/b\nc\u{202e}.txt", 'a', 1).await;
    let dots = common::raw_file(&pool, alice.id, files.id, "..", 'b', 1).await;
    trash::trash(&pool, alice.id, bad).await.unwrap();
    trash::trash(&pool, alice.id, dots).await.unwrap();

    let restored = trash::restore(&pool, alice.id, bad).await.unwrap();
    assert_eq!(restored.name, "a_bc.txt");
    let restored = trash::restore(&pool, alice.id, dots).await.unwrap();
    assert!(squirrel_core::NodeName::parse(&restored.name).is_ok());
    assert_ne!(restored.name, "..");
}
