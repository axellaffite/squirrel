//! Live co-editing of text files over the WebSocket, with a `yrs` document
//! standing in for each browser's Yjs document.

mod common;

use std::time::Duration;

use axum::http::header::ORIGIN;
use axum::http::{HeaderValue, StatusCode};
use axum_test::{TestServer, TestWebSocket, WsMessage};
use common::files::{code, root_id, upload, user_id};
use common::{CSRF, Instance, csrf, login, test_config, two_users};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::ApiConfig;
use squirrel_api::collab::CollabConfig;
use uuid::Uuid;
use yrs::encoding::read::Cursor;
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, ReadTxn, Text, Transact, Update};

const WAIT: Duration = Duration::from_secs(5);

fn config() -> ApiConfig {
    ApiConfig {
        collab: CollabConfig {
            save_quiet: Duration::from_millis(50),
            save_max_delay: Duration::from_millis(200),
            recheck_every: Duration::from_millis(100),
            save_retry: Duration::from_millis(50),
            ..CollabConfig::default()
        },
        ..test_config()
    }
}

/// One browser tab with the editor open.
struct Client {
    ws: TestWebSocket,
    doc: Doc,
}

impl Client {
    async fn connect(browser: &TestServer, file: Uuid, epoch: &str) -> Self {
        let ws = socket(browser, file, epoch).await.into_websocket().await;
        let mut client = Self {
            ws,
            doc: Doc::new(),
        };
        // Like y-websocket: announce our state; the server's greeting and its
        // answer to our step 1 bring the document.
        let step1 = Message::Sync(SyncMessage::SyncStep1(client.doc.transact().state_vector()));
        client.send(step1.encode_v1()).await;
        client
    }

    async fn send(&mut self, bytes: Vec<u8>) {
        self.ws.send_message(WsMessage::Binary(bytes.into())).await;
    }

    /// Handles one message from the server. `Err`: the close code.
    async fn step(&mut self) -> Result<(), u16> {
        let message = tokio::time::timeout(WAIT, self.ws.receive_message())
            .await
            .expect("a message from the server");
        let data = match message {
            WsMessage::Binary(data) => data,
            WsMessage::Close(frame) => return Err(frame.map_or(1005, |frame| frame.code.into())),
            _ => return Ok(()),
        };
        let mut decoder = DecoderV1::new(Cursor::new(&data));
        let messages: Vec<Message> = MessageReader::new(&mut decoder)
            .collect::<Result<_, _>>()
            .unwrap();
        for message in messages {
            match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let update = self.doc.transact().encode_state_as_update_v1(&sv);
                    self.send(Message::Sync(SyncMessage::SyncStep2(update)).encode_v1())
                        .await;
                }
                Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                    let update = Update::decode_v1(&update).unwrap();
                    self.doc.transact_mut().apply_update(update).unwrap();
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn text(&self) -> String {
        let txn = self.doc.transact();
        txn.get_text("content")
            .map(|text| text.get_string(&txn))
            .unwrap_or_default()
    }

    async fn wait_for_text(&mut self, expected: &str) {
        while self.text() != expected {
            self.step().await.expect("still connected");
        }
    }

    async fn wait_for_close(&mut self) -> u16 {
        loop {
            if let Err(code) = self.step().await {
                return code;
            }
        }
    }

    async fn insert(&mut self, index: u32, text: &str) {
        let shared = self.doc.get_or_insert_text("content");
        let before = self.doc.transact().state_vector();
        shared.insert(&mut self.doc.transact_mut(), index, text);
        let update = self.doc.transact().encode_state_as_update_v1(&before);
        self.send(Message::Sync(SyncMessage::Update(update)).encode_v1())
            .await;
    }
}

fn origin(browser: &TestServer) -> HeaderValue {
    let address = browser.server_address().expect("http transport");
    let origin = format!("http://{}", address.authority());
    HeaderValue::from_str(&origin).unwrap()
}

fn socket(browser: &TestServer, file: Uuid, epoch: &str) -> axum_test::TestRequest {
    browser
        .get_websocket(&format!("/api/files/{file}/collab/ws?epoch={epoch}"))
        .add_header(ORIGIN, origin(browser))
}

async fn open(browser: &TestServer, me: &Value, file: Uuid) -> axum_test::TestResponse {
    browser
        .post(&format!("/api/files/{file}/collab"))
        .add_header(CSRF, csrf(me))
        .await
}

async fn epoch(browser: &TestServer, me: &Value, file: Uuid) -> String {
    let opened = open(browser, me, file).await;
    opened.assert_status_ok();
    opened.json::<Value>()["epoch"].as_str().unwrap().to_owned()
}

async fn share(browser: &TestServer, me: &Value, file: Uuid, permission: &str) -> Value {
    let created = browser
        .post("/api/shares")
        .add_header(CSRF, csrf(me))
        .json(&json!({ "node_id": file, "username": "bob", "permission": permission }))
        .await;
    created.assert_status(StatusCode::CREATED);
    created.json()
}

async fn content(browser: &TestServer, file: Uuid) -> String {
    let response = browser.get(&format!("/api/files/{file}/content")).await;
    response.assert_status_ok();
    response.text()
}

/// Waits until the file's content is `expected` (saves are asynchronous).
async fn wait_for_content(browser: &TestServer, file: Uuid, expected: &str) {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        if content(browser, file).await == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the file was never saved as {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

struct Scene {
    instance: Instance,
    alice: (TestServer, Value),
    bob: (TestServer, Value),
    file: Uuid,
}

/// Alice owns `notes.txt` ("hello"); both are signed in over real sockets.
async fn scene(pool: PgPool) -> Scene {
    let instance = Instance::with_config(pool, config());
    let ((alice, alice_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &alice_me, "Files").await;
    let node = upload(&alice, &alice_me, files, "notes.txt", b"hello").await;
    let file = node["id"].as_str().unwrap().parse().unwrap();
    let (alice_http, bob_http) = (instance.http_browser(), instance.http_browser());
    let alice_me = login(&alice_http, "alice").await;
    let bob_me = login(&bob_http, "bob").await;
    Scene {
        instance,
        alice: (alice_http, alice_me),
        bob: (bob_http, bob_me),
        file,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_owner_and_a_writer_co_edit_and_the_file_is_saved(pool: PgPool) {
    let Scene {
        instance,
        alice: (alice, alice_me),
        bob: (bob, bob_me),
        file,
    } = scene(pool).await;
    share(&alice, &alice_me, file, "write").await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let opened: Value = open(&bob, &bob_me, file).await.json();
    assert_eq!(opened["epoch"], json!(epoch));
    assert_eq!(opened["can_write"], json!(true));

    let mut a = Client::connect(&alice, file, &epoch).await;
    let mut b = Client::connect(&bob, file, &epoch).await;
    a.wait_for_text("hello").await;
    b.wait_for_text("hello").await;

    a.insert(5, " world").await;
    b.wait_for_text("hello world").await;
    b.insert(0, "Oh, ").await;
    a.wait_for_text("Oh, hello world").await;

    wait_for_content(&alice, file, "Oh, hello world").await;
    let node: Value = alice.get(&format!("/api/nodes/{file}")).await.json();
    assert_eq!(node["node"]["size"], json!(15));

    // Everyone leaves: the room is dropped, and reopening merges into the same
    // document (same epoch, no duplicated text).
    drop((a, b));
    let deadline = tokio::time::Instant::now() + WAIT;
    while !instance.state.collab.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the room was never dropped"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(self::epoch(&alice, &alice_me, file).await, epoch);
    let mut again = Client::connect(&alice, file, &epoch).await;
    again.wait_for_text("Oh, hello world").await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_reader_sees_edits_but_cannot_change_the_file(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        bob: (bob, bob_me),
        file,
        ..
    } = scene(pool).await;
    share(&alice, &alice_me, file, "read").await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let opened: Value = open(&bob, &bob_me, file).await.json();
    assert_eq!(opened["can_write"], json!(false));

    let mut a = Client::connect(&alice, file, &epoch).await;
    let mut b = Client::connect(&bob, file, &epoch).await;
    a.wait_for_text("hello").await;
    b.wait_for_text("hello").await;

    b.insert(0, "hacked ").await;
    a.insert(5, "!").await;
    b.wait_for_text("hacked hello!").await;
    a.wait_for_text("hello!").await;
    wait_for_content(&alice, file, "hello!").await;
    // Bob's change never reached Alice.
    assert_eq!(a.text(), "hello!");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoking_the_share_closes_the_connection(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        bob: (bob, _),
        file,
        ..
    } = scene(pool).await;
    let shared = share(&alice, &alice_me, file, "write").await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let mut b = Client::connect(&bob, file, &epoch).await;
    b.wait_for_text("hello").await;

    alice
        .delete(&format!("/api/shares/{}", shared["id"].as_str().unwrap()))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(b.wait_for_close().await, 4404);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn strangers_stale_epochs_and_other_sites_are_refused(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        bob: (bob, bob_me),
        file,
        ..
    } = scene(pool).await;
    let epoch = epoch(&alice, &alice_me, file).await;

    // Bob has no share: the file does not exist for him.
    let refused = open(&bob, &bob_me, file).await;
    refused.assert_status(StatusCode::NOT_FOUND);
    let mut stranger = Client::connect(&bob, file, &epoch).await;
    assert_eq!(stranger.wait_for_close().await, 4404);

    let mut stale = Client::connect(&alice, file, &Uuid::now_v7().to_string()).await;
    assert_eq!(stale.wait_for_close().await, 4409);

    alice
        .get_websocket(&format!("/api/files/{file}/collab/ws?epoch={epoch}"))
        .add_header(ORIGIN, HeaderValue::from_static("https://evil.example"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    alice
        .get_websocket(&format!("/api/files/{file}/collab/ws?epoch={epoch}"))
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_utf8_text_files_open(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        file,
        ..
    } = scene(pool).await;
    let me: Value = alice.get("/api/auth/me").await.json();
    assert_eq!(user_id(&me), user_id(&alice_me));
    let parent: Value = alice.get(&format!("/api/nodes/{file}")).await.json();
    let parent: Uuid = parent["node"]["parent_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let binary = upload(&alice, &alice_me, parent, "latin1.txt", b"caf\xe9").await;
    let binary: Uuid = binary["id"].as_str().unwrap().parse().unwrap();
    let refused = open(&alice, &alice_me, binary).await;
    refused.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code(&refused), "not_editable");

    let pdf = upload(&alice, &alice_me, parent, "doc.pdf", b"%PDF-1.7\n").await;
    let pdf: Uuid = pdf["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(code(&open(&alice, &alice_me, pdf).await), "not_editable");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashing_the_file_closes_the_room(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        file,
        ..
    } = scene(pool).await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let mut a = Client::connect(&alice, file, &epoch).await;
    a.wait_for_text("hello").await;
    alice
        .delete(&format!("/api/nodes/{file}"))
        .add_header(CSRF, csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(a.wait_for_close().await, 4404);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shutdown_saves_and_disconnects_everyone(pool: PgPool) {
    let instance = Instance::with_config(
        pool,
        ApiConfig {
            collab: CollabConfig {
                // Far beyond the test: only the shutdown can save.
                save_quiet: Duration::from_secs(600),
                save_max_delay: Duration::from_secs(600),
                ..CollabConfig::default()
            },
            ..test_config()
        },
    );
    let ((setup, setup_me), _) = two_users(&instance).await;
    let files = root_id(&instance, &setup_me, "Files").await;
    let node = upload(&setup, &setup_me, files, "notes.txt", b"hello").await;
    let file: Uuid = node["id"].as_str().unwrap().parse().unwrap();
    let alice = instance.http_browser();
    let alice_me = login(&alice, "alice").await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let mut a = Client::connect(&alice, file, &epoch).await;
    a.wait_for_text("hello").await;
    a.insert(5, "!").await;
    // The edit reached the room once a round trip came back.
    let probe = Message::Sync(SyncMessage::SyncStep1(a.doc.transact().state_vector()));
    a.send(probe.encode_v1()).await;
    a.step().await.unwrap();

    squirrel_api::collab::shutdown(&instance.state).await;
    assert_eq!(a.wait_for_close().await, 1012);
    assert_eq!(content(&alice, file).await, "hello!");
    assert!(instance.state.collab.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_wrong_epoch_leaves_no_room_behind(pool: PgPool) {
    let Scene {
        instance,
        alice: (alice, alice_me),
        file,
        ..
    } = scene(pool).await;
    let epoch = epoch(&alice, &alice_me, file).await;
    let mut stale = Client::connect(&alice, file, &Uuid::now_v7().to_string()).await;
    assert_eq!(stale.wait_for_close().await, 4409);
    assert!(instance.state.collab.is_empty());
    let mut fine = Client::connect(&alice, file, &epoch).await;
    fine.wait_for_text("hello").await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_file_replaced_outside_the_editor_reopens_everyone(pool: PgPool) {
    let Scene {
        instance,
        alice: (alice, alice_me),
        file,
        ..
    } = scene(pool).await;
    let old_epoch = epoch(&alice, &alice_me, file).await;
    let mut idle = Client::connect(&alice, file, &old_epoch).await;
    idle.wait_for_text("hello").await;

    // Another version of the file takes its place (as a future "replace" would).
    let files: Value = alice.get(&format!("/api/nodes/{file}")).await.json();
    let parent: Uuid = files["node"]["parent_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let other = upload(&alice, &alice_me, parent, "v2.txt", b"second version").await;
    sqlx::query(
        "UPDATE nodes SET blob_sha256 = (SELECT blob_sha256 FROM nodes WHERE id = $2) WHERE id = $1",
    )
    .bind(file)
    .bind(other["id"].as_str().unwrap().parse::<Uuid>().unwrap())
    .execute(&instance.state.pool)
    .await
    .unwrap();

    let new_epoch = epoch(&alice, &alice_me, file).await;
    assert_ne!(new_epoch, old_epoch);
    let mut fresh = Client::connect(&alice, file, &new_epoch).await;
    fresh.wait_for_text("second version").await;
    assert_eq!(idle.wait_for_close().await, 4409);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_full_quota_closes_the_room_with_4507(pool: PgPool) {
    let Scene {
        instance,
        alice: (alice, alice_me),
        file,
        ..
    } = scene(pool).await;
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::bigint FROM nodes n JOIN users u ON u.id = n.owner_id \
         WHERE u.username = 'alice' AND n.kind = 'file' AND n.deleted_at IS NULL",
    )
    .fetch_one(&instance.state.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET quota_bytes = $1 WHERE username = 'alice'")
        .bind(used + 2)
        .execute(&instance.state.pool)
        .await
        .unwrap();
    let epoch = epoch(&alice, &alice_me, file).await;
    let mut a = Client::connect(&alice, file, &epoch).await;
    a.wait_for_text("hello").await;
    a.insert(5, " world, too long").await;
    assert_eq!(a.wait_for_close().await, 4507);
    assert_eq!(content(&alice, file).await, "hello");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_downgraded_writer_is_sent_to_reopen(pool: PgPool) {
    let Scene {
        alice: (alice, alice_me),
        bob: (bob, bob_me),
        file,
        ..
    } = scene(pool).await;
    share(&alice, &alice_me, file, "write").await;
    let epoch = epoch(&bob, &bob_me, file).await;
    let mut b = Client::connect(&bob, file, &epoch).await;
    b.wait_for_text("hello").await;
    share(&alice, &alice_me, file, "read").await;
    assert_eq!(b.wait_for_close().await, 4409);
    let reopened: Value = open(&bob, &bob_me, file).await.json();
    assert_eq!(reopened["can_write"], json!(false));
}
