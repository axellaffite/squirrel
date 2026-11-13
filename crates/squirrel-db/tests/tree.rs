mod common;

use sqlx::PgPool;
use squirrel_core::{NodeName, Username};
use squirrel_db::DbError;
use squirrel_db::nodes::{self, NodeChange};
use squirrel_db::{quota, users};

fn name(raw: &str) -> NodeName {
    NodeName::parse(raw).unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_folders_and_refuses_duplicate_names(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = nodes::create_folder(&pool, alice.id, files.id, &name("Taxes"))
        .await
        .unwrap();
    assert_eq!(
        (folder.name.as_str(), folder.parent_id),
        ("Taxes", Some(files.id))
    );
    let err = nodes::create_folder(&pool, alice.id, files.id, &name("Taxes"))
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NameConflict));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_cannot_be_created_in_files_or_foreign_folders(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    assert!(matches!(
        nodes::create_folder(&pool, alice.id, file, &name("x")).await,
        Err(DbError::NotFound)
    ));
    assert!(matches!(
        nodes::create_folder(&pool, bob.id, files.id, &name("x")).await,
        Err(DbError::NotFound)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renames_and_moves(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;

    let renamed = nodes::update(
        &pool,
        alice.id,
        file,
        &NodeChange {
            name: Some(name("b.txt")),
            parent_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.name, "b.txt");
    let moved = nodes::update(
        &pool,
        alice.id,
        file,
        &NodeChange {
            name: None,
            parent_id: Some(docs.id),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (moved.name.as_str(), moved.parent_id),
        ("b.txt", Some(docs.id))
    );

    common::raw_file(&pool, alice.id, docs.id, "c.txt", 'c', 1).await;
    let clash = nodes::update(
        &pool,
        alice.id,
        file,
        &NodeChange {
            name: Some(name("c.txt")),
            parent_id: None,
        },
    )
    .await;
    assert!(matches!(clash, Err(DbError::NameConflict)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn moves_cannot_create_cycles(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_folder(&pool, alice.id, files.id, "a").await;
    let b = common::raw_folder(&pool, alice.id, a, "b").await;
    for target in [a, b] {
        let result = nodes::update(
            &pool,
            alice.id,
            a,
            &NodeChange {
                name: None,
                parent_id: Some(target),
            },
        )
        .await;
        assert!(
            matches!(result, Err(DbError::MoveIntoSelf)),
            "into {target}"
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn roots_are_protected_and_foreign_nodes_invisible(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let rename_root = nodes::update(
        &pool,
        alice.id,
        files.id,
        &NodeChange {
            name: Some(name("Stuff")),
            parent_id: None,
        },
    )
    .await;
    assert!(matches!(rename_root, Err(DbError::ProtectedRoot)));

    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let bobs_files = common::root(&pool, bob.id, "Files").await;
    let steal = nodes::update(
        &pool,
        bob.id,
        file,
        &NodeChange {
            name: Some(name("mine.txt")),
            parent_id: None,
        },
    )
    .await;
    assert!(matches!(steal, Err(DbError::NotFound)));
    let push = nodes::update(
        &pool,
        alice.id,
        file,
        &NodeChange {
            name: None,
            parent_id: Some(bobs_files.id),
        },
    )
    .await;
    assert!(matches!(push, Err(DbError::NotFound)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn usage_counts_live_files_and_trash_separately(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 100).await;
    let trashed = common::raw_file(&pool, alice.id, files.id, "b.txt", 'b', 30).await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(trashed)
        .execute(&pool)
        .await
        .unwrap();

    let usage = quota::usage(&pool, alice.id).await.unwrap();
    assert_eq!(
        (usage.used_bytes, usage.trash_bytes, usage.quota_bytes),
        (100, 30, None)
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ensure_room_compares_used_plus_extra_with_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 60).await;
    assert!(
        users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100))
            .await
            .unwrap()
    );

    let mut tx = pool.begin().await.unwrap();
    let limit = quota::lock_owner(&mut tx, alice.id).await.unwrap();
    assert_eq!(limit, Some(100));
    quota::ensure_room(&mut tx, alice.id, limit, 40, quota::Pending::Ignore)
        .await
        .unwrap();
    let err = quota::ensure_room(&mut tx, alice.id, limit, 41, quota::Pending::Ignore)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::QuotaExceeded));
    quota::ensure_room(&mut tx, alice.id, None, i64::MAX, quota::Pending::Reserve)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn set_quota_reports_unknown_users(pool: PgPool) {
    common::admin(&pool).await;
    assert!(
        !users::set_quota(&pool, &Username::parse("nobody").unwrap(), Some(1))
            .await
            .unwrap()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn updates_bump_modified_at(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    sqlx::query("UPDATE nodes SET modified_at = now() - interval '1 day' WHERE id = $1")
        .bind(file)
        .execute(&pool)
        .await
        .unwrap();
    let before = nodes::find_live(&pool, alice.id, file)
        .await
        .unwrap()
        .unwrap()
        .modified_at;
    let renamed = nodes::update(
        &pool,
        alice.id,
        file,
        &NodeChange {
            name: Some(name("b.txt")),
            parent_id: None,
        },
    )
    .await
    .unwrap();
    assert!(renamed.modified_at > before);
}
