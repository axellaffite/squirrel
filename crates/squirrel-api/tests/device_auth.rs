mod common;

use axum::http::StatusCode;
use common::devices::{
    access, access_is_live, device_body, device_id, device_login, refresh, refresh_token,
    try_device_login,
};
use common::files::code;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, RateLimit};
use uuid::Uuid;

async fn setup(pool: &PgPool) -> common::Instance {
    let instance = common::Instance::new(pool.clone());
    common::setup_admin(&instance.browser(), "alice").await;
    instance
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn device_login_returns_tokens_and_no_cookie(pool: PgPool) {
    let instance = setup(&pool).await;
    let response = try_device_login(&instance.browser(), "alice", "Pixel 8").await;
    response.assert_status(StatusCode::CREATED);
    assert!(response.maybe_header("set-cookie").is_none());
    let body: Value = response.json();
    assert!(access(&body).starts_with("sqa_"));
    assert!(refresh_token(&body).starts_with("sqr_"));
    assert_eq!(body["account"]["username"], "alice");
    assert_eq!(body["account"]["role"], "admin");
    assert_eq!(body["device"]["current"], true);
    assert_eq!(body["device"]["platform"], "android");

    let id: Uuid = device_id(&body).parse().unwrap();
    let (name, platform, version): (String, String, String) =
        sqlx::query_as("SELECT name, platform, app_version FROM devices WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (name.as_str(), platform.as_str(), version.as_str()),
        ("Pixel 8", "android", "0.1.0")
    );
    assert!(access_is_live(&pool, &access(&body)).await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn failed_device_logins_look_like_web_logins_and_count(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let mut wrong = device_body("alice", "Pixel");
    wrong["password"] = json!("wrong password!");
    let device = phone.post("/api/auth/device/login").json(&wrong).await;
    let web = phone
        .post("/api/auth/login")
        .json(&json!({ "username": "alice", "password": "wrong password!" }))
        .await;
    device.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(device.text(), web.text());
    assert_eq!(code(&device), "invalid_credentials");
    let unknown = try_device_login(&phone, "nobody", "Pixel").await;
    assert_eq!(unknown.text(), web.text());

    let failures: i32 = sqlx::query_scalar("SELECT failed_logins FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(failures, 2, "device and web failures are counted together");

    // The lockout is shared: after five failures, even the right password fails.
    for _ in 0..3 {
        phone.post("/api/auth/device/login").json(&wrong).await;
    }
    try_device_login(&phone, "alice", "Pixel")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_disabled_user_cannot_sign_a_device_in(pool: PgPool) {
    let instance = setup(&pool).await;
    sqlx::query("UPDATE users SET disabled_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    let response = try_device_login(&instance.browser(), "alice", "Pixel").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(code(&response), "invalid_credentials");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn device_fields_are_validated(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let mut cases = Vec::new();
    for name in [json!(""), json!("x".repeat(65))] {
        let mut body = device_body("alice", "x");
        body["device"]["name"] = name;
        cases.push(body);
    }
    let mut windows = device_body("alice", "x");
    windows["device"]["platform"] = json!("windows");
    cases.push(windows);
    let mut no_version = device_body("alice", "x");
    no_version["device"]
        .as_object_mut()
        .unwrap()
        .remove("app_version");
    cases.push(no_version);
    for body in cases {
        let response = phone.post("/api/auth/device/login").json(&body).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error");
    }
    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(devices, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refresh_rotates_and_detects_reuse(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let login = device_login(&instance, "alice", "Pixel").await;
    let (a1, r1) = (access(&login), refresh_token(&login));

    let second: Value = refresh(&phone, &r1).await.json();
    let (a2, r2) = (access(&second), refresh_token(&second));
    assert_ne!(r1, r2);
    assert!(access_is_live(&pool, &a1).await);
    assert!(access_is_live(&pool, &a2).await);

    // The response with r2 was lost (or a second request raced it): r1 again
    // within the grace window gives the same r2 and a fresh access token.
    let replayed = refresh(&phone, &r1).await;
    replayed.assert_status_ok();
    let replayed: Value = replayed.json();
    assert_eq!(refresh_token(&replayed), r2);
    assert_ne!(access(&replayed), a2);

    let third = refresh(&phone, &r2).await;
    third.assert_status_ok();
    let r3 = refresh_token(&third.json());
    let a3 = access(&third.json());

    // r2 is now the previous token: replayed after the grace window, it is reuse.
    sqlx::query("UPDATE devices SET rotated_at = now() - interval '31 seconds'")
        .execute(&pool)
        .await
        .unwrap();
    let reused = refresh(&phone, &r2).await;
    reused.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(code(&reused), "unauthorized");
    for token in [&a1, &a2, &a3] {
        assert!(!access_is_live(&pool, token).await);
    }
    refresh(&phone, &r3)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_retired_refresh_token_revokes_the_device(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let login = device_login(&instance, "alice", "Pixel").await;
    let r1 = refresh_token(&login);
    let r2 = refresh_token(&refresh(&phone, &r1).await.json());
    let second = refresh(&phone, &r2).await;
    let r3 = refresh_token(&second.json());
    // r1 was retired by the second rotation, whatever the time.
    refresh(&phone, &r1)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    assert!(!access_is_live(&pool, &access(&second.json())).await);
    refresh(&phone, &r3)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refresh_refuses_anything_but_a_refresh_token_without_writing(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let login = device_login(&instance, "alice", "Pixel").await;
    let me = common::login(&phone, "alice").await;
    let session_cookie = phone
        .get("/api/auth/me")
        .await
        .maybe_header("set-cookie")
        .map(|v| v.to_str().unwrap().to_owned());
    let before: (i64, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM device_access_tokens), last_seen_at::text FROM devices",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let candidates = [
        access(&login),
        me["csrf_token"].as_str().unwrap().to_owned(),
        session_cookie.unwrap_or_default(),
        "garbage".to_owned(),
        String::new(),
        format!("sqr_{}", "a".repeat(43)),
    ];
    for token in candidates {
        let response = refresh(&phone, &token).await;
        response.assert_status(StatusCode::UNAUTHORIZED);
        assert_eq!(code(&response), "unauthorized");
    }
    let after: (i64, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM device_access_tokens), last_seen_at::text FROM devices",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoked_expired_or_disabled_devices_cannot_refresh(pool: PgPool) {
    let instance = setup(&pool).await;
    let phone = instance.browser();
    let revoked = device_login(&instance, "alice", "Revoked").await;
    let expired = device_login(&instance, "alice", "Expired").await;
    let disabled = device_login(&instance, "alice", "Disabled").await;
    sqlx::query("UPDATE devices SET revoked_at = now() WHERE id = $1")
        .bind(device_id(&revoked).parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE devices SET refresh_expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(device_id(&expired).parse::<Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    for login in [&revoked, &expired] {
        refresh(&phone, &refresh_token(login))
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("UPDATE users SET disabled_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    refresh(&phone, &refresh_token(&disabled))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

fn limit(burst: u32) -> Option<RateLimit> {
    Some(RateLimit {
        replenish_every_secs: 60,
        burst,
    })
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn device_login_shares_the_login_bucket(pool: PgPool) {
    let instance = setup(&pool).await;
    let limited = instance.sharing(ApiConfig {
        login_rate_limit: limit(3),
        ..common::test_config()
    });
    let phone = limited.browser();
    phone
        .post("/api/auth/login")
        .json(&json!({ "username": "x", "password": "y" }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    for _ in 0..2 {
        try_device_login(&phone, "alice", "Pixel")
            .await
            .assert_status(StatusCode::CREATED);
    }
    let limited_out = try_device_login(&phone, "alice", "Pixel").await;
    limited_out.assert_status(StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(code(&limited_out), "rate_limited");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refresh_has_its_own_bucket(pool: PgPool) {
    let instance = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let limited = instance.sharing(ApiConfig {
        login_rate_limit: limit(1),
        device_refresh_rate_limit: limit(2),
        ..common::test_config()
    });
    let phone = limited.browser();
    phone
        .post("/api/auth/login")
        .json(&json!({ "username": "x", "password": "y" }))
        .await;
    try_device_login(&phone, "alice", "Pixel")
        .await
        .assert_status(StatusCode::TOO_MANY_REQUESTS);

    let first = refresh(&phone, &refresh_token(&login)).await;
    first.assert_status_ok();
    refresh(&phone, &refresh_token(&first.json()))
        .await
        .assert_status_ok();
    refresh(&phone, "garbage")
        .await
        .assert_status(StatusCode::TOO_MANY_REQUESTS);
}
