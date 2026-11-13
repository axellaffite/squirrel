use axum::body::Bytes;
use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use squirrel_db::nodes;
use uuid::Uuid;

use super::{CSRF, Instance, csrf};

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn user_id(me: &Value) -> Uuid {
    me["id"].as_str().expect("id").parse().expect("uuid")
}

/// One of the user's top-level folders, read from the database.
pub async fn root_id(instance: &Instance, me: &Value, name: &str) -> Uuid {
    nodes::list_children(&instance.state.pool, user_id(me), None, None, 10)
        .await
        .expect("roots")
        .items
        .into_iter()
        .find(|node| node.name == name)
        .expect("root exists")
        .id
}

pub async fn start_upload(
    browser: &TestServer,
    me: &Value,
    parent: Uuid,
    name: &str,
    content: &[u8],
) -> TestResponse {
    browser
        .post("/api/uploads")
        .add_header(CSRF, csrf(me))
        .json(&json!({
            "parent_id": parent,
            "name": name,
            "size": content.len(),
            "sha256": sha256_hex(content),
        }))
        .await
}

pub async fn put_chunk(
    browser: &TestServer,
    me: &Value,
    upload_id: &str,
    index: usize,
    bytes: &[u8],
) -> TestResponse {
    browser
        .put(&format!("/api/uploads/{upload_id}/chunks/{index}"))
        .add_header(CSRF, csrf(me))
        .bytes(Bytes::copy_from_slice(bytes))
        .await
}

pub async fn complete(browser: &TestServer, me: &Value, upload_id: &str) -> TestResponse {
    browser
        .post(&format!("/api/uploads/{upload_id}/complete"))
        .add_header(CSRF, csrf(me))
        .await
}

/// Runs the whole protocol (or takes the dedup shortcut) and returns the node.
pub async fn upload(
    browser: &TestServer,
    me: &Value,
    parent: Uuid,
    name: &str,
    content: &[u8],
) -> Value {
    let started = start_upload(browser, me, parent, name, content).await;
    if started.status_code() == StatusCode::OK {
        return started.json::<Value>()["node"].clone();
    }
    started.assert_status(StatusCode::CREATED);
    let session: Value = started.json();
    let id = session["upload_id"].as_str().expect("upload_id").to_owned();
    let chunk_size = usize::try_from(session["chunk_size"].as_u64().expect("chunk_size")).unwrap();
    for (index, chunk) in content.chunks(chunk_size).enumerate() {
        put_chunk(browser, me, &id, index, chunk)
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let done = complete(browser, me, &id).await;
    done.assert_status(StatusCode::CREATED);
    done.json()
}

/// Error code of an envelope response.
pub fn code(response: &TestResponse) -> String {
    response.json::<Value>()["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}
