mod common;

use common::{admin, invited, raw_folder, root};
use sqlx::PgPool;
use squirrel_db::DbError;
use squirrel_db::friends;
use squirrel_db::shares::{self, Permission, Subject};
use uuid::Uuid;

fn usernames(list: &[friends::FriendRecord]) -> Vec<&str> {
    list.iter().map(|friend| friend.username.as_str()).collect()
}

async fn share_folder(pool: &PgPool, owner: Uuid, recipient: Uuid, name: &str) {
    let files = root(pool, owner, "Files").await.id;
    let folder = raw_folder(pool, owner, files, name).await;
    shares::upsert(
        pool,
        owner,
        Subject::Node(folder),
        recipient,
        Permission::Read,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn friends_are_one_way_sorted_by_name_and_idempotent(pool: PgPool) {
    let alice = admin(&pool).await;
    let carol = invited(&pool, &alice, "carol").await;
    let bob = invited(&pool, &alice, "bob").await;

    friends::add(&pool, alice.id, carol.id).await.unwrap();
    friends::add(&pool, alice.id, bob.id).await.unwrap();
    // Adding again changes nothing.
    friends::add(&pool, alice.id, bob.id).await.unwrap();

    let list = friends::list(&pool, alice.id).await.unwrap();
    assert_eq!(usernames(&list), ["bob", "carol"]);
    // One way: Bob's own list stays empty.
    assert!(friends::list(&pool, bob.id).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn you_cannot_befriend_yourself(pool: PgPool) {
    let alice = admin(&pool).await;
    let refused = friends::add(&pool, alice.id, alice.id).await;
    assert!(matches!(refused, Err(DbError::InvalidInput(_))));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn removing_a_friend_reports_whether_they_were_one(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    friends::add(&pool, alice.id, bob.id).await.unwrap();

    assert!(friends::remove(&pool, alice.id, bob.id).await.unwrap());
    assert!(!friends::remove(&pool, alice.id, bob.id).await.unwrap());
    assert!(friends::list(&pool, alice.id).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn disabled_accounts_are_hidden_and_deleted_ones_leave(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    friends::add(&pool, alice.id, bob.id).await.unwrap();
    friends::add(&pool, alice.id, carol.id).await.unwrap();

    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(bob.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(carol.id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(friends::list(&pool, alice.id).await.unwrap().is_empty());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM friends")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        rows, 1,
        "the deleted account's row cascaded, the disabled one stays"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn suggestions_are_sharing_partners_not_yet_friends(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let carol = invited(&pool, &alice, "carol").await;
    let dave = invited(&pool, &alice, "dave").await;
    let _erin = invited(&pool, &alice, "erin").await;

    // Alice shares with Bob, Carol shares with Alice, Dave shares with Alice
    // twice; Erin never shares anything.
    share_folder(&pool, alice.id, bob.id, "For Bob").await;
    share_folder(&pool, dave.id, alice.id, "From Dave 1").await;
    share_folder(&pool, carol.id, alice.id, "From Carol").await;
    share_folder(&pool, dave.id, alice.id, "From Dave 2").await;

    let suggested = friends::suggestions(&pool, alice.id).await.unwrap();
    // Most recent sharing first, each person once.
    assert_eq!(usernames(&suggested), ["dave", "carol", "bob"]);

    friends::add(&pool, alice.id, carol.id).await.unwrap();
    let suggested = friends::suggestions(&pool, alice.id).await.unwrap();
    assert_eq!(usernames(&suggested), ["dave", "bob"]);
}

/// `count` regular accounts named `pal-<label>-<n>`, straight in SQL.
async fn many_users(pool: &PgPool, label: &str, count: i32) -> Vec<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO users (id, username, display_name, password_hash, role) \
         SELECT gen_random_uuid(), 'pal-' || $1 || '-' || n, 'Pal ' || n, 'x', 'user' \
           FROM generate_series(1, $2) AS n RETURNING id",
    )
    .bind(label)
    .bind(count)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_cap_counts_live_friends_and_re_adding_at_the_cap_is_fine(pool: PgPool) {
    let alice = admin(&pool).await;
    let cap = i32::try_from(friends::MAX_FRIENDS).unwrap();
    let pals = many_users(&pool, "a", cap).await;
    sqlx::query("INSERT INTO friends (user_id, friend_id) SELECT $1, unnest($2::uuid[])")
        .bind(alice.id)
        .bind(&pals)
        .execute(&pool)
        .await
        .unwrap();
    let extra = many_users(&pool, "b", 1).await[0];

    let refused = friends::add(&pool, alice.id, extra).await;
    assert!(matches!(refused, Err(DbError::InvalidInput(_))));
    // Already a friend: fine even at the cap.
    let again = friends::add(&pool, alice.id, pals[0]).await.unwrap();
    assert_eq!(again.username, "pal-a-1");

    // A disabled friend is hidden, so it no longer takes a place.
    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(pals[1])
        .execute(&pool)
        .await
        .unwrap();
    let added = friends::add(&pool, alice.id, extra).await.unwrap();
    assert_eq!(added.username, "pal-b-1");
}
