mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderValue, StatusCode};
use common::devices::{access, bearer, device_id, device_login, list_roots};
use common::files::{code, root_id};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (common::Instance, Value) {
    let instance = common::Instance::new(pool.clone());
    let me = common::setup_admin(&instance.browser(), "alice").await;
    (instance, me)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_bearer_reads_its_users_files(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let response = list_roots(&instance, &access(&login)).await;
    response.assert_status_ok();
    let names: Vec<String> = response.json::<Value>()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Documents", "Files", "Photos"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bad_bearers_are_unauthorized(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let expired = device_login(&instance, "alice", "Expired").await;
    let revoked = device_login(&instance, "alice", "Revoked").await;
    sqlx::query(
        "UPDATE device_access_tokens SET expires_at = now() - interval '1 second' \
                 WHERE device_id = $1",
    )
    .bind(device_id(&expired).parse::<Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE devices SET revoked_at = now() WHERE id = $1")
        .bind(device_id(&revoked).parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let unknown = format!("sqa_{}", "a".repeat(43));
    for token in [
        access(&expired),
        access(&revoked),
        unknown,
        "sqa_short".into(),
    ] {
        let response = list_roots(&instance, &token).await;
        response.assert_status(StatusCode::UNAUTHORIZED);
        assert_eq!(code(&response), "unauthorized");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_bearer_needs_no_csrf_token(pool: PgPool) {
    let (instance, me) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let files = root_id(&instance, &me, "Files").await;
    let (name, value) = bearer(&access(&login));
    instance
        .browser()
        .post("/api/folders")
        .add_header(name, value)
        .json(&json!({ "parent_id": files, "name": "From the phone" }))
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_bearer_header_overrides_the_cookie(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let browser = instance.browser();
    common::login(&browser, "alice").await;
    browser.get("/api/nodes").await.assert_status_ok();

    for token in [
        format!("sqa_{}", "a".repeat(43)),
        "garbage".into(),
        String::new(),
    ] {
        let (name, value) = bearer(&token);
        browser
            .get("/api/nodes")
            .add_header(name, value)
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_scheme_leaves_the_session_in_charge(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let basic = HeaderValue::from_static("Basic YWxpY2U6eA==");
    // Basic alone (a proxy's HTTP authentication, no session): 401.
    instance
        .browser()
        .get("/api/nodes")
        .add_header(AUTHORIZATION, basic.clone())
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    let browser = instance.browser();
    let me = common::login(&browser, "alice").await;
    browser
        .get("/api/nodes")
        .add_header(AUTHORIZATION, basic.clone())
        .await
        .assert_status_ok();
    let files = root_id(&instance, &me, "Files").await;
    let body = json!({ "parent_id": files, "name": "Behind a proxy" });
    // The session path still enforces CSRF.
    browser
        .post("/api/folders")
        .add_header(AUTHORIZATION, basic.clone())
        .json(&body)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    browser
        .post("/api/folders")
        .add_header(AUTHORIZATION, basic)
        .add_header(common::CSRF, common::csrf(&me))
        .json(&body)
        .await
        .assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_disabled_users_bearer_is_unauthorized(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    sqlx::query("UPDATE users SET disabled_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    list_roots(&instance, &access(&login))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn last_seen_moves_at_most_once_per_touch_interval(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let id: Uuid = device_id(&login).parse().unwrap();
    let last_seen = || async {
        sqlx::query_scalar::<_, time::OffsetDateTime>(
            "SELECT last_seen_at FROM devices WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let first = last_seen().await;
    list_roots(&instance, &access(&login))
        .await
        .assert_status_ok();
    assert_eq!(last_seen().await, first, "within the interval: untouched");

    sqlx::query("UPDATE devices SET last_seen_at = now() - interval '2 hours' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let stale = last_seen().await;
    list_roots(&instance, &access(&login))
        .await
        .assert_status_ok();
    assert!(last_seen().await > stale);
}
