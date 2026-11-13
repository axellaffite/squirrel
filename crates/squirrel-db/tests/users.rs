use sqlx::PgPool;
use squirrel_core::{DisplayName, Role, Username};
use squirrel_db::DbError;
use squirrel_db::users::{self, NewUser};
use tokio::task::JoinSet;

fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn first_user_becomes_admin(pool: PgPool) {
    assert!(!users::any_exists(&pool).await.unwrap());
    let user = users::create_first_admin(&pool, &new_user("alice"))
        .await
        .unwrap();
    assert_eq!(user.role(), Role::Admin);
    assert!(users::any_exists(&pool).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn second_bootstrap_is_rejected(pool: PgPool) {
    users::create_first_admin(&pool, &new_user("alice"))
        .await
        .unwrap();
    let err = users::create_first_admin(&pool, &new_user("bob"))
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NotFirstUser));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_bootstrap_creates_exactly_one_admin(pool: PgPool) {
    let mut attempts = JoinSet::new();
    for i in 0..5 {
        let pool = pool.clone();
        attempts.spawn(async move {
            users::create_first_admin(&pool, &new_user(&format!("admin{i}"))).await
        });
    }
    let mut created = 0;
    while let Some(result) = attempts.join_next().await {
        match result.unwrap() {
            Ok(_) => created += 1,
            Err(DbError::NotFirstUser) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_users_by_username_and_id(pool: PgPool) {
    let alice = users::create_first_admin(&pool, &new_user("alice"))
        .await
        .unwrap();
    let by_name = users::find_by_username(&pool, &Username::parse("ALICE").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_name.id, alice.id);
    assert!(users::find_by_id(&pool, alice.id).await.unwrap().is_some());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn locks_after_five_failures_and_resets_on_success(pool: PgPool) {
    let alice = users::create_first_admin(&pool, &new_user("alice"))
        .await
        .unwrap();
    for _ in 0..4 {
        users::record_login_failure(&pool, alice.id).await.unwrap();
    }
    let now = time::OffsetDateTime::now_utc();
    assert!(
        !users::find_by_id(&pool, alice.id)
            .await
            .unwrap()
            .unwrap()
            .is_locked(now)
    );

    users::record_login_failure(&pool, alice.id).await.unwrap();
    assert!(
        users::find_by_id(&pool, alice.id)
            .await
            .unwrap()
            .unwrap()
            .is_locked(now)
    );

    users::record_login_success(&pool, alice.id).await.unwrap();
    let reset = users::find_by_id(&pool, alice.id).await.unwrap().unwrap();
    assert_eq!(reset.failed_logins, 0);
    assert!(!reset.is_locked(now));
}
