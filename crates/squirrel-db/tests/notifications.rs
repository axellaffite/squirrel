mod common;

use common::{admin, invited, photo, raw_file, raw_folder, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::notifications::{self, Kind};
use squirrel_db::shares::{self, Permission, Subject};
use squirrel_db::{albums, comments};
use time::macros::datetime;
use uuid::Uuid;

async fn inbox(pool: &PgPool, user: Uuid) -> Vec<notifications::NotificationRecord> {
    notifications::list(pool, user, None, 50)
        .await
        .unwrap()
        .items
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_new_share_is_announced_once(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;

    let share = shares::upsert(
        &pool,
        alice.id,
        Subject::Node(trip),
        bob.id,
        Permission::Read,
    )
    .await
    .unwrap();
    notifications::for_share(&pool, share).await.unwrap();
    // A permission change re-runs the hook: still one notification.
    shares::upsert(
        &pool,
        alice.id,
        Subject::Node(trip),
        bob.id,
        Permission::Write,
    )
    .await
    .unwrap();
    notifications::for_share(&pool, share).await.unwrap();

    let items = inbox(&pool, bob.id).await;
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.kind(), Kind::Share);
    assert_eq!(item.subject(), Subject::Node(trip));
    assert_eq!(item.subject_name, "Trip");
    assert!(item.subject_is_folder);
    assert_eq!(item.actor_username.as_deref(), Some("alice"));
    assert_eq!(item.permission.as_deref(), Some("write"));
    assert!(inbox(&pool, alice.id).await.is_empty());

    // Revoking the share takes its notification away.
    shares::delete(&pool, alice.id, share).await.unwrap();
    assert!(inbox(&pool, bob.id).await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_comment_reaches_everyone_who_sees_it_but_its_author(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    let dave = invited(&pool, &alice, "dave").await;
    let erin = invited(&pool, &alice, "erin").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let day1 = raw_folder(&pool, alice.id, trip, "Day 1").await;
    let beach = photo(
        &pool,
        alice.id,
        day1,
        "beach.jpg",
        datetime!(2026-07-01 10:00),
    )
    .await;
    let other = raw_file(&pool, alice.id, files, "other.txt", 'e', 3).await;

    // bob through the top folder, carol through an album holding the photo,
    // dave not at all, erin on another file.
    shares::upsert(
        &pool,
        alice.id,
        Subject::Node(trip),
        bob.id,
        Permission::Read,
    )
    .await
    .unwrap();
    let album = albums::create(
        &pool,
        alice.id,
        &AlbumName::parse("Summer").unwrap(),
        &[beach],
    )
    .await
    .unwrap();
    shares::upsert(
        &pool,
        alice.id,
        Subject::Album(album),
        carol.id,
        Permission::Read,
    )
    .await
    .unwrap();
    shares::upsert(
        &pool,
        alice.id,
        Subject::Node(other),
        erin.id,
        Permission::Read,
    )
    .await
    .unwrap();
    let _ = dave;

    let comment = comments::create(&pool, alice.id, Subject::Node(beach), bob.id, "Nice one")
        .await
        .unwrap();
    assert_eq!(
        notifications::for_comment(&pool, comment.id).await.unwrap(),
        2
    );

    for user in [alice.id, carol.id] {
        let items = inbox(&pool, user).await;
        assert_eq!(items.len(), 1, "{user}");
        assert_eq!(items[0].kind(), Kind::Comment);
        assert_eq!(items[0].comment_body.as_deref(), Some("Nice one"));
        assert_eq!(items[0].subject_name, "beach.jpg");
        assert_eq!(items[0].actor_username.as_deref(), Some("bob"));
    }
    for user in [bob.id, dave.id, erin.id] {
        assert!(inbox(&pool, user).await.is_empty());
    }

    // Deleting the comment takes its notifications away.
    comments::delete(&pool, bob.id, comment.id).await.unwrap();
    assert!(inbox(&pool, alice.id).await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn album_comments_reach_the_owner_and_recipients(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    let album = albums::create(&pool, alice.id, &AlbumName::parse("Party").unwrap(), &[])
        .await
        .unwrap();
    for user in [bob.id, carol.id] {
        shares::upsert(
            &pool,
            alice.id,
            Subject::Album(album),
            user,
            Permission::Write,
        )
        .await
        .unwrap();
    }
    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(carol.id)
        .execute(&pool)
        .await
        .unwrap();

    let comment = comments::create(&pool, alice.id, Subject::Album(album), alice.id, "Hi")
        .await
        .unwrap();
    assert_eq!(
        notifications::for_comment(&pool, comment.id).await.unwrap(),
        1
    );
    let items = inbox(&pool, bob.id).await;
    assert_eq!(items[0].subject(), Subject::Album(album));
    assert_eq!(items[0].subject_name, "Party");
    assert!(!items[0].subject_is_folder);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_inbox_pages_counts_and_marks_read(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let folder = raw_folder(&pool, alice.id, files, name).await;
        let share = shares::upsert(
            &pool,
            alice.id,
            Subject::Node(folder),
            bob.id,
            Permission::Read,
        )
        .await
        .unwrap();
        notifications::for_share(&pool, share).await.unwrap();
        ids.push(folder);
    }
    assert_eq!(notifications::unread_count(&pool, bob.id).await.unwrap(), 3);

    let first = notifications::list(&pool, bob.id, None, 2).await.unwrap();
    assert!(first.has_more);
    let names: Vec<_> = first
        .items
        .iter()
        .map(|n| n.subject_name.as_str())
        .collect();
    assert_eq!(names, ["c", "b"]);
    let last = first.items.last().unwrap();
    let cursor = notifications::NotificationCursor {
        created_at: last.created_at,
        id: last.id,
    };
    let second = notifications::list(&pool, bob.id, Some(cursor), 2)
        .await
        .unwrap();
    assert!(!second.has_more);
    assert_eq!(second.items[0].subject_name, "a");

    // Someone else cannot mark bob's notifications read.
    let one = [first.items[0].id];
    assert_eq!(
        notifications::mark_read(&pool, alice.id, Some(&one))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        notifications::mark_read(&pool, bob.id, Some(&one))
            .await
            .unwrap(),
        1
    );
    assert_eq!(notifications::unread_count(&pool, bob.id).await.unwrap(), 2);
    assert_eq!(
        notifications::mark_read(&pool, bob.id, None).await.unwrap(),
        2
    );
    assert_eq!(notifications::unread_count(&pool, bob.id).await.unwrap(), 0);

    notifications::delete(&pool, bob.id, &[second.items[0].id])
        .await
        .unwrap();
    assert_eq!(inbox(&pool, bob.id).await.len(), 2);
}
