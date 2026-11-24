//! The notification inbox: shares and comments announced to the right people,
//! never revealing what they can no longer see.

mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{root_id, upload};
use common::{CSRF, Instance, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn post(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .post(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

async fn folder(browser: &TestServer, me: &Value, parent: Uuid, name: &str) -> Uuid {
    let created = post(
        browser,
        me,
        "/api/folders",
        json!({ "parent_id": parent, "name": name }),
    )
    .await;
    created.assert_status(StatusCode::CREATED);
    created.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn inbox(browser: &TestServer) -> Vec<Value> {
    let page = browser.get("/api/notifications").await;
    page.assert_status_ok();
    page.json::<Value>()["items"].as_array().unwrap().clone()
}

async fn unread(browser: &TestServer) -> i64 {
    browser
        .get("/api/notifications/unread-count")
        .await
        .json::<Value>()["unread"]
        .as_i64()
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shares_and_comments_reach_the_inbox(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let note = upload(&alice, &alice_me, trip, "note.txt", b"hello").await;
    let note_id = note["id"].as_str().unwrap().to_owned();

    let share = post(
        &alice,
        &alice_me,
        "/api/shares",
        json!({ "node_id": trip, "username": "bob", "permission": "read" }),
    )
    .await;
    share.assert_status(StatusCode::CREATED);
    let share_id = share.json::<Value>()["id"].as_str().unwrap().to_owned();
    assert_eq!(unread(&bob).await, 1);
    let items = inbox(&bob).await;
    assert_eq!(items[0]["kind"], "share");
    assert_eq!(items[0]["actor"]["username"], "alice");
    assert_eq!(
        items[0]["subject"],
        json!({
            "kind": "folder", "id": trip, "name": "Trip", "mime": null, "folder_id": null
        })
    );
    assert_eq!(items[0]["permission"], "read");
    assert_eq!(items[0]["read"], false);

    // Bob comments on the file: Alice hears about it, Bob does not.
    post(
        &bob,
        &bob_me,
        "/api/comments",
        json!({ "node_id": note_id, "body": "  Looks\ngood  " }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    let items = inbox(&alice).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "comment");
    assert_eq!(items[0]["excerpt"], "Looks good");
    assert_eq!(items[0]["subject"]["kind"], "file");
    assert_eq!(items[0]["subject"]["folder_id"], json!(trip));
    assert_eq!(unread(&bob).await, 1);

    // Alice answers: Bob sees the file inside the shared folder he may open.
    post(
        &alice,
        &alice_me,
        "/api/comments",
        json!({ "node_id": note_id, "body": "Thanks" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    let items = inbox(&bob).await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["subject"]["folder_id"], json!(trip));

    // Marking read: one, then all.
    let first = items[0]["id"].clone();
    let marked = post(
        &bob,
        &bob_me,
        "/api/notifications/read",
        json!({ "ids": [first] }),
    )
    .await;
    marked.assert_status_ok();
    assert_eq!(marked.json::<Value>()["unread"], 1);
    let marked = post(&bob, &bob_me, "/api/notifications/read", json!({})).await;
    assert_eq!(marked.json::<Value>()["unread"], 0);
    assert!(inbox(&bob).await.iter().all(|item| item["read"] == true));

    // Once the share is revoked, Bob's inbox forgets the folder and its comments.
    alice
        .delete(&format!("/api/shares/{share_id}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(inbox(&bob).await.is_empty());
    assert_eq!(unread(&bob).await, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_inbox_is_private_and_bounded(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    for name in ["a", "b", "c"] {
        let id = folder(&alice, &alice_me, files, name).await;
        post(
            &alice,
            &alice_me,
            "/api/shares",
            json!({ "node_id": id, "username": "bob", "permission": "read" }),
        )
        .await
        .assert_status(StatusCode::CREATED);
    }
    let page: Value = bob.get("/api/notifications?limit=2").await.json();
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();
    let rest: Value = bob
        .get(&format!("/api/notifications?limit=2&cursor={cursor}"))
        .await
        .json();
    assert_eq!(rest["items"][0]["subject"]["name"], "a");
    assert!(rest["next_cursor"].is_null());
    bob.get("/api/notifications?cursor=garbage")
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // Alice cannot mark Bob's notifications read.
    let id = page["items"][0]["id"].clone();
    post(
        &alice,
        &alice_me,
        "/api/notifications/read",
        json!({ "ids": [id] }),
    )
    .await
    .assert_status_ok();
    assert_eq!(unread(&bob).await, 3);

    let too_many: Vec<Uuid> = (0..501).map(|_| Uuid::now_v7()).collect();
    post(
        &bob,
        &bob_me,
        "/api/notifications/read",
        json!({ "ids": too_many }),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}
