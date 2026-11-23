//! Sharing folders, files and albums, and comment threads, across accounts.

mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{code, complete, put_chunk, root_id, start_upload, upload, user_id};
use common::photos::photo;
use common::{CSRF, Instance, account, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn post(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .post(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

async fn patch(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .patch(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

async fn delete(browser: &TestServer, me: &Value, url: &str) -> TestResponse {
    browser.delete(url).add_header(CSRF, csrf(me)).await
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

async fn share(
    browser: &TestServer,
    me: &Value,
    subject: Value,
    username: &str,
    permission: &str,
) -> Value {
    let mut body = subject;
    body["username"] = json!(username);
    body["permission"] = json!(permission);
    let created = post(browser, me, "/api/shares", body).await;
    created.assert_status(StatusCode::CREATED);
    created.json()
}

/// A third account, Carol, invited by Alice, in her own browser.
async fn carol(instance: &Instance, alice: &TestServer, alice_me: &Value) -> (TestServer, Value) {
    let created: Value = post(alice, alice_me, "/api/invitations", json!({}))
        .await
        .json();
    let token = created["token"].as_str().unwrap();
    let browser = instance.browser();
    let joined = browser
        .post(&format!("/api/join/{token}"))
        .json(&account("carol"))
        .await;
    joined.assert_status(StatusCode::CREATED);
    let me = joined.json();
    (browser, me)
}

fn names(listing: &Value) -> Vec<&str> {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_folder_shared_for_reading_is_seen_but_not_changed(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let day = folder(&alice, &alice_me, trip, "Day 1").await;
    let note = upload(&alice, &alice_me, day, "note.txt", b"hello bob").await;
    let note_id = note["id"].as_str().unwrap();

    // Before sharing: exactly as missing.
    bob.get(&format!("/api/nodes?parent_id={trip}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    bob.get(&format!("/api/files/{note_id}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    share(&alice, &alice_me, json!({ "node_id": trip }), "bob", "read").await;

    let listing: Value = bob
        .get(&format!("/api/nodes?parent_id={trip}"))
        .await
        .json();
    assert_eq!(names(&listing), ["Day 1"]);
    let details: Value = bob.get(&format!("/api/nodes/{note_id}")).await.json();
    assert_eq!(details["access"], "read");
    assert_eq!(details["shared_by"]["username"], "alice");
    // The path starts at the shared folder: Alice's "Files" stays private.
    let path: Vec<&str> = details["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(path, ["Trip", "Day 1"]);
    let content = bob.get(&format!("/api/files/{note_id}/content")).await;
    content.assert_status_ok();
    assert_eq!(content.as_bytes().as_ref(), b"hello bob");
    bob.get(&format!("/api/nodes?parent_id={files}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Reading only: no new folder, upload, rename or trash.
    let refused = post(
        &bob,
        &bob_me,
        "/api/folders",
        json!({ "parent_id": day, "name": "mine" }),
    )
    .await;
    refused.assert_status(StatusCode::FORBIDDEN);
    start_upload(&bob, &bob_me, day, "x.txt", b"x")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{note_id}"),
        json!({ "name": "y.txt" }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    delete(&bob, &bob_me, &format!("/api/nodes/{note_id}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);

    // "Shared with me", then leaving the share.
    let shared: Value = bob.get("/api/shared-with-me").await.json();
    assert_eq!(shared.as_array().unwrap().len(), 1);
    assert_eq!(shared[0]["node"]["name"], "Trip");
    assert_eq!(shared[0]["owner"]["username"], "alice");
    assert_eq!(shared[0]["permission"], "read");
    let share_id = shared[0]["share_id"].as_str().unwrap();
    delete(&bob, &bob_me, &format!("/api/shares/{share_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    bob.get(&format!("/api/files/{note_id}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn writers_work_inside_the_shared_folder_on_the_owners_account(pool: PgPool) {
    let instance = Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    // Alice already has these bytes elsewhere: Bob must not learn it.
    upload(&alice, &alice_me, files, "private.txt", b"same bytes").await;
    share(
        &alice,
        &alice_me,
        json!({ "node_id": trip }),
        "bob",
        "write",
    )
    .await;

    let sub = folder(&bob, &bob_me, trip, "From Bob").await;
    let started = start_upload(&bob, &bob_me, sub, "pic.txt", b"same bytes").await;
    started.assert_status(StatusCode::CREATED);
    let node = upload(&bob, &bob_me, sub, "pic.txt", b"same bytes").await;
    let node_id: Uuid = node["id"].as_str().unwrap().parse().unwrap();
    let owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM nodes WHERE id = $1")
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, user_id(&alice_me));

    // Rename and move inside the share.
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{node_id}"),
        json!({ "name": "renamed.txt", "parent_id": trip }),
    )
    .await
    .assert_status_ok();
    // The shared folder itself stays Alice's to rename, move or trash.
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{trip}"),
        json!({ "name": "Mine" }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    delete(&bob, &bob_me, &format!("/api/nodes/{trip}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    // Nothing leaves the share: Bob's own folder is another tree.
    let bob_files = root_id(&instance, &bob_me, "Files").await;
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{node_id}"),
        json!({ "parent_id": bob_files }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{node_id}"),
        json!({ "parent_id": files }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);

    // Trashing puts it in Alice's trash.
    delete(&bob, &bob_me, &format!("/api/nodes/{node_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let trash: Value = alice.get("/api/trash").await.json();
    assert_eq!(trash["items"][0]["node"]["name"], "renamed.txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_owner_shares_and_only_with_real_accounts(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let (carol, carol_me) = carol(&instance, &alice, &alice_me).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;

    for (username, expected) in [
        ("nobody", "validation_error"),
        ("alice", "validation_error"),
    ] {
        let refused = post(
            &alice,
            &alice_me,
            "/api/shares",
            json!({ "node_id": trip, "username": username, "permission": "read" }),
        )
        .await;
        refused.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&refused), expected);
    }
    let both = post(
        &alice,
        &alice_me,
        "/api/shares",
        json!({ "node_id": trip, "album_id": trip, "username": "bob", "permission": "read" }),
    )
    .await;
    both.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    share(
        &alice,
        &alice_me,
        json!({ "node_id": trip }),
        "bob",
        "write",
    )
    .await;
    // A writer does not re-share, nor see who else it is shared with.
    post(
        &bob,
        &bob_me,
        "/api/shares",
        json!({ "node_id": trip, "username": "carol", "permission": "read" }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    bob.get(&format!("/api/shares?node_id={trip}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    carol
        .get(&format!("/api/shares?node_id={trip}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let listed: Value = alice
        .get(&format!("/api/shares?node_id={trip}"))
        .await
        .json();
    assert_eq!(listed[0]["person"]["username"], "bob");
    assert_eq!(listed[0]["permission"], "write");

    // Changing the permission keeps one share; a stranger cannot remove it.
    let changed = share(&alice, &alice_me, json!({ "node_id": trip }), "bob", "read").await;
    assert_eq!(changed["id"], listed[0]["id"]);
    let share_id = changed["id"].as_str().unwrap();
    delete(&carol, &carol_me, &format!("/api/shares/{share_id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    delete(&alice, &alice_me, &format!("/api/shares/{share_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    bob.get(&format!("/api/nodes/{trip}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn album_writers_add_their_photos_and_take_them_back(pool: PgPool) {
    let instance = Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let alice_photos = root_id(&instance, &alice_me, "Photos").await;
    let bob_photos = root_id(&instance, &bob_me, "Photos").await;
    let hers = photo(
        &pool,
        &alice_me,
        alice_photos,
        "hers.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let his = photo(
        &pool,
        &bob_me,
        bob_photos,
        "his.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    let album: Value = post(
        &alice,
        &alice_me,
        "/api/albums",
        json!({ "name": "Party", "node_ids": [hers] }),
    )
    .await
    .json();
    let id = album["id"].as_str().unwrap().to_owned();
    assert_eq!(album["access"], "owner");

    bob.get(&format!("/api/albums/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    share(&alice, &alice_me, json!({ "album_id": id }), "bob", "write").await;
    let seen: Value = bob.get(&format!("/api/albums/{id}")).await.json();
    assert_eq!(seen["access"], "write");
    assert_eq!(seen["shared_by"]["username"], "alice");

    let added = post(
        &bob,
        &bob_me,
        &format!("/api/albums/{id}/items"),
        json!({ "node_ids": [his, hers] }),
    )
    .await;
    assert_eq!(added.json::<Value>()["changed"], 1);
    let items: Value = alice.get(&format!("/api/albums/{id}/items")).await.json();
    assert_eq!(names(&items), ["his.jpg", "hers.jpg"]);

    // Each sees the other's photo through the album, without its private details.
    let details: Value = alice.get(&format!("/api/photos/{his}")).await.json();
    assert_eq!(details["albums"], json!([]));
    bob.get(&format!("/api/photos/{hers}"))
        .await
        .assert_status_ok();

    // Bob cannot rename the album or remove Alice's photo.
    patch(
        &bob,
        &bob_me,
        &format!("/api/albums/{id}"),
        json!({ "name": "Mine" }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    let removed = post(
        &bob,
        &bob_me,
        &format!("/api/albums/{id}/items/remove"),
        json!({ "node_ids": [hers] }),
    )
    .await;
    assert_eq!(removed.json::<Value>()["changed"], 0);

    // Leaving the album takes Bob's photo out of it.
    let shared: Value = bob.get("/api/shared-with-me").await.json();
    assert_eq!(shared[0]["album"]["name"], "Party");
    let share_id = shared[0]["share_id"].as_str().unwrap();
    delete(&bob, &bob_me, &format!("/api/shares/{share_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let items: Value = alice.get(&format!("/api/albums/{id}/items")).await.json();
    assert_eq!(names(&items), ["hers.jpg"]);
    alice
        .get(&format!("/api/photos/{his}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn everyone_who_sees_something_discusses_it(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let (carol, carol_me) = carol(&instance, &alice, &alice_me).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    share(&alice, &alice_me, json!({ "node_id": trip }), "bob", "read").await;

    let posted = post(
        &bob,
        &bob_me,
        "/api/comments",
        json!({ "node_id": trip, "body": "  Lovely trip!  " }),
    )
    .await;
    posted.assert_status(StatusCode::CREATED);
    let comment: Value = posted.json();
    assert_eq!(comment["body"], "Lovely trip!");
    assert_eq!(comment["mine"], true);
    let comment_id = comment["id"].as_str().unwrap();
    post(
        &alice,
        &alice_me,
        "/api/comments",
        json!({ "node_id": trip, "body": "Thanks" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let thread: Value = alice
        .get(&format!("/api/comments?node_id={trip}"))
        .await
        .json();
    let items = thread["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["author"]["username"], "bob");
    assert_eq!(
        (items[0]["mine"].clone(), items[0]["can_delete"].clone()),
        (json!(false), json!(true))
    );

    // Only the author edits; strangers see nothing; empty comments are refused.
    patch(
        &alice,
        &alice_me,
        &format!("/api/comments/{comment_id}"),
        json!({ "body": "x" }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    patch(
        &bob,
        &bob_me,
        &format!("/api/comments/{comment_id}"),
        json!({ "body": "Edited" }),
    )
    .await
    .assert_status_ok();
    carol
        .get(&format!("/api/comments?node_id={trip}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    delete(&carol, &carol_me, &format!("/api/comments/{comment_id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    post(
        &bob,
        &bob_me,
        "/api/comments",
        json!({ "node_id": trip, "body": "   " }),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // The owner removes any comment on her folder.
    delete(&alice, &alice_me, &format!("/api/comments/{comment_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let thread: Value = bob
        .get(&format!("/api/comments?node_id={trip}"))
        .await
        .json();
    assert_eq!(thread["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_revoked_writer_cannot_finish_an_upload(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let shared = share(
        &alice,
        &alice_me,
        json!({ "node_id": trip }),
        "bob",
        "write",
    )
    .await;

    let started = start_upload(&bob, &bob_me, trip, "late.txt", b"late bytes").await;
    started.assert_status(StatusCode::CREATED);
    let upload_id = started.json::<Value>()["upload_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let share_id = shared["id"].as_str().unwrap();
    delete(&alice, &alice_me, &format!("/api/shares/{share_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    put_chunk(&bob, &bob_me, &upload_id, 0, b"late bytes")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    complete(&bob, &bob_me, &upload_id)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let listing: Value = alice
        .get(&format!("/api/nodes?parent_id={trip}"))
        .await
        .json();
    assert!(names(&listing).is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_owners_folders_stay_private_and_shares_stay_apart(pool: PgPool) {
    let instance = Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let one = folder(&alice, &alice_me, files, "One").await;
    let two = folder(&alice, &alice_me, files, "Two").await;
    share(&alice, &alice_me, json!({ "node_id": one }), "bob", "write").await;
    share(&alice, &alice_me, json!({ "node_id": two }), "bob", "write").await;
    let inside = folder(&bob, &bob_me, one, "Inside").await;

    // The shared folder's parent (Alice's "Files") is not revealed.
    let details: Value = bob.get(&format!("/api/nodes/{one}")).await.json();
    assert_eq!(details["node"]["parent_id"], Value::Null);
    let shared: Value = bob.get("/api/shared-with-me").await.json();
    assert!(
        shared
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["node"]["parent_id"].is_null())
    );
    let details: Value = bob.get(&format!("/api/nodes/{inside}")).await.json();
    assert_eq!(details["node"]["parent_id"], one.to_string());

    // Moving between two shares is refused.
    patch(
        &bob,
        &bob_me,
        &format!("/api/nodes/{inside}"),
        json!({ "parent_id": two }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);

    // A shared photo says what the viewer may do with it.
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let pic = photo(
        &pool,
        &alice_me,
        photos,
        "p.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    share(&alice, &alice_me, json!({ "node_id": pic }), "bob", "read").await;
    let seen: Value = bob.get(&format!("/api/photos/{pic}")).await.json();
    assert_eq!(seen["access"], "read");
    assert_eq!(seen["parent_id"], Value::Null);
    let own: Value = alice.get(&format!("/api/photos/{pic}")).await.json();
    assert_eq!(own["access"], "owner");
}
