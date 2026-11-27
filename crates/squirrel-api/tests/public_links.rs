//! Public links: the owner makes, lists and revokes them; anyone holding one
//! sees what it opens, and nothing else, through either listener.

mod common;

use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum_test::{TestResponse, TestServer};
use common::files::{code, root_id, upload};
use common::photos::photo;
use common::{CSRF, Instance, csrf, test_config, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, PublicUrl};
use time::macros::datetime;
use uuid::Uuid;

const PUBLIC_URL: &str = "https://share.example:8443";
const LINK_PASSWORD: &str = "open sesame please";

fn config() -> ApiConfig {
    ApiConfig {
        public_url: Some(PublicUrl::parse(PUBLIC_URL).unwrap()),
        ..test_config()
    }
}

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
    id_of(&created.json())
}

fn id_of(value: &Value) -> Uuid {
    value["id"].as_str().unwrap().parse().unwrap()
}

async fn file(browser: &TestServer, me: &Value, parent: Uuid, name: &str) -> Uuid {
    let content = format!("{name} {}", Uuid::now_v7());
    id_of(&upload(browser, me, parent, name, content.as_bytes()).await)
}

/// Makes a link and returns its token.
async fn make_link(browser: &TestServer, me: &Value, body: Value) -> String {
    let created = post(browser, me, "/api/public-links", body).await;
    created.assert_status(StatusCode::CREATED);
    let created: Value = created.json();
    let url = created["url"].as_str().unwrap();
    let token = url
        .strip_prefix(&format!("{PUBLIC_URL}/s/"))
        .expect("the URL is under the public URL");
    assert_eq!(token.len(), 43);
    token.to_owned()
}

fn thumb(token: &str, id: Uuid) -> String {
    format!("/api/public/{token}/files/{id}/thumbnail?size=thumb")
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_folder_link_shows_its_subtree_and_nothing_else(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let day1 = folder(&alice, &alice_me, trip, "Day 1").await;
    let notes = file(&alice, &alice_me, day1, "notes.txt").await;
    let secret = file(&alice, &alice_me, files, "secret.txt").await;
    let bobs_files = root_id(&instance, &bob_me, "Files").await;
    let bobs = file(&bob, &bob_me, bobs_files, "his.txt").await;

    let token = make_link(&alice, &alice_me, json!({ "node_id": trip })).await;
    let visitor = instance.public_browser();

    let info: Value = visitor.get(&format!("/api/public/{token}")).await.json();
    assert_eq!(info["locked"], false);
    assert_eq!(info["kind"], "folder");
    assert_eq!(info["shared_by"], alice_me["display_name"]);
    assert_eq!(info["node"]["name"], "Trip");
    assert_eq!(
        info["node"]["parent_id"],
        Value::Null,
        "nothing above the link"
    );

    let top: Value = visitor
        .get(&format!("/api/public/{token}/nodes"))
        .await
        .json();
    assert_eq!(top["folder"]["id"], trip.to_string());
    assert_eq!(top["path"], json!([]));
    assert_eq!(top["items"][0]["name"], "Day 1");

    let inside: Value = visitor
        .get(&format!("/api/public/{token}/nodes?parent_id={day1}"))
        .await
        .json();
    assert_eq!(inside["path"], json!([{ "id": trip, "name": "Trip" }]));
    assert_eq!(inside["items"][0]["name"], "notes.txt");

    let content = visitor
        .get(&format!("/api/public/{token}/files/{notes}/content"))
        .await;
    content.assert_status_ok();
    assert!(content.text().starts_with("notes.txt "));

    // Outside the link: the parent, a sibling, another user's file, a made-up id.
    for outside in [files, secret, bobs, Uuid::now_v7()] {
        visitor
            .get(&format!("/api/public/{token}/nodes?parent_id={outside}"))
            .await
            .assert_status(StatusCode::NOT_FOUND);
        visitor
            .get(&format!("/api/public/{token}/files/{outside}/content"))
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }
    // A folder is not a file, a file is not a folder.
    visitor
        .get(&format!("/api/public/{token}/files/{day1}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    visitor
        .get(&format!("/api/public/{token}/nodes?parent_id={notes}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    // Not an album link.
    visitor
        .get(&format!("/api/public/{token}/album/items"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Trashing what is inside closes it; trashing the folder closes the link.
    alice
        .delete(&format!("/api/nodes/{notes}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    visitor
        .get(&format!("/api/public/{token}/files/{notes}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    alice
        .delete(&format!("/api/nodes/{trip}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    visitor
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_file_link_opens_that_file(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let shared = file(&alice, &alice_me, files, "a.txt").await;
    let other = file(&alice, &alice_me, files, "b.txt").await;

    let token = make_link(&alice, &alice_me, json!({ "node_id": shared })).await;
    let visitor = instance.public_browser();
    let info: Value = visitor.get(&format!("/api/public/{token}")).await.json();
    assert_eq!(info["kind"], "file");
    assert_eq!(info["node"]["id"], shared.to_string());
    assert_eq!(info["node"]["parent_id"], Value::Null);

    visitor
        .get(&format!(
            "/api/public/{token}/files/{shared}/content?download=true"
        ))
        .await
        .assert_status_ok();
    visitor
        .get(&format!("/api/public/{token}/files/{other}/content"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    visitor
        .get(&format!("/api/public/{token}/nodes"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_album_link_shows_its_photos(pool: PgPool) {
    let instance = Instance::with_config(pool.clone(), config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let at = datetime!(2026-05-01 10:00);
    let inside = photo(&pool, &alice_me, photos, "in.jpg", at).await;
    let outside = photo(&pool, &alice_me, photos, "out.jpg", at).await;
    let album: Value = post(
        &alice,
        &alice_me,
        "/api/albums",
        json!({ "name": "Party", "node_ids": [inside] }),
    )
    .await
    .json();
    let album_id = id_of(&album);

    let token = make_link(&alice, &alice_me, json!({ "album_id": album_id })).await;
    let visitor = instance.public_browser();
    let info: Value = visitor.get(&format!("/api/public/{token}")).await.json();
    assert_eq!(info["kind"], "album");
    assert_eq!(info["album"]["name"], "Party");
    assert_eq!(info["album"]["item_count"], 1);
    assert_eq!(info["album"]["cover_id"], inside.to_string());

    let items: Value = visitor
        .get(&format!("/api/public/{token}/album/items"))
        .await
        .json();
    let ids: Vec<&str> = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [inside.to_string()]);

    // Seeded thumbnails are recorded but not on disk: `not_ready` means the
    // link opened the photo, `not_found` that it did not.
    assert_eq!(
        code(&visitor.get(&thumb(&token, inside)).await),
        "not_ready"
    );
    assert_eq!(
        code(&visitor.get(&thumb(&token, outside)).await),
        "not_found"
    );
    visitor
        .get(&format!("/api/public/{token}/nodes"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_password_locks_everything_until_it_is_given(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let notes = file(&alice, &alice_me, trip, "notes.txt").await;
    let body = json!({ "node_id": trip, "password": LINK_PASSWORD });
    let token = make_link(&alice, &alice_me, body.clone()).await;
    let other_token = make_link(&alice, &alice_me, body).await;

    let visitor = instance.public_browser();
    let info: Value = visitor.get(&format!("/api/public/{token}")).await.json();
    assert_eq!(
        info,
        json!({ "locked": true, "kind": null, "shared_by": null, "expires_at": null,
                "node": null, "album": null })
    );
    for url in [
        format!("/api/public/{token}/nodes"),
        format!("/api/public/{token}/files/{notes}/content"),
    ] {
        let locked = visitor.get(&url).await;
        locked.assert_status(StatusCode::UNAUTHORIZED);
        assert_eq!(code(&locked), "link_locked");
    }

    let wrong = visitor
        .post(&format!("/api/public/{token}/unlock"))
        .json(&json!({ "password": "not the password" }))
        .await;
    wrong.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(code(&wrong), "invalid_credentials");

    let unlocked = visitor
        .post(&format!("/api/public/{token}/unlock"))
        .json(&json!({ "password": LINK_PASSWORD }))
        .await;
    unlocked.assert_status(StatusCode::NO_CONTENT);
    let cookie = unlocked.header(SET_COOKIE);
    let cookie = cookie.to_str().unwrap();
    assert!(
        cookie.contains(&format!("Path=/api/public/{token}")),
        "{cookie}"
    );
    assert!(cookie.contains("HttpOnly"), "{cookie}");

    let info: Value = visitor.get(&format!("/api/public/{token}")).await.json();
    assert_eq!(info["locked"], false);
    visitor
        .get(&format!("/api/public/{token}/files/{notes}/content"))
        .await
        .assert_status_ok();

    // The other link (same item, same password) stays locked.
    let other: Value = visitor
        .get(&format!("/api/public/{other_token}"))
        .await
        .json();
    assert_eq!(other["locked"], true);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_owner_lists_and_revokes_links_and_others_cannot(pool: PgPool) {
    let instance = Instance::with_config(pool.clone(), config());
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;

    let token = make_link(
        &alice,
        &alice_me,
        json!({ "node_id": trip, "password": LINK_PASSWORD,
                "expires_at": "2099-01-01T00:00:00Z" }),
    )
    .await;
    let listed: Value = alice
        .get(&format!("/api/public-links?node_id={trip}"))
        .await
        .json();
    assert_eq!(listed["enabled"], true);
    let link = &listed["links"][0];
    assert_eq!(link["has_password"], true);
    assert_eq!(link["expired"], false);
    assert_eq!(link["expires_at"], "2099-01-01T00:00:00Z");
    assert!(link.get("url").is_none(), "the URL is shown only once");
    let link_id = link["id"].as_str().unwrap().to_owned();

    // Bob: a stranger to the folder, then a reader of it. Neither manages links.
    let stranger = bob.get(&format!("/api/public-links?node_id={trip}")).await;
    stranger.assert_status(StatusCode::NOT_FOUND);
    post(
        &alice,
        &alice_me,
        "/api/shares",
        json!({ "node_id": trip, "username": "bob", "permission": "write" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    bob.get(&format!("/api/public-links?node_id={trip}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    post(
        &bob,
        &bob_me,
        "/api/public-links",
        json!({ "node_id": trip }),
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    bob.delete(&format!("/api/public-links/{link_id}"))
        .add_header(CSRF, csrf(&bob_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Revoked: gone at once.
    alice
        .delete(&format!("/api/public-links/{link_id}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    instance
        .public_browser()
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let listed: Value = alice
        .get(&format!("/api/public-links?node_id={trip}"))
        .await
        .json();
    assert_eq!(listed["links"], json!([]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn links_need_a_future_expiry_a_valid_password_and_a_public_url(pool: PgPool) {
    let instance = Instance::with_config(pool.clone(), config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;

    let past = post(
        &alice,
        &alice_me,
        "/api/public-links",
        json!({ "node_id": trip, "expires_at": "2020-01-01T00:00:00Z" }),
    )
    .await;
    past.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let short = post(
        &alice,
        &alice_me,
        "/api/public-links",
        json!({ "node_id": trip, "password": "short" }),
    )
    .await;
    short.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    post(&alice, &alice_me, "/api/public-links", json!({}))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // A link that expires closes then.
    let token = make_link(
        &alice,
        &alice_me,
        json!({ "node_id": trip, "expires_at": "2099-01-01T00:00:00Z" }),
    )
    .await;
    sqlx::query("UPDATE public_links SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    instance
        .public_browser()
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let listed: Value = alice
        .get(&format!("/api/public-links?node_id={trip}"))
        .await
        .json();
    assert_eq!(listed["links"][0]["expired"], true);

    // Without a public URL, links cannot be made, and existing ones still open.
    let open = make_link(&alice, &alice_me, json!({ "node_id": trip })).await;
    let unset = instance.sharing(test_config());
    let browser = unset.browser();
    let me = common::login(&browser, "alice").await;
    let refused = post(
        &browser,
        &me,
        "/api/public-links",
        json!({ "node_id": trip }),
    )
    .await;
    refused.assert_status(StatusCode::CONFLICT);
    let listed: Value = browser
        .get(&format!("/api/public-links?node_id={trip}"))
        .await
        .json();
    assert_eq!(listed["enabled"], false);
    unset
        .public_browser()
        .get(&format!("/api/public/{open}"))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_disabled_owners_links_close(pool: PgPool) {
    let instance = Instance::with_config(pool.clone(), config());
    let (_, (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &bob_me, "Files").await;
    let token = make_link(&bob, &bob_me, json!({ "node_id": files })).await;
    let visitor = instance.public_browser();
    visitor
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status_ok();
    sqlx::query("UPDATE users SET disabled_at = now() WHERE username = 'bob'")
        .execute(&pool)
        .await
        .unwrap();
    visitor
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn zips_hold_files_of_their_link_only(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    let a = file(&alice, &alice_me, trip, "a.txt").await;
    let b = file(&alice, &alice_me, trip, "b.txt").await;
    let secret = file(&alice, &alice_me, files, "secret.txt").await;
    let token = make_link(&alice, &alice_me, json!({ "node_id": trip })).await;
    let other = make_link(&alice, &alice_me, json!({ "node_id": trip })).await;
    let visitor = instance.public_browser();
    let archive = |token: &str| format!("/api/public/{token}/archive");

    let ticket = visitor
        .post(&archive(&token))
        .json(&json!({ "node_ids": [a, b] }))
        .await;
    ticket.assert_status(StatusCode::CREATED);
    let ticket: Value = ticket.json();
    assert_eq!(ticket["count"], 2);
    let ticket = ticket["ticket"].as_str().unwrap();
    let zip = visitor.get(&format!("{}/{ticket}", archive(&token))).await;
    zip.assert_status_ok();
    assert_eq!(zip.as_bytes().len(), ticket_size(&zip));
    // Another link's ticket is not this one's.
    visitor
        .get(&format!("{}/{ticket}", archive(&other)))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    visitor
        .post(&archive(&token))
        .json(&json!({ "node_ids": [a, secret] }))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    visitor
        .post(&archive(&token))
        .json(&json!({ "node_ids": [trip] }))
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // A file moved out of the link after the ticket was made closes the ticket.
    let ticket: Value = visitor
        .post(&archive(&token))
        .json(&json!({ "node_ids": [a, b] }))
        .await
        .json();
    let ticket = ticket["ticket"].as_str().unwrap().to_owned();
    alice
        .patch(&format!("/api/nodes/{a}"))
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({ "parent_id": files }))
        .await
        .assert_status_ok();
    visitor
        .get(&format!("{}/{ticket}", archive(&token)))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_item_has_a_bounded_number_of_links(pool: PgPool) {
    let instance = Instance::with_config(pool.clone(), config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let trip = folder(&alice, &alice_me, files, "Trip").await;
    sqlx::query(
        "INSERT INTO public_links (id, owner_id, node_id, token_hash) \
         SELECT gen_random_uuid(), $1, $2, sha256(i::text::bytea) \
           FROM generate_series(1, $3::int) i",
    )
    .bind(common::files::user_id(&alice_me))
    .bind(trip)
    .bind(i32::try_from(squirrel_db::public_links::MAX_LINKS).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    post(
        &alice,
        &alice_me,
        "/api/public-links",
        json!({ "node_id": trip }),
    )
    .await
    .assert_status(StatusCode::CONFLICT);
}

fn ticket_size(zip: &TestResponse) -> usize {
    zip.header("content-length")
        .to_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_public_listener_serves_public_links_only(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let token = make_link(&alice, &alice_me, json!({ "node_id": files })).await;
    let visitor = instance.public_browser();
    visitor
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status_ok();

    // The private API is not there, even with a valid session cookie.
    for path in [
        "/api/healthz",
        "/api/setup/status",
        "/api/auth/me",
        "/api/nodes",
        "/api/public-links",
        "/login",
        "/files",
        "/",
        "/index.html",
    ] {
        let response = visitor.get(path).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
    let session = alice.get("/api/auth/me").await;
    session.assert_status_ok();
    visitor
        .post("/api/auth/login")
        .json(&json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    // The tailnet listener answers public links too.
    instance
        .browser()
        .get(&format!("/api/public/{token}"))
        .await
        .assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn malformed_and_unknown_tokens_are_not_found(pool: PgPool) {
    let instance = Instance::with_config(pool, config());
    let visitor = instance.public_browser();
    let unknown = squirrel_auth::generate_token();
    for token in ["short", unknown.as_str(), &"!".repeat(43)] {
        let response = visitor.get(&format!("/api/public/{token}")).await;
        response.assert_status(StatusCode::NOT_FOUND);
        assert_eq!(code(&response), "not_found");
    }
}
