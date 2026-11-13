use sqlx::PgPool;
use squirrel_core::{DisplayName, Username};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::users;
use time::{Duration, OffsetDateTime};

async fn alice(pool: &PgPool) -> users::UserRecord {
    let new_user = users::NewUser {
        username: Username::parse("alice").unwrap(),
        display_name: DisplayName::parse("Alice").unwrap(),
        password_hash: "x".into(),
    };
    users::create_first_admin(pool, &new_user).await.unwrap()
}

fn session<'a>(user_id: uuid::Uuid, hash: &'a [u8], ttl: Duration) -> NewSession<'a> {
    with_absolute(user_id, hash, ttl, Duration::days(90))
}

fn with_absolute<'a>(
    user_id: uuid::Uuid,
    hash: &'a [u8],
    ttl: Duration,
    absolute: Duration,
) -> NewSession<'a> {
    let now = OffsetDateTime::now_utc();
    NewSession {
        user_id,
        token_hash: hash,
        csrf_token: "csrf",
        user_agent: Some("test"),
        expires_at: now + ttl,
        absolute_expires_at: now + absolute,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_only_unexpired_sessions(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"live", Duration::days(1)))
        .await
        .unwrap();
    sessions::create(&pool, &session(user.id, b"dead", Duration::seconds(-1)))
        .await
        .unwrap();

    assert!(
        sessions::find_active(&pool, b"live")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        sessions::find_active(&pool, b"dead")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_all_for_user_revokes_every_session(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"a", Duration::days(1)))
        .await
        .unwrap();
    sessions::create(&pool, &session(user.id, b"b", Duration::days(1)))
        .await
        .unwrap();

    assert_eq!(
        sessions::delete_all_for_user(&pool, user.id).await.unwrap(),
        2
    );
    assert!(sessions::find_active(&pool, b"a").await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn touch_extends_expiry(pool: PgPool) {
    let user = alice(&pool).await;
    let created = sessions::create(&pool, &session(user.id, b"a", Duration::hours(1)))
        .await
        .unwrap();
    let new_expiry = OffsetDateTime::now_utc() + Duration::days(30);
    assert!(
        sessions::touch(&pool, created.id, new_expiry)
            .await
            .unwrap()
    );

    let found = sessions::find_active(&pool, b"a").await.unwrap().unwrap();
    assert!(found.expires_at > created.expires_at);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn touch_does_not_revive_expired_session(pool: PgPool) {
    let user = alice(&pool).await;
    let dead = sessions::create(&pool, &session(user.id, b"dead", Duration::seconds(-1)))
        .await
        .unwrap();
    let new_expiry = OffsetDateTime::now_utc() + Duration::days(30);

    assert!(!sessions::touch(&pool, dead.id, new_expiry).await.unwrap());
    assert!(
        sessions::find_active(&pool, b"dead")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_expired_removes_only_expired_sessions(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"live", Duration::days(1)))
        .await
        .unwrap();
    sessions::create(&pool, &session(user.id, b"dead-1", Duration::seconds(-1)))
        .await
        .unwrap();
    sessions::create(&pool, &session(user.id, b"dead-2", Duration::hours(-1)))
        .await
        .unwrap();

    assert_eq!(sessions::delete_expired(&pool).await.unwrap(), 2);
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 1);
    assert!(
        sessions::find_active(&pool, b"live")
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_past_its_absolute_lifetime_is_not_active(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(
        &pool,
        &with_absolute(user.id, b"old", Duration::days(1), Duration::seconds(-1)),
    )
    .await
    .unwrap();
    assert!(
        sessions::find_active(&pool, b"old")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(sessions::delete_expired(&pool).await.unwrap(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn touch_never_extends_beyond_the_absolute_lifetime(pool: PgPool) {
    let user = alice(&pool).await;
    let created = sessions::create(
        &pool,
        &with_absolute(user.id, b"a", Duration::minutes(5), Duration::hours(1)),
    )
    .await
    .unwrap();
    let wanted = OffsetDateTime::now_utc() + Duration::days(30);
    assert!(sessions::touch(&pool, created.id, wanted).await.unwrap());

    let found = sessions::find_active(&pool, b"a").await.unwrap().unwrap();
    assert_eq!(found.expires_at, found.absolute_expires_at);
    assert_eq!(found.absolute_expires_at, created.absolute_expires_at);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_by_token_hash_removes_only_that_session(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"a", Duration::days(1)))
        .await
        .unwrap();
    sessions::create(&pool, &session(user.id, b"b", Duration::days(1)))
        .await
        .unwrap();

    sessions::delete_by_token_hash(&pool, b"a").await.unwrap();
    assert!(sessions::find_active(&pool, b"a").await.unwrap().is_none());
    assert!(sessions::find_active(&pool, b"b").await.unwrap().is_some());
}
