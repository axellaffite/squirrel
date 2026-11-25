//! Friends: a one-way list of accounts to share with quickly.

mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::root_id;
use common::{CSRF, Instance, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn add(browser: &TestServer, me: &Value, username: &str) -> TestResponse {
    browser
        .post("/api/friends")
        .add_header(CSRF, csrf(me))
        .json(&json!({ "username": username }))
        .await
}

async fn remove(browser: &TestServer, me: &Value, username: &str) -> TestResponse {
    browser
        .delete(&format!("/api/friends/{username}"))
        .add_header(CSRF, csrf(me))
        .await
}

fn usernames<'a>(page: &'a Value, key: &str) -> Vec<&'a str> {
    page[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|person| person["username"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn friends_are_added_listed_and_removed(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, _)) = two_users(&instance).await;

    let added = add(&alice, &alice_me, " bob ").await;
    added.assert_status(StatusCode::CREATED);
    let friend: Value = added.json();
    assert_eq!(friend["username"], "bob");
    assert!(friend["display_name"].is_string());
    assert!(friend["added_at"].is_string());
    // Adding again is fine.
    add(&alice, &alice_me, "bob")
        .await
        .assert_status(StatusCode::CREATED);

    let page: Value = alice.get("/api/friends").await.json();
    assert_eq!(usernames(&page, "friends"), ["bob"]);
    // One way: Bob's list is untouched.
    let bobs: Value = bob.get("/api/friends").await.json();
    assert!(usernames(&bobs, "friends").is_empty());

    remove(&alice, &alice_me, "bob")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    remove(&alice, &alice_me, "bob")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let page: Value = alice.get("/api/friends").await.json();
    assert!(usernames(&page, "friends").is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn unknown_accounts_and_yourself_are_refused(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), _) = two_users(&instance).await;

    for username in ["nobody", "Not A Username!", "", "alice"] {
        add(&alice, &alice_me, username)
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    remove(&alice, &alice_me, "nobody")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn people_you_share_with_are_suggested_until_added(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (_, _)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    alice
        .post("/api/shares")
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({ "node_id": files, "username": "bob", "permission": "read" }))
        .await
        .assert_status(StatusCode::CREATED);

    let page: Value = alice.get("/api/friends").await.json();
    assert_eq!(usernames(&page, "suggestions"), ["bob"]);

    add(&alice, &alice_me, "bob")
        .await
        .assert_status(StatusCode::CREATED);
    let page: Value = alice.get("/api/friends").await.json();
    assert_eq!(usernames(&page, "friends"), ["bob"]);
    assert!(usernames(&page, "suggestions").is_empty());
}
