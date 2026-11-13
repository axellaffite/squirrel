use sqlx::PgPool;
use squirrel_core::{DisplayName, Username};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::{settings, users};
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
    NewSession {
        user_id,
        token_hash: hash,
        csrf_token: "csrf",
        user_agent: Some("test"),
        expires_at: OffsetDateTime::now_utc() + ttl,
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
    sessions::touch(&pool, created.id, new_expiry)
        .await
        .unwrap();

    let found = sessions::find_active(&pool, b"a").await.unwrap().unwrap();
    assert!(found.expires_at > created.expires_at);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn settings_default_and_update(pool: PgPool) {
    assert!(!settings::get(&pool).await.unwrap().invites_admin_only);
    let updated = settings::set_invites_admin_only(&pool, true).await.unwrap();
    assert!(updated.invites_admin_only);
}
