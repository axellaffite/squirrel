use sqlx::PgPool;
use squirrel_db::settings;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn settings_default_and_update(pool: PgPool) {
    assert!(!settings::get(&pool).await.unwrap().invites_admin_only);
    let updated = settings::set_invites_admin_only(&pool, true).await.unwrap();
    assert!(updated.invites_admin_only);
}
