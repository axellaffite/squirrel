use sqlx::PgPool;
use squirrel_core::{DisplayName, Role, Username};
use squirrel_db::DbError;
use squirrel_db::invitations::{self, InvitationStatus, NewInvitation};
use squirrel_db::users::{self, NewUser, UserRecord};
use time::{Duration, OffsetDateTime};

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

async fn invite(pool: &PgPool, by: &UserRecord, hash: &[u8], ttl: Duration) {
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
    .unwrap();
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
    invite(&pool, &alice, b"revoked", Duration::days(7)).await;
    let revoked_id = invitations::list_created_by(&pool, alice.id).await.unwrap()[0].id;
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
