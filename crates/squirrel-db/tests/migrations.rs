use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_singleton_instance_settings(pool: PgPool) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

async fn insert_user(
    pool: &PgPool,
    username: &str,
    display_name: &str,
    failed_logins: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, username, display_name, password_hash, role, failed_logins) \
         VALUES (gen_random_uuid(), $1, $2, 'x', 'user', $3)",
    )
    .bind(username)
    .bind(display_name)
    .bind(failed_logins)
    .execute(pool)
    .await
    .map(|_| ())
}

fn constraint_of(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.constraint().map(str::to_owned),
        _ => None,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn accepts_a_valid_user_row(pool: PgPool) {
    insert_user(&pool, "alice", "Alice", 0).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rejects_uppercase_username(pool: PgPool) {
    let err = insert_user(&pool, "Alice", "Alice", 0).await.unwrap_err();
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("users_username_format")
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rejects_malformed_usernames(pool: PgPool) {
    for name in ["ab", ".alice", "al ice", &"a".repeat(33)] {
        assert!(
            insert_user(&pool, name, "Alice", 0).await.is_err(),
            "{name}"
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rejects_invalid_display_name_length(pool: PgPool) {
    assert!(insert_user(&pool, "alice", "", 0).await.is_err());
    assert!(
        insert_user(&pool, "alice", &"é".repeat(65), 0)
            .await
            .is_err()
    );
    insert_user(&pool, "alice", &"é".repeat(64), 0)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rejects_negative_failed_logins(pool: PgPool) {
    assert!(insert_user(&pool, "alice", "Alice", -1).await.is_err());
}

async fn insert_invitation(
    pool: &PgPool,
    email_hint: Option<&str>,
    max_uses: i32,
    used_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO invitations (id, token_hash, created_by, email_hint, max_uses, used_count, expires_at) \
         VALUES (gen_random_uuid(), gen_random_uuid()::text::bytea, \
                 (SELECT id FROM users LIMIT 1), $1, $2, $3, now() + interval '1 day')",
    )
    .bind(email_hint)
    .bind(max_uses)
    .bind(used_count)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invitation_constraints(pool: PgPool) {
    insert_user(&pool, "alice", "Alice", 0).await.unwrap();
    insert_invitation(&pool, Some("a@example.com"), 2, 2)
        .await
        .unwrap();
    assert!(insert_invitation(&pool, None, 1, 2).await.is_err());
    assert!(
        insert_invitation(&pool, Some(&"a".repeat(255)), 1, 0)
            .await
            .is_err()
    );
    insert_invitation(&pool, Some(&"a".repeat(254)), 1, 0)
        .await
        .unwrap();
}

fn all_versions() -> Vec<i64> {
    squirrel_db::MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| m.version)
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_migrated_database_has_nothing_pending(pool: PgPool) {
    assert!(
        squirrel_db::pending_migrations(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = false)]
async fn an_empty_database_has_every_migration_pending(pool: PgPool) {
    assert_eq!(
        squirrel_db::pending_migrations(&pool).await.unwrap(),
        all_versions()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_or_failed_migrations_are_pending(pool: PgPool) {
    let versions = all_versions();
    let (first, last) = (versions[0], *versions.last().unwrap());
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
        .bind(last)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET success = false WHERE version = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        squirrel_db::pending_migrations(&pool).await.unwrap(),
        [first, last]
    );
}
