//! Several files downloaded as one zip: a ticket, then the stream.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum_test::{TestResponse, TestServer};
use common::files::{code, root_id, upload, user_id};
use common::{CSRF, Instance, csrf, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, DownloadLimits};
use uuid::Uuid;

async fn create(browser: &TestServer, me: &Value, ids: Value) -> TestResponse {
    browser
        .post("/api/files/archive")
        .add_header(CSRF, csrf(me))
        .json(&json!({ "node_ids": ids }))
        .await
}

fn id_of(node: &Value) -> String {
    node["id"].as_str().unwrap().to_owned()
}

fn url(ticket: &Value) -> String {
    format!("/api/files/archive/{}", ticket["ticket"].as_str().unwrap())
}

fn header(response: &TestResponse, name: axum::http::HeaderName) -> String {
    response.header(name).to_str().unwrap().to_owned()
}

fn today() -> String {
    let date = time::OffsetDateTime::now_utc().date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

/// The exact zip size of files `(name, content)`.
fn zip_size(files: &[(&str, &[u8])]) -> u64 {
    let entries: u64 = files
        .iter()
        .map(|(name, data)| (30 + 46 + 2 * name.len() + data.len()) as u64)
        .sum();
    entries + 22
}

/// `(name, content)` of each entry, read from the local headers.
fn entries(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let le16 = |at: usize| usize::from(u16::from_le_bytes([zip[at], zip[at + 1]]));
    let le32 = |at: usize| u32::from_le_bytes(zip[at..at + 4].try_into().unwrap());
    let mut found = Vec::new();
    let mut at = 0;
    while le32(at) == 0x0403_4b50 {
        let crc = le32(at + 14);
        let size = le32(at + 22) as usize;
        let name_len = le16(at + 26);
        let name = String::from_utf8(zip[at + 30..at + 30 + name_len].to_vec()).unwrap();
        let start = at + 30 + name_len + le16(at + 28);
        let data = zip[start..start + size].to_vec();
        assert_eq!(crc, crc32fast::hash(&data), "CRC of {name}");
        found.push((name, data));
        at = start + size;
    }
    assert_eq!(
        u32::from_le_bytes(zip[zip.len() - 22..zip.len() - 18].try_into().unwrap()),
        0x0605_4b50,
        "ends with the end record"
    );
    found
}

/// Alice with `files` uploaded to her "Files" folder.
async fn alice_with(
    instance: &Instance,
    files: &[(&str, &[u8])],
) -> (TestServer, Value, Vec<String>) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = root_id(instance, &me, "Files").await;
    let mut ids = Vec::new();
    for (name, content) in files {
        ids.push(id_of(&upload(&browser, &me, parent, name, content).await));
    }
    (browser, me, ids)
}

const TWO: [(&str, &[u8]); 2] = [("alpha.txt", b"alpha"), ("bravo.txt", b"bravo bravo")];

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn two_files_stream_as_a_zip_of_the_announced_size(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;

    let created = create(&browser, &me, json!(ids)).await;
    created.assert_status(StatusCode::CREATED);
    let ticket: Value = created.json();
    assert_eq!(ticket["count"], 2);
    assert_eq!(ticket["size"].as_u64().unwrap(), zip_size(&TWO));
    let name = format!("Squirrel 2 files {}.zip", today());
    assert_eq!(ticket["name"], name.as_str());
    assert!(ticket["ticket"].as_str().unwrap().len() >= 43);

    let response = browser.get(&url(&ticket)).await;
    response.assert_status_ok();
    assert_eq!(header(&response, CONTENT_TYPE), "application/zip");
    assert_eq!(
        header(&response, CONTENT_DISPOSITION),
        format!(
            "attachment; filename=\"{name}\"; filename*=UTF-8''{}",
            name.replace(' ', "%20")
        )
    );
    assert_eq!(
        header(&response, CONTENT_LENGTH),
        zip_size(&TWO).to_string()
    );
    assert_eq!(header(&response, CACHE_CONTROL), "no-store");
    let body = response.as_bytes();
    assert_eq!(body.len() as u64, zip_size(&TWO));
    let found = entries(body);
    let expected: Vec<(String, Vec<u8>)> = TWO
        .iter()
        .map(|(name, data)| ((*name).to_owned(), data.to_vec()))
        .collect();
    assert_eq!(found, expected);

    // A ticket serves again until it expires (a resumed download).
    let again = browser.get(&url(&ticket)).await;
    again.assert_status_ok();
    assert_eq!(again.as_bytes(), body);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_device_bearer_may_create_and_fetch_a_ticket(pool: PgPool) {
    let instance = Instance::new(pool);
    let (_, _, ids) = alice_with(&instance, &TWO).await;
    let login = common::devices::device_login(&instance, "alice", "Pixel").await;
    let (name, value) = common::devices::bearer(&common::devices::access(&login));
    let phone = instance.browser();
    let created = phone
        .post("/api/files/archive")
        .add_header(name.clone(), value.clone())
        .json(&json!({ "node_ids": ids }))
        .await;
    created.assert_status(StatusCode::CREATED);
    let response = phone
        .get(&url(&created.json()))
        .add_header(name, value)
        .await;
    response.assert_status_ok();
    assert_eq!(entries(response.as_bytes()).len(), 2);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn duplicate_ids_collapse_and_names_are_disambiguated(pool: PgPool) {
    let instance = Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let files = root_id(&instance, &me, "Files").await;
    let created = browser
        .post("/api/folders")
        .add_header(CSRF, csrf(&me))
        .json(&json!({ "parent_id": files, "name": "Other" }))
        .await;
    let other: Uuid = created.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let first = id_of(&upload(&browser, &me, files, "Notes.txt", b"one").await);
    let second = id_of(&upload(&browser, &me, other, "notes.txt", b"two").await);

    let ticket: Value = create(&browser, &me, json!([first, second, first]))
        .await
        .json();
    assert_eq!(ticket["count"], 2);
    let zip = browser.get(&url(&ticket)).await;
    let found = entries(zip.as_bytes());
    assert_eq!(
        found,
        [
            ("Notes.txt".to_owned(), b"one".to_vec()),
            ("notes (2).txt".to_owned(), b"two".to_vec()),
        ]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn photos_only_are_named_photos(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    sqlx::query("UPDATE nodes SET mime = 'image/jpeg' WHERE id = $1")
        .bind(ids[0].parse::<Uuid>().unwrap())
        .execute(&instance.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE nodes SET mime = 'video/mp4' WHERE id = $1")
        .bind(ids[1].parse::<Uuid>().unwrap())
        .execute(&instance.state.pool)
        .await
        .unwrap();
    let ticket: Value = create(&browser, &me, json!(ids)).await.json();
    assert_eq!(ticket["name"], format!("Squirrel 2 photos {}.zip", today()));

    // One file that is not a photo: files.
    let parent = root_id(&instance, &me, "Files").await;
    let text = id_of(&upload(&browser, &me, parent, "c.txt", b"charlie").await);
    let mixed: Value = create(&browser, &me, json!([ids[0], text])).await.json();
    assert_eq!(mixed["name"], format!("Squirrel 2 files {}.zip", today()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ids_the_caller_cannot_download_are_404(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let parent = root_id(&instance, &alice_me, "Files").await;
    let secret = id_of(&upload(&alice, &alice_me, parent, "secret.txt", b"secret").await);
    let bobs_parent = root_id(&instance, &bob_me, "Files").await;
    let own = id_of(&upload(&bob, &bob_me, bobs_parent, "mine.txt", b"mine").await);

    let refused = create(&bob, &bob_me, json!([own, secret])).await;
    refused.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(code(&refused), "not_found");
    create(&bob, &bob_me, json!([Uuid::now_v7()]))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Trashed files are gone too.
    alice
        .delete(&format!("/api/nodes/{secret}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    create(&alice, &alice_me, json!([secret]))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_and_bad_lists_are_400(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    let folder = root_id(&instance, &me, "Files").await;

    let with_folder = create(&browser, &me, json!([ids[0], folder])).await;
    with_folder.assert_status(StatusCode::BAD_REQUEST);
    let message = with_folder.json::<Value>()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("folder"), "{message}");

    create(&browser, &me, json!([]))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    let too_many: Vec<Uuid> = (0..501).map(|_| Uuid::now_v7()).collect();
    create(&browser, &me, json!(too_many))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_ticket_belongs_to_its_creator(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, _)) = two_users(&instance).await;
    let parent = root_id(&instance, &alice_me, "Files").await;
    let id = id_of(&upload(&alice, &alice_me, parent, "a.txt", b"alpha").await);
    let ticket: Value = create(&alice, &alice_me, json!([id])).await.json();

    let stolen = bob.get(&url(&ticket)).await;
    stolen.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(code(&stolen), "not_found");
    alice
        .get("/api/files/archive/not-a-real-ticket")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    // Bob's attempt did not spend it.
    alice.get(&url(&ticket)).await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn files_shared_with_the_caller_are_allowed(pool: PgPool) {
    let instance = Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let created = alice
        .post("/api/folders")
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({ "parent_id": files, "name": "Trip" }))
        .await;
    let trip: Uuid = created.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let id = id_of(&upload(&alice, &alice_me, trip, "beach.txt", b"sand").await);
    create(&bob, &bob_me, json!([id]))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    alice
        .post("/api/shares")
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({ "node_id": trip, "username": "bob", "permission": "read" }))
        .await
        .assert_status(StatusCode::CREATED);

    let created = create(&bob, &bob_me, json!([id])).await;
    created.assert_status(StatusCode::CREATED);
    let ticket: Value = created.json();
    assert_eq!(ticket["name"], format!("Squirrel 1 file {}.zip", today()));
    let zip = bob.get(&url(&ticket)).await;
    zip.assert_status_ok();
    assert_eq!(
        entries(zip.as_bytes()),
        [("beach.txt".to_owned(), b"sand".to_vec())]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn archives_of_4_gib_or_more_are_refused(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    // What the database records decides; no need for real 2 GiB files.
    sqlx::query("UPDATE nodes SET size = 2147483648 WHERE id = ANY($1)")
        .bind(
            ids.iter()
                .map(|id| id.parse::<Uuid>().unwrap())
                .collect::<Vec<_>>(),
        )
        .execute(&instance.state.pool)
        .await
        .unwrap();
    let refused = create(&browser, &me, json!(ids)).await;
    refused.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(code(&refused), "archive_too_large");
    let message = refused.json::<Value>()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("select fewer files"), "{message}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_file_trashed_after_the_ticket_makes_it_404(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    let ticket: Value = create(&browser, &me, json!(ids)).await.json();
    browser
        .delete(&format!("/api/nodes/{}", ids[1]))
        .add_header(CSRF, csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    browser
        .get(&url(&ticket))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn streaming_holds_a_download_slot(pool: PgPool) {
    let config = ApiConfig {
        download_limits: DownloadLimits {
            concurrency: 4,
            per_user: 1,
            wait: Duration::from_millis(50),
        },
        ..common::test_config()
    };
    let instance = Instance::with_config(pool, config);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    let ticket: Value = create(&browser, &me, json!(ids)).await.json();

    let held = instance
        .state
        .downloads
        .acquire(user_id(&me))
        .await
        .expect("a free slot");
    let refused = browser.get(&url(&ticket)).await;
    refused.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code(&refused), "busy");
    drop(held);
    browser.get(&url(&ticket)).await.assert_status_ok();
    // The slot went back once the body was sent.
    browser.get(&url(&ticket)).await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_missing_blob_ends_the_stream_early(pool: PgPool) {
    let instance = Instance::new(pool);
    let (browser, me, ids) = alice_with(&instance, &TWO).await;
    let ticket: Value = create(&browser, &me, json!(ids)).await.json();
    let sha = common::files::sha256_hex(TWO[1].1);
    let blob = walk(instance.data_dir())
        .into_iter()
        .find(|path| path.file_name().is_some_and(|name| name == sha.as_str()))
        .expect("the blob on disk");
    std::fs::remove_file(blob).unwrap();

    let response = browser.get(&url(&ticket)).await;
    response.assert_status_ok();
    let body = response.as_bytes();
    assert!((body.len() as u64) < zip_size(&TWO), "cut short");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}
