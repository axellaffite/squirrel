mod common;

use axum::body::Bytes;
use axum::http::StatusCode;
use axum_test::TestResponse;
use common::devices::{access, bearer, device_id, device_login};
use common::files::{code, root_id, sha256_hex};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::PrimitiveDateTime;
use time::macros::datetime;
use uuid::Uuid;

struct Phone<'a> {
    instance: &'a common::Instance,
    token: String,
}

impl Phone<'_> {
    async fn start(&self, body: Value) -> TestResponse {
        let (name, value) = bearer(&self.token);
        self.instance
            .browser()
            .post("/api/uploads")
            .add_header(name, value)
            .json(&body)
            .await
    }

    /// Sends every chunk and completes; returns the node.
    async fn finish(&self, session: &Value, content: &[u8]) -> Value {
        let id = session["upload_id"].as_str().unwrap();
        let (name, value) = bearer(&self.token);
        let browser = self.instance.browser();
        browser
            .put(&format!("/api/uploads/{id}/chunks/0"))
            .add_header(name.clone(), value.clone())
            .bytes(Bytes::copy_from_slice(content))
            .await
            .assert_status(StatusCode::NO_CONTENT);
        let done = browser
            .post(&format!("/api/uploads/{id}/complete"))
            .add_header(name, value)
            .await;
        done.assert_status(StatusCode::CREATED);
        done.json()
    }
}

fn request(parent: Uuid, name: &str, content: &[u8], extra: Value) -> Value {
    let mut body = json!({
        "parent_id": parent,
        "name": name,
        "size": content.len(),
        "sha256": sha256_hex(content),
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    body
}

async fn client_taken(pool: &PgPool, node: &Value) -> (Option<PrimitiveDateTime>, Option<i16>) {
    let id: Uuid = node["id"].as_str().unwrap().parse().unwrap();
    sqlx::query_as("SELECT client_taken_at, client_taken_offset_min FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn recorded(pool: &PgPool, device: &str) -> Vec<(String, Option<Uuid>)> {
    sqlx::query_as(
        "SELECT device_asset_id, node_id FROM device_assets WHERE device_id = $1 \
         ORDER BY device_asset_id",
    )
    .bind(device.parse::<Uuid>().unwrap())
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn setup(pool: &PgPool) -> (common::Instance, Value) {
    let instance = common::Instance::new(pool.clone());
    let me = common::setup_admin(&instance.browser(), "alice").await;
    (instance, me)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_phone_upload_carries_its_capture_time_and_asset_id(pool: PgPool) {
    let (instance, me) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let phone = Phone {
        instance: &instance,
        token: access(&login),
    };
    let photos = root_id(&instance, &me, "Photos").await;
    let extra = json!({ "taken_at": "2024-07-14T18:30:05+02:00", "device_asset_id": "images:12" });
    let started = phone
        .start(request(photos, "IMG_12.jpg", b"phone bytes", extra))
        .await;
    started.assert_status(StatusCode::CREATED);
    let node = phone.finish(&started.json(), b"phone bytes").await;

    assert_eq!(
        client_taken(&pool, &node).await,
        (Some(datetime!(2024-07-14 18:30:05)), Some(120))
    );
    let node_id: Uuid = node["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        recorded(&pool, &device_id(&login)).await,
        [("images:12".to_owned(), Some(node_id))]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_deduplicated_phone_upload_is_recorded_too(pool: PgPool) {
    let (instance, me) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let phone = Phone {
        instance: &instance,
        token: access(&login),
    };
    let photos = root_id(&instance, &me, "Photos").await;
    let first = phone
        .start(request(
            photos,
            "a.jpg",
            b"same",
            json!({ "taken_at": "2024-07-14T18:30:05Z", "device_asset_id": "images:1" }),
        ))
        .await;
    phone.finish(&first.json(), b"same").await;

    let again = phone
        .start(request(
            photos,
            "b.jpg",
            b"same",
            json!({ "taken_at": "2023-01-02T03:04:05-05:30", "device_asset_id": "images:2" }),
        ))
        .await;
    again.assert_status_ok();
    let node = again.json::<Value>()["node"].clone();
    assert_eq!(
        client_taken(&pool, &node).await,
        (Some(datetime!(2023-01-02 03:04:05)), Some(-330))
    );
    let ids: Vec<String> = recorded(&pool, &device_id(&login))
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, ["images:1", "images:2"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_browser_may_send_a_capture_time_but_no_asset_id(pool: PgPool) {
    let (instance, _) = setup(&pool).await;
    let browser = instance.browser();
    let me = common::login(&browser, "alice").await;
    let files = root_id(&instance, &me, "Files").await;
    let with_id = browser
        .post("/api/uploads")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&request(
            files,
            "a.jpg",
            b"x",
            json!({ "device_asset_id": "images:1" }),
        ))
        .await;
    with_id.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code(&with_id), "validation_error");

    let with_time = browser
        .post("/api/uploads")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&request(
            files,
            "a.jpg",
            b"x",
            json!({ "taken_at": "2024-07-14T18:30:05+02:00" }),
        ))
        .await;
    with_time.assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn malformed_capture_times_are_refused(pool: PgPool) {
    let (instance, me) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let phone = Phone {
        instance: &instance,
        token: access(&login),
    };
    let photos = root_id(&instance, &me, "Photos").await;
    for taken_at in [
        "2024-07-14T18:30:05",
        "2024-07-14T18:30:05+19:00",
        "yesterday",
        "2024-07-14",
    ] {
        let response = phone
            .start(request(
                photos,
                "a.jpg",
                b"x",
                json!({ "taken_at": taken_at }),
            ))
            .await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{taken_at}");
    }
    let bad_id = phone
        .start(request(
            photos,
            "a.jpg",
            b"x",
            json!({ "device_asset_id": "" }),
        ))
        .await;
    bad_id.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resuming_keeps_the_first_capture_time(pool: PgPool) {
    let (instance, me) = setup(&pool).await;
    let login = device_login(&instance, "alice", "Pixel").await;
    let phone = Phone {
        instance: &instance,
        token: access(&login),
    };
    let photos = root_id(&instance, &me, "Photos").await;
    let first = phone
        .start(request(
            photos,
            "a.jpg",
            b"resumed",
            json!({ "taken_at": "2024-07-14T18:30:05+02:00", "device_asset_id": "images:1" }),
        ))
        .await;
    let second = phone
        .start(request(
            photos,
            "a.jpg",
            b"resumed",
            json!({ "taken_at": "2020-01-01T00:00:00Z", "device_asset_id": "images:1" }),
        ))
        .await;
    second.assert_status(StatusCode::CREATED);
    assert_eq!(
        first.json::<Value>()["upload_id"],
        second.json::<Value>()["upload_id"]
    );
    let node = phone.finish(&second.json(), b"resumed").await;
    assert_eq!(
        client_taken(&pool, &node).await,
        (Some(datetime!(2024-07-14 18:30:05)), Some(120))
    );
}
