mod common;

use common::{admin, invited, photo, raw_file, raw_folder, raw_trash, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::DbError;
use squirrel_db::access::{self, Access};
use squirrel_db::albums::{self, Remover};
use squirrel_db::comments;
use squirrel_db::shares::{self, Permission, Subject};
use time::macros::datetime;
use uuid::Uuid;

async fn access_of(pool: &PgPool, user: Uuid, node: Uuid) -> Option<Access> {
    access::node(pool, user, node)
        .await
        .unwrap()
        .map(|found| found.access)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_folder_share_reaches_its_whole_subtree_only(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let day1 = raw_folder(&pool, alice.id, trip, "Day 1").await;
    let deep = raw_file(&pool, alice.id, day1, "notes.txt", 'a', 3).await;
    let outside = raw_file(&pool, alice.id, files, "secret.txt", 'b', 3).await;

    assert_eq!(access_of(&pool, alice.id, deep).await, Some(Access::Owner));
    assert_eq!(access_of(&pool, bob.id, deep).await, None);

    shares::upsert(
        &pool,
        alice.id,
        Subject::Node(trip),
        bob.id,
        Permission::Read,
    )
    .await
    .unwrap();
    assert_eq!(access_of(&pool, bob.id, trip).await, Some(Access::Read));
    assert_eq!(access_of(&pool, bob.id, deep).await, Some(Access::Read));
    assert_eq!(access_of(&pool, bob.id, outside).await, None);
    assert_eq!(access_of(&pool, bob.id, files).await, None);
    let found = access::node(&pool, bob.id, deep).await.unwrap().unwrap();
    assert_eq!(found.share_root, Some(trip));

    // A write share lower down is stronger there only; the root stays the top share.
    shares::upsert(
        &pool,
        alice.id,
        Subject::Node(day1),
        bob.id,
        Permission::Write,
    )
    .await
    .unwrap();
    assert_eq!(access_of(&pool, bob.id, trip).await, Some(Access::Read));
    let found = access::node(&pool, bob.id, deep).await.unwrap().unwrap();
    assert_eq!(
        (found.access, found.share_root),
        (Access::Write, Some(trip))
    );

    // Trashed nodes are never reachable through a share.
    raw_trash(&pool, deep).await;
    assert_eq!(access_of(&pool, bob.id, deep).await, None);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shares_are_upserted_listed_and_revoked_by_either_side(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let subject = Subject::Node(trip);

    let first = shares::upsert(&pool, alice.id, subject, bob.id, Permission::Read)
        .await
        .unwrap();
    let again = shares::upsert(&pool, alice.id, subject, bob.id, Permission::Write)
        .await
        .unwrap();
    assert_eq!(first, again);
    assert!(matches!(
        shares::upsert(&pool, alice.id, subject, alice.id, Permission::Read).await,
        Err(DbError::InvalidInput(_))
    ));

    let listed = shares::for_subject(&pool, subject).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].person_username, "bob");
    assert_eq!(listed[0].permission(), Permission::Write);

    let mine = shares::shared_with(&pool, bob.id).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].person_username, "alice");

    // A stranger cannot remove it; the recipient can leave it.
    assert!(!shares::delete(&pool, carol.id, first).await.unwrap());
    assert!(shares::delete(&pool, bob.id, first).await.unwrap());
    assert_eq!(access_of(&pool, bob.id, trip).await, None);

    // Trashed subjects leave "shared with me".
    shares::upsert(&pool, alice.id, subject, bob.id, Permission::Read)
        .await
        .unwrap();
    raw_trash(&pool, trip).await;
    assert!(shares::shared_with(&pool, bob.id).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn album_writers_add_their_photos_and_lose_them_with_the_share(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    let alice_photos = root(&pool, alice.id, "Photos").await.id;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let hers = photo(
        &pool,
        alice.id,
        alice_photos,
        "a.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    let his = photo(
        &pool,
        bob.id,
        bob_photos,
        "b.jpg",
        datetime!(2024-02-01 10:00),
    )
    .await;
    let album = albums::create(
        &pool,
        alice.id,
        &AlbumName::parse("Party").unwrap(),
        &[hers],
    )
    .await
    .unwrap();

    assert!(access::album(&pool, bob.id, album).await.unwrap().is_none());
    let share = shares::upsert(
        &pool,
        alice.id,
        Subject::Album(album),
        bob.id,
        Permission::Write,
    )
    .await
    .unwrap();
    assert_eq!(
        access::album(&pool, bob.id, album)
            .await
            .unwrap()
            .map(|a| a.access),
        Some(Access::Write)
    );
    // Bob may only add his own photos.
    assert_eq!(
        albums::add_items(&pool, album, bob.id, &[his, hers])
            .await
            .unwrap(),
        1
    );
    let items = albums::items(&pool, album, None, 10).await.unwrap().items;
    assert_eq!(items.len(), 2);

    // Album members (and the album's owner) read every photo of it.
    assert_eq!(access_of(&pool, alice.id, his).await, Some(Access::Read));
    assert_eq!(access_of(&pool, bob.id, hers).await, Some(Access::Read));
    assert_eq!(access_of(&pool, carol.id, hers).await, None);

    // Bob cannot remove Alice's photo; Alice can remove any.
    assert_eq!(
        albums::remove_items(&pool, album, Remover::Contributor(bob.id), &[hers])
            .await
            .unwrap(),
        0
    );

    // Downgrading to read, or revoking, takes Bob's photos out.
    shares::upsert(
        &pool,
        alice.id,
        Subject::Album(album),
        bob.id,
        Permission::Read,
    )
    .await
    .unwrap();
    assert_eq!(
        albums::items(&pool, album, None, 10)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(access_of(&pool, alice.id, his).await, None);
    shares::upsert(
        &pool,
        alice.id,
        Subject::Album(album),
        bob.id,
        Permission::Write,
    )
    .await
    .unwrap();
    albums::add_items(&pool, album, bob.id, &[his])
        .await
        .unwrap();
    assert!(shares::delete(&pool, alice.id, share).await.unwrap());
    assert_eq!(
        albums::items(&pool, album, None, 10)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        albums::remove_items(&pool, album, Remover::AlbumOwner, &[hers])
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn comments_are_edited_by_authors_and_deleted_by_authors_or_owner(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let doc = raw_file(&pool, alice.id, files, "plan.txt", 'c', 3).await;
    let subject = Subject::Node(doc);

    let hello = comments::create(&pool, alice.id, subject, bob.id, "Hello")
        .await
        .unwrap();
    assert_eq!(hello.author_username.as_deref(), Some("bob"));
    let reply = comments::create(&pool, alice.id, subject, alice.id, "Hi Bob")
        .await
        .unwrap();
    let page = comments::list(&pool, subject, None, 1).await.unwrap();
    assert_eq!((page.items.len(), page.has_more), (1, true));
    let cursor = comments::CommentCursor {
        created_at: page.items[0].created_at,
        id: page.items[0].id,
    };
    let rest = comments::list(&pool, subject, Some(cursor), 10)
        .await
        .unwrap();
    assert_eq!(rest.items[0].id, reply.id);

    assert!(
        !comments::edit(&pool, alice.id, hello.id, "x")
            .await
            .unwrap()
    );
    assert!(
        comments::edit(&pool, bob.id, hello.id, "Hello!")
            .await
            .unwrap()
    );
    let edited = comments::find(&pool, hello.id).await.unwrap().unwrap();
    assert_eq!(edited.body, "Hello!");
    assert!(edited.edited_at.is_some());

    // Bob cannot delete Alice's comment; Alice (owner) can delete Bob's.
    assert!(!comments::delete(&pool, bob.id, reply.id).await.unwrap());
    assert!(comments::delete(&pool, alice.id, hello.id).await.unwrap());
    assert_eq!(
        comments::subject_of(&pool, reply.id).await.unwrap(),
        Some(subject)
    );
    assert_eq!(
        comments::counts_for_nodes(&pool, &[doc]).await.unwrap(),
        [(doc, 1)]
    );
}
