mod common;

use axum::http::StatusCode;
use common::devices::{
    access, bearer, device_id, device_login, list_roots, refresh, refresh_token,
};
use common::files::code;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::maintenance;
use uuid::Uuid;

async fn current(instance: &common::Instance, token: &str) -> axum_test::TestResponse {
    let (name, value) = bearer(token);
    instance
        .browser()
        .get("/api/devices/current")
        .add_header(name, value)
        .await
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_device_reads_and_renames_itself(pool: PgPool) {
    let instance = common::Instance::new(pool);
    common::setup_admin(&instance.browser(), "alice").await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let response = current(&instance, &access(&login)).await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(body["device"]["id"], login["device"]["id"]);
    assert_eq!(body["device"]["current"], true);
    assert_eq!(body["account"]["username"], "alice");
    assert_eq!(body["backup_folder_id"], Value::Null);

    let (name, value) = bearer(&access(&login));
    let renamed = instance
        .browser()
        .patch("/api/devices/current")
        .add_header(name.clone(), value.clone())
        .json(&json!({ "name": " Work phone " }))
        .await;
    renamed.assert_status_ok();
    assert_eq!(renamed.json::<Value>()["device"]["name"], "Work phone");
    let invalid = instance
        .browser()
        .patch("/api/devices/current")
        .add_header(name, value)
        .json(&json!({ "name": "x".repeat(65) }))
        .await;
    invalid.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_web_lists_only_its_users_devices(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, _), (bob, _)) = common::two_users(&instance).await;
    let first = device_login(&instance, "alice", "Old phone").await;
    let second = device_login(&instance, "alice", "Pixel").await;
    let alice_me = common::login(&alice, "alice").await;
    alice
        .delete(&format!("/api/devices/{}", device_id(&first)))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let listed: Vec<Value> = alice.get("/api/devices").await.json();
    let ids: Vec<&str> = listed.iter().map(|d| d["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [device_id(&second), device_id(&first)]);
    assert!(listed.iter().all(|d| d["current"] == false));
    assert!(listed[1]["revoked_at"].is_string());
    assert!(listed[0]["revoked_at"].is_null());
    let bobs: Vec<Value> = bob.get("/api/devices").await.json();
    assert!(bobs.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoking_from_the_web_is_immediate_and_owner_scoped(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let url = format!("/api/devices/{}", device_id(&login));

    let by_bob = bob
        .delete(&url)
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await;
    by_bob.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(code(&by_bob), "not_found");
    let no_csrf = alice.delete(&url).await;
    no_csrf.assert_status(StatusCode::FORBIDDEN);
    list_roots(&instance, &access(&login))
        .await
        .assert_status_ok();

    for _ in 0..2 {
        alice
            .delete(&url)
            .add_header(common::CSRF, common::csrf(&alice_me))
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    list_roots(&instance, &access(&login))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    refresh(&instance.browser(), &refresh_token(&login))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_device_signs_itself_out(pool: PgPool) {
    let instance = common::Instance::new(pool);
    common::setup_admin(&instance.browser(), "alice").await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let (name, value) = bearer(&access(&login));
    instance
        .browser()
        .post("/api/auth/device/logout")
        .add_header(name, value)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    current(&instance, &access(&login))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    refresh(&instance.browser(), &refresh_token(&login))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn logout_all_signs_out_the_users_devices_too(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), _) = common::two_users(&instance).await;
    let alices = device_login(&instance, "alice", "Pixel").await;
    let bobs = device_login(&instance, "bob", "iPhone").await;
    alice
        .post("/api/auth/logout-all")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    list_roots(&instance, &access(&alices))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    refresh(&instance.browser(), &refresh_token(&alices))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    list_roots(&instance, &access(&bobs))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn maintenance_deletes_expired_tokens_and_old_devices(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    common::setup_admin(&instance.browser(), "alice").await;
    let live = device_login(&instance, "alice", "Live").await;
    let old = device_login(&instance, "alice", "Old").await;
    let idle = device_login(&instance, "alice", "Idle").await;
    let id = |login: &Value| device_id(login).parse::<Uuid>().unwrap();
    sqlx::query("UPDATE devices SET revoked_at = now() - interval '31 days' WHERE id = $1")
        .bind(id(&old))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET refresh_expires_at = now() - interval '31 days' WHERE id = $1")
        .bind(id(&idle))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE device_access_tokens SET expires_at = now() - interval '1 minute' \
                 WHERE device_id = $1",
    )
    .bind(id(&live))
    .execute(&pool)
    .await
    .unwrap();

    let cleanup = maintenance::purge_devices(&instance.state).await.unwrap();
    assert_eq!(cleanup.stale_devices, 2);
    assert!(cleanup.expired_access_tokens >= 1);
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM devices")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [id(&live)]);
}
