mod common;

use common::{admin, invited, photo, raw_file, raw_folder, raw_restore, raw_trash, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::albums::{self, Remover};
use squirrel_db::public_links::{self, NewPublicLink, PublicLinkRecord};
use squirrel_db::shares::Subject;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn token_hash(fill: u8) -> Vec<u8> {
    vec![fill; 32]
}

async fn link(
    pool: &PgPool,
    owner: Uuid,
    subject: Subject,
    fill: u8,
    expires_at: Option<OffsetDateTime>,
) -> PublicLinkRecord {
    public_links::create(
        pool,
        &NewPublicLink {
            owner_id: owner,
            subject,
            token_hash: &token_hash(fill),
            password_hash: None,
            expires_at,
        },
    )
    .await
    .unwrap()
}

async fn reaches(pool: &PgPool, link: &PublicLinkRecord, id: Uuid) -> bool {
    public_links::node_in_link(pool, link, id)
        .await
        .unwrap()
        .is_some()
}

async fn active(pool: &PgPool, fill: u8) -> bool {
    public_links::find_active(pool, &token_hash(fill))
        .await
        .unwrap()
        .is_some()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_folder_link_opens_its_subtree_only(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let day1 = raw_folder(&pool, alice.id, trip, "Day 1").await;
    let deep = raw_file(&pool, alice.id, day1, "notes.txt", 'a', 3).await;
    let outside = raw_file(&pool, alice.id, files, "secret.txt", 'b', 3).await;
    let bobs_files = root(&pool, bob.id, "Files").await.id;
    let bobs = raw_file(&pool, bob.id, bobs_files, "his.txt", 'c', 3).await;

    let link = link(&pool, alice.id, Subject::Node(trip), 1, None).await;
    assert!(reaches(&pool, &link, trip).await);
    assert!(reaches(&pool, &link, day1).await);
    assert!(reaches(&pool, &link, deep).await);
    assert!(
        !reaches(&pool, &link, files).await,
        "the parent stays private"
    );
    assert!(!reaches(&pool, &link, outside).await);
    assert!(!reaches(&pool, &link, bobs).await);
    assert!(!reaches(&pool, &link, Uuid::now_v7()).await);

    raw_trash(&pool, deep).await;
    assert!(
        !reaches(&pool, &link, deep).await,
        "trashed nodes are closed"
    );
    raw_restore(&pool, deep).await;
    assert!(reaches(&pool, &link, deep).await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_file_link_opens_that_file_only(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let shared = raw_file(&pool, alice.id, files, "a.txt", 'a', 3).await;
    let other = raw_file(&pool, alice.id, files, "b.txt", 'b', 3).await;

    let link = link(&pool, alice.id, Subject::Node(shared), 1, None).await;
    assert!(reaches(&pool, &link, shared).await);
    assert!(!reaches(&pool, &link, other).await);
    assert!(!reaches(&pool, &link, files).await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_album_link_opens_its_visible_photos(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Photos").await.id;
    let taken = datetime!(2026-05-01 10:00);
    let inside = photo(&pool, alice.id, files, "in.jpg", taken).await;
    let removed = photo(&pool, alice.id, files, "removed.jpg", taken).await;
    let outside = photo(&pool, alice.id, files, "out.jpg", taken).await;
    let album = albums::create(
        &pool,
        alice.id,
        &AlbumName::parse("Trip").unwrap(),
        &[inside, removed],
    )
    .await
    .unwrap();
    albums::remove_items(&pool, album, Remover::AlbumOwner, &[removed])
        .await
        .unwrap();

    let link = link(&pool, alice.id, Subject::Album(album), 1, None).await;
    assert!(reaches(&pool, &link, inside).await);
    assert!(!reaches(&pool, &link, removed).await);
    assert!(!reaches(&pool, &link, outside).await);
    raw_trash(&pool, inside).await;
    assert!(!reaches(&pool, &link, inside).await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn links_stop_when_expired_trashed_or_their_owner_is_disabled(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let now = OffsetDateTime::now_utc();

    link(&pool, alice.id, Subject::Node(trip), 1, None).await;
    link(
        &pool,
        alice.id,
        Subject::Node(trip),
        2,
        Some(now + Duration::days(1)),
    )
    .await;
    link(
        &pool,
        alice.id,
        Subject::Node(trip),
        3,
        Some(now - Duration::minutes(1)),
    )
    .await;
    assert!(active(&pool, 1).await);
    assert!(active(&pool, 2).await);
    assert!(!active(&pool, 3).await, "expired");
    assert!(!active(&pool, 9).await, "unknown token");

    let found = public_links::find_active(&pool, &token_hash(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.owner_display_name, alice.display_name);
    assert_eq!(found.link.subject(), Subject::Node(trip));

    raw_trash(&pool, trip).await;
    assert!(!active(&pool, 1).await, "trashed item");
    raw_restore(&pool, trip).await;
    assert!(active(&pool, 1).await);

    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(alice.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!active(&pool, 1).await, "disabled owner");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn links_are_listed_per_item_and_deleted_by_their_owner_only(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let other = raw_folder(&pool, alice.id, files, "Other").await;

    let first = link(&pool, alice.id, Subject::Node(trip), 1, None).await;
    let second = link(&pool, alice.id, Subject::Node(trip), 2, None).await;
    link(&pool, alice.id, Subject::Node(other), 3, None).await;
    let listed: Vec<Uuid> = public_links::for_subject(&pool, Subject::Node(trip))
        .await
        .unwrap()
        .iter()
        .map(|link| link.id)
        .collect();
    assert_eq!(listed, [second.id, first.id], "newest first");

    assert!(!public_links::delete(&pool, bob.id, first.id).await.unwrap());
    assert!(
        public_links::delete(&pool, alice.id, first.id)
            .await
            .unwrap()
    );
    assert!(!active(&pool, 1).await);
    assert!(active(&pool, 2).await);
    assert!(
        public_links::find(&pool, alice.id, first.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_the_item_deletes_its_links(pool: PgPool) {
    let alice = admin(&pool).await;
    let album = albums::create(&pool, alice.id, &AlbumName::parse("Trip").unwrap(), &[])
        .await
        .unwrap();
    link(&pool, alice.id, Subject::Album(album), 1, None).await;
    assert!(active(&pool, 1).await);
    albums::delete(&pool, album, alice.id).await.unwrap();
    assert!(!active(&pool, 1).await);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM public_links")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_link_is_only_to_the_owners_own_item(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let files = root(&pool, alice.id, "Files").await.id;
    let trip = raw_folder(&pool, alice.id, files, "Trip").await;
    let refused = public_links::create(
        &pool,
        &NewPublicLink {
            owner_id: bob.id,
            subject: Subject::Node(trip),
            token_hash: &token_hash(1),
            password_hash: None,
            expires_at: None,
        },
    )
    .await;
    assert!(
        refused.is_err(),
        "the composite key ties the item to its owner"
    );
}
