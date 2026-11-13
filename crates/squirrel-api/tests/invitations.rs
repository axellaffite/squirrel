mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn create_invitation(server: &TestServer, me: &Value) -> Value {
    let response = server
        .post("/api/invitations")
        .add_header(common::CSRF, common::csrf(me))
        .json(&json!({ "email_hint": "bob@example.com" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}

async fn join(pool: &PgPool, token: &str, username: &str) -> (TestServer, Value) {
    let browser = common::server(pool.clone());
    let response = browser
        .post(&format!("/api/join/{token}"))
        .json(&common::account(username))
        .await;
    response.assert_status(StatusCode::CREATED);
    let me = response.json();
    (browser, me)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creating_an_invitation_requires_login(pool: PgPool) {
    let server = common::server(pool);
    server
        .post("/api/invitations")
        .json(&json!({}))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invited_user_joins_once(pool: PgPool) {
    let alice = common::server(pool.clone());
    let me = common::setup_admin(&alice, "alice").await;
    let created = create_invitation(&alice, &me).await;
    let token = created["token"].as_str().unwrap();
    assert_eq!(created["invitation"]["status"], "active");

    let anonymous = common::server(pool.clone());
    anonymous
        .get(&format!("/api/join/{token}"))
        .await
        .assert_json(&json!({ "valid": true }));

    let (bob, bob_me) = join(&pool, token, "bob").await;
    assert_eq!(bob_me["role"], "user");
    bob.get("/api/auth/me").await.assert_status_ok();

    anonymous
        .get(&format!("/api/join/{token}"))
        .await
        .assert_json(&json!({ "valid": false }));
    let reuse = anonymous
        .post(&format!("/api/join/{token}"))
        .json(&common::account("carol"))
        .await;
    reuse.assert_status(StatusCode::GONE);

    let list: Value = alice.get("/api/invitations").await.json();
    assert_eq!(list[0]["status"], "used");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn any_user_can_invite_and_revoke_their_own(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;

    let bobs = create_invitation(&bob, &bob_me).await;
    let id = bobs["invitation"]["id"].as_str().unwrap();

    alice
        .delete(&format!("/api/invitations/{id}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    bob.delete(&format!("/api/invitations/{id}"))
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let token = bobs["token"].as_str().unwrap();
    common::server(pool)
        .get(&format!("/api/join/{token}"))
        .await
        .assert_json(&json!({ "valid": false }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admin_can_restrict_invitations_to_admins(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;

    bob.get("/api/admin/settings")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    alice
        .patch("/api/admin/settings")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .json(&json!({ "invites_admin_only": true }))
        .await
        .assert_json(&json!({ "invites_admin_only": true }));

    bob.post("/api/invitations")
        .add_header(common::CSRF, common::csrf(&bob_me))
        .json(&json!({}))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    create_invitation(&alice, &alice_me).await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn join_rejects_taken_username_without_consuming_invite(pool: PgPool) {
    let alice = common::server(pool.clone());
    let me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let browser = common::server(pool.clone());
    browser
        .post(&format!("/api/join/{token}"))
        .json(&common::account("alice"))
        .await
        .assert_status(StatusCode::CONFLICT);
    join(&pool, &token, "bob").await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admin_settings_are_forbidden_to_regular_users(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;

    bob.get("/api/admin/settings")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    bob.patch("/api/admin/settings")
        .add_header(common::CSRF, common::csrf(&bob_me))
        .json(&json!({ "invites_admin_only": true }))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    alice
        .get("/api/admin/settings")
        .await
        .assert_json(&json!({ "invites_admin_only": false }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn users_only_see_their_own_invitations(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;
    let bobs = create_invitation(&bob, &bob_me).await;
    let alices_second = create_invitation(&alice, &alice_me).await;

    let ids = |list: Value| -> Vec<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|inv| inv["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let bob_list = ids(bob.get("/api/invitations").await.json());
    assert_eq!(bob_list, vec![bobs["invitation"]["id"].as_str().unwrap()]);

    let alice_list = ids(alice.get("/api/invitations").await.json());
    assert_eq!(alice_list.len(), 2);
    assert!(
        alice_list.contains(
            &alices_second["invitation"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        )
    );
    assert!(!alice_list.contains(&bobs["invitation"]["id"].as_str().unwrap().to_owned()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_reports_whether_the_user_can_invite(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    assert_eq!(alice_me["can_invite"], true);
    let token = create_invitation(&alice, &alice_me).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;
    assert_eq!(bob_me["can_invite"], true);

    alice
        .patch("/api/admin/settings")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .json(&json!({ "invites_admin_only": true }))
        .await
        .assert_status_ok();

    let bob_now: Value = bob.get("/api/auth/me").await.json();
    assert_eq!(bob_now["can_invite"], false);
    let alice_now: Value = alice.get("/api/auth/me").await.json();
    assert_eq!(alice_now["can_invite"], true);
    let bob_login: Value = common::server(pool)
        .post("/api/auth/login")
        .json(&json!({ "username": "bob", "password": common::PASSWORD }))
        .await
        .json();
    assert_eq!(bob_login["can_invite"], false);
}
