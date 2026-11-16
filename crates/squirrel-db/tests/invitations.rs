use sqlx::PgPool;
use squirrel_core::{DisplayName, Role, Username};
use squirrel_db::invitations::{self, InvitationStatus, NewInvitation};
use squirrel_db::users::{self, NewUser, UserRecord};
use squirrel_db::{DbError, settings};
use time::{Duration, OffsetDateTime};
use tokio::task::JoinSet;

fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

async fn admin(pool: &PgPool) -> UserRecord {
    users::create_first_admin(pool, &new_user("alice"))
        .await
        .unwrap()
}

async fn invite(
    pool: &PgPool,
    by: &UserRecord,
    hash: &[u8],
    ttl: Duration,
) -> invitations::InvitationRecord {
    invitations::create(
        pool,
        &NewInvitation {
            created_by: by.id,
            token_hash: hash,
            email_hint: None,
            expires_at: OffsetDateTime::now_utc() + ttl,
        },
    )
    .await
    .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn redeeming_creates_regular_user_linked_to_inviter(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;

    let bob = users::create_with_invitation(&pool, &new_user("bob"), b"token-1")
        .await
        .unwrap();
    assert_eq!(bob.role(), Role::User);
    assert_eq!(bob.invited_by, Some(alice.id));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invitation_is_single_use(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;
    users::create_with_invitation(&pool, &new_user("bob"), b"token-1")
        .await
        .unwrap();

    let err = users::create_with_invitation(&pool, &new_user("carol"), b"token-1")
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::InvitationInvalid));
    assert!(!invitations::is_redeemable(&pool, b"token-1").await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_and_revoked_invitations_are_rejected(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"expired", Duration::seconds(-1)).await;
    // Revoke by id: both invitations can share `created_at`, so list order is no guide.
    let revoked_id = invite(&pool, &alice, b"revoked", Duration::days(7))
        .await
        .id;
    assert!(
        invitations::revoke(&pool, revoked_id, alice.id)
            .await
            .unwrap()
    );

    for token in [
        b"expired".as_slice(),
        b"revoked".as_slice(),
        b"unknown".as_slice(),
    ] {
        let err = users::create_with_invitation(&pool, &new_user("bob"), token)
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::InvitationInvalid));
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn taken_username_does_not_consume_the_invitation(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;

    let err = users::create_with_invitation(&pool, &new_user("alice"), b"token-1")
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::UsernameTaken));
    assert!(invitations::is_redeemable(&pool, b"token-1").await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_creator_can_revoke_and_status_reflects_state(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;
    let bob = users::create_with_invitation(&pool, &new_user("bob"), b"token-1")
        .await
        .unwrap();
    invite(&pool, &alice, b"token-2", Duration::days(7)).await;

    let list = invitations::list_created_by(&pool, alice.id).await.unwrap();
    let now = OffsetDateTime::now_utc();
    assert_eq!(list[0].status(now), InvitationStatus::Active); // newest first
    assert_eq!(list[1].status(now), InvitationStatus::Used);

    assert!(
        !invitations::revoke(&pool, list[0].id, bob.id)
            .await
            .unwrap()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_redemptions_of_single_use_invite_admit_one(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;

    let mut attempts = JoinSet::new();
    for i in 0..10 {
        let pool = pool.clone();
        attempts.spawn(async move {
            users::create_with_invitation(&pool, &new_user(&format!("user{i}")), b"token-1").await
        });
    }
    let mut created = 0;
    while let Some(result) = attempts.join_next().await {
        match result.unwrap() {
            Ok(_) => created += 1,
            Err(DbError::InvitationInvalid) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invitation_from_disabled_creator_is_not_redeemable(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;
    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(alice.id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(!invitations::is_redeemable(&pool, b"token-1").await.unwrap());
    let err = users::create_with_invitation(&pool, &new_user("bob"), b"token-1")
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::InvitationInvalid));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admin_only_mode_blocks_invitations_from_regular_users(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"from-admin", Duration::days(7)).await;
    invite(&pool, &alice, b"for-bob", Duration::days(7)).await;
    let bob = users::create_with_invitation(&pool, &new_user("bob"), b"for-bob")
        .await
        .unwrap();
    invite(&pool, &bob, b"from-bob", Duration::days(7)).await;
    assert!(
        invitations::is_redeemable(&pool, b"from-bob")
            .await
            .unwrap()
    );

    settings::set_invites_admin_only(&pool, true).await.unwrap();

    assert!(
        !invitations::is_redeemable(&pool, b"from-bob")
            .await
            .unwrap()
    );
    let err = users::create_with_invitation(&pool, &new_user("carol"), b"from-bob")
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::InvitationInvalid));

    assert!(
        invitations::is_redeemable(&pool, b"from-admin")
            .await
            .unwrap()
    );
    users::create_with_invitation(&pool, &new_user("dave"), b"from-admin")
        .await
        .unwrap();
}
