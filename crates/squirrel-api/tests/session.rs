mod common;

use axum::http::StatusCode;
use axum_extra::extract::cookie::SameSite;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_requires_a_session(pool: PgPool) {
    let server = common::server(pool);
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_returns_the_logged_in_user(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let fetched: serde_json::Value = server.get("/api/auth/me").await.json();
    assert_eq!(fetched["username"], "alice");
    assert_eq!(fetched["csrf_token"], me["csrf_token"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn stale_session_is_extended_on_use(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() - interval '2 hours', \
         expires_at = now() + interval '1 minute'",
    )
    .execute(&pool)
    .await
    .unwrap();

    server.get("/api/auth/me").await.assert_status_ok();
    let extended: bool =
        sqlx::query_scalar("SELECT expires_at > now() + interval '29 days' FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(extended);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn state_changing_requests_need_the_csrf_header(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;

    let missing = server.post("/api/auth/logout").await;
    missing.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(
        missing.json::<serde_json::Value>()["error"]["code"],
        "csrf_invalid"
    );
    let wrong = server
        .post("/api/auth/logout")
        .add_header(common::CSRF, "not-the-token")
        .await;
    wrong.assert_status(StatusCode::FORBIDDEN);
    assert_eq!(
        wrong.json::<serde_json::Value>()["error"]["code"],
        "csrf_invalid"
    );
    server
        .post("/api/auth/logout")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn logout_all_revokes_other_browsers(pool: PgPool) {
    let first = common::server(pool.clone());
    let me = common::setup_admin(&first, "alice").await;

    let second = common::server(pool);
    second
        .post("/api/auth/login")
        .json(&serde_json::json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status_ok();

    first
        .post("/api/auth/logout-all")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    second
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

async fn session_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_is_never_extended_past_its_absolute_lifetime(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query(
        "UPDATE sessions SET last_seen_at = now() - interval '2 hours', \
         expires_at = now() + interval '1 minute', \
         absolute_expires_at = now() + interval '1 minute'",
    )
    .execute(&pool)
    .await
    .unwrap();

    server.get("/api/auth/me").await.assert_status_ok();
    let clamped: bool = sqlx::query_scalar("SELECT expires_at = absolute_expires_at FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(clamped);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_past_its_absolute_lifetime_is_rejected(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query("UPDATE sessions SET absolute_expires_at = now() - interval '1 second', expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_session_is_rejected(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn disabled_users_existing_session_is_rejected(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    sqlx::query("UPDATE users SET disabled_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    server
        .get("/api/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn login_replaces_the_session_the_browser_already_had(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;
    assert_eq!(session_count(&pool).await, 1);

    server
        .post("/api/auth/login")
        .json(&json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status_ok();
    assert_eq!(session_count(&pool).await, 1);
    server.get("/api/auth/me").await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_cookie_has_safe_flags_and_absolute_max_age(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/setup/admin")
        .json(&common::setup_request("alice"))
        .await;
    response.assert_status(StatusCode::CREATED);
    let cookie = response.cookie("squirrel_session");
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.same_site(), Some(SameSite::Lax));
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.max_age(), Some(time::Duration::days(90)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn logout_clears_the_cookie(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let response = server
        .post("/api/auth/logout")
        .add_header(common::CSRF, common::csrf(&me))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
    let cookie = response.cookie("squirrel_session");
    assert_eq!(cookie.value(), "");
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.max_age(), Some(time::Duration::ZERO));
}
