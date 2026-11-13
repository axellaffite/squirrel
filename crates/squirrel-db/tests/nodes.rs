mod common;

use sqlx::PgPool;
use squirrel_core::NodeKind;
use squirrel_db::nodes::{self, ListCursor};

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn every_new_user_gets_the_default_roots(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    for user in [&alice, &bob] {
        let page = nodes::list_children(&pool, user.id, None, None, 10)
            .await
            .unwrap();
        let names: Vec<_> = page.items.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Documents", "Files", "Photos"]);
        assert!(
            page.items
                .iter()
                .all(|n| n.is_root() && n.kind() == NodeKind::Folder)
        );
        assert!(!page.has_more);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn children_are_listed_folders_first_and_paginated(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    common::raw_folder(&pool, alice.id, files.id, "Zeta").await;
    common::raw_folder(&pool, alice.id, files.id, "beta").await;
    common::raw_file(&pool, alice.id, files.id, "B.txt", 'b', 1).await;

    let mut seen = Vec::new();
    let mut cursor: Option<ListCursor> = None;
    loop {
        let page = nodes::list_children(&pool, alice.id, Some(files.id), cursor.as_ref(), 2)
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|n| n.name.clone()));
        let Some(last) = page.items.last() else { break };
        cursor = Some(ListCursor {
            kind: last.kind(),
            name: last.name.clone(),
        });
        if !page.has_more {
            break;
        }
    }
    // Folders first, then byte order ("B" < "a", "Z" < "b").
    assert_eq!(seen, ["Zeta", "beta", "B.txt", "a.txt"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn nodes_are_private_to_their_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;

    assert!(
        nodes::find_live(&pool, alice.id, file)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        nodes::find_live(&pool, bob.id, file)
            .await
            .unwrap()
            .is_none()
    );
    let bobs_view = nodes::list_children(&pool, bob.id, Some(files.id), None, 10)
        .await
        .unwrap();
    assert!(bobs_view.items.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn find_live_folder_ignores_files_and_trash(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    assert!(
        nodes::find_live_folder(&pool, alice.id, file)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        nodes::find_live_folder(&pool, alice.id, folder)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(folder)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        nodes::find_live_folder(&pool, alice.id, folder)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn path_lists_ancestors_from_the_root(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let taxes = common::raw_folder(&pool, alice.id, docs.id, "Taxes").await;
    let year = common::raw_folder(&pool, alice.id, taxes, "2026").await;

    let path = nodes::path_to(&pool, alice.id, year).await.unwrap();
    let names: Vec<_> = path.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Documents", "Taxes"]);
    assert!(
        nodes::path_to(&pool, alice.id, docs.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_come_before_files_across_a_page_boundary(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    common::raw_file(&pool, alice.id, files.id, "zz.txt", 'b', 1).await;
    common::raw_folder(&pool, alice.id, files.id, "m").await;
    common::raw_folder(&pool, alice.id, files.id, "z").await;

    let first = nodes::list_children(&pool, alice.id, Some(files.id), None, 2)
        .await
        .unwrap();
    let names: Vec<_> = first.items.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["m", "z"]);
    assert!(first.has_more);
    let last = first.items.last().unwrap();
    let cursor = ListCursor {
        kind: last.kind(),
        name: last.name.clone(),
    };
    let second = nodes::list_children(&pool, alice.id, Some(files.id), Some(&cursor), 2)
        .await
        .unwrap();
    let names: Vec<_> = second.items.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "zz.txt"]);
    assert!(!second.has_more);

    let after_file = ListCursor {
        kind: NodeKind::File,
        name: "a.txt".into(),
    };
    let rest = nodes::list_children(&pool, alice.id, Some(files.id), Some(&after_file), 5)
        .await
        .unwrap();
    let names: Vec<_> = rest.items.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["zz.txt"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn free_name_picks_the_first_unused_variant(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    for (i, taken) in [
        "report.pdf",
        "report (1).pdf",
        "report (2).pdf",
        "report (4).pdf",
    ]
    .into_iter()
    .enumerate()
    {
        let fill = char::from_digit(u32::try_from(i).unwrap(), 16).unwrap();
        common::raw_file(&pool, alice.id, files.id, taken, fill, 1).await;
    }
    let long = format!("{}.txt", "x".repeat(251));
    common::raw_file(&pool, alice.id, files.id, &long, 'e', 1).await;

    let mut tx = pool.begin().await.unwrap();
    let pick = |raw: &str| squirrel_core::NodeName::parse(raw).unwrap();
    let free = nodes::free_name(&mut tx, alice.id, files.id, &pick("report.pdf"))
        .await
        .unwrap();
    assert_eq!(free.as_str(), "report (3).pdf");
    let free = nodes::free_name(&mut tx, alice.id, files.id, &pick("other.pdf"))
        .await
        .unwrap();
    assert_eq!(free.as_str(), "other.pdf");
    let free = nodes::free_name(&mut tx, alice.id, files.id, &pick(&long))
        .await
        .unwrap();
    assert_eq!(free, pick(&long).variant(1));
}
