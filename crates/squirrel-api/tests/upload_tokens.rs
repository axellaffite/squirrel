mod common;

use axum::body::Bytes;
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderValue, StatusCode};
use axum_test::TestResponse;
use common::devices::{access, bearer, device_id, device_login};
use common::files::{root_id, sha256_hex};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const CONTENT: &[u8] = &[7; 2000];

struct Setup {
    instance: common::Instance,
    phone: Value,
    files: Uuid,
}

async fn setup(pool: &PgPool) -> Setup {
    let instance = common::Instance::new(pool.clone());
    let me = common::setup_admin(&instance.browser(), "alice").await;
    let phone = device_login(&instance, "alice", "Pixel").await;
    let files = root_id(&instance, &me, "Files").await;
    Setup {
        instance,
        phone,
        files,
    }
}

impl Setup {
    /// `POST /api/uploads` as the phone; returns the session body.
    async fn start(&self, name: &str, content: &[u8]) -> Value {
        let (header, value) = bearer(&access(&self.phone));
        let response = self
            .instance
            .browser()
            .post("/api/uploads")
            .add_header(header, value)
            .json(&json!({
                "parent_id": self.files, "name": name, "size": content.len(),
                "sha256": sha256_hex(content),
            }))
            .await;
        response.assert_status(StatusCode::CREATED);
        response.json()
    }

    async fn chunk(&self, auth: HeaderValue, upload: &str, index: usize) -> TestResponse {
        let start = index * 1024;
        let end = CONTENT.len().min(start + 1024);
        self.instance
            .browser()
            .put(&format!("/api/uploads/{upload}/chunks/{index}"))
            .add_header(AUTHORIZATION, auth)
            .bytes(Bytes::copy_from_slice(&CONTENT[start..end]))
            .await
    }

    async fn status(&self, auth: HeaderValue, upload: &str) -> TestResponse {
        self.instance
            .browser()
            .get(&format!("/api/uploads/{upload}"))
            .add_header(AUTHORIZATION, auth)
            .await
    }

    fn bearer(&self) -> HeaderValue {
        bearer(&access(&self.phone)).1
    }
}

fn token(session: &Value) -> String {
    session["upload_token"]
        .as_str()
        .expect("upload_token")
        .to_owned()
}

fn upload_auth(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("Upload {token}")).unwrap()
}

fn id(session: &Value) -> String {
    session["upload_id"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn devices_get_a_token_browsers_do_not(pool: PgPool) {
    let s = setup(&pool).await;
    let session = s.start("a.bin", CONTENT).await;
    assert!(token(&session).starts_with("squ_"));

    let browser = s.instance.browser();
    let me = common::login(&browser, "alice").await;
    let by_web = common::files::start_upload(&browser, &me, s.files, "b.bin", b"web").await;
    by_web.assert_status(StatusCode::CREATED);
    assert!(by_web.json::<Value>().get("upload_token").is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_token_sends_chunks_and_reads_status_after_the_access_token_expired(pool: PgPool) {
    let s = setup(&pool).await;
    let session = s.start("a.bin", CONTENT).await;
    let (upload, auth) = (id(&session), upload_auth(&token(&session)));
    sqlx::query("UPDATE device_access_tokens SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    for index in 0..2 {
        s.chunk(auth.clone(), &upload, index)
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let status = s.status(auth.clone(), &upload).await;
    status.assert_status_ok();
    assert_eq!(status.json::<Value>()["received_chunks"], json!([0, 1]));
    assert!(status.json::<Value>().get("upload_token").is_none());

    // Completion needs the device itself: never the upload token.
    let complete = s
        .instance
        .browser()
        .post(&format!("/api/uploads/{upload}/complete"))
        .add_header(AUTHORIZATION, auth)
        .await;
    complete.assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_token_opens_only_its_own_upload(pool: PgPool) {
    let s = setup(&pool).await;
    let first = s.start("a.bin", CONTENT).await;
    let second = s.start("b.bin", b"other content").await;
    let auth = upload_auth(&token(&first));
    s.status(auth.clone(), &id(&second))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    s.chunk(auth.clone(), &id(&second), 0)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    for (method, path) in [
        ("GET", "/api/nodes".to_owned()),
        ("DELETE", format!("/api/uploads/{}", id(&first))),
        ("POST", "/api/sync/check".to_owned()),
        ("GET", "/api/devices/current".to_owned()),
    ] {
        let request = s
            .instance
            .browser()
            .method(method.parse().unwrap(), &path)
            .add_header(AUTHORIZATION, auth.clone());
        request.await.assert_status(StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resuming_issues_a_new_token_and_keeps_the_previous_one(pool: PgPool) {
    let s = setup(&pool).await;
    let first = s.start("a.bin", CONTENT).await;
    let resumed = s.start("a.bin", CONTENT).await;
    assert_eq!(id(&first), id(&resumed));
    assert_ne!(token(&first), token(&resumed));
    // Chunks queued with the first token before the resume still go through.
    s.chunk(upload_auth(&token(&first)), &id(&first), 0)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    s.status(upload_auth(&token(&first)), &id(&first))
        .await
        .assert_status_ok();
    s.status(upload_auth(&token(&resumed)), &id(&first))
        .await
        .assert_status_ok();

    // One more rotation: the oldest token dies, the two latest work.
    let third = s.start("a.bin", CONTENT).await;
    s.status(upload_auth(&token(&first)), &id(&first))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    s.chunk(upload_auth(&token(&first)), &id(&first), 1)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    for live in [&resumed, &third] {
        s.status(upload_auth(&token(live)), &id(&first))
            .await
            .assert_status_ok();
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn both_live_tokens_die_on_completion_and_on_revocation(pool: PgPool) {
    let s = setup(&pool).await;
    let done = s.start("done.bin", CONTENT).await;
    let done_again = s.start("done.bin", CONTENT).await;
    for index in 0..2 {
        s.chunk(upload_auth(&token(&done_again)), &id(&done), index)
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    s.instance
        .browser()
        .post(&format!("/api/uploads/{}/complete", id(&done)))
        .add_header(AUTHORIZATION, s.bearer())
        .await
        .assert_status(StatusCode::CREATED);
    for dead in [&done, &done_again] {
        s.status(upload_auth(&token(dead)), &id(&done))
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }

    let open = s.start("open.bin", b"still open").await;
    let open_again = s.start("open.bin", b"still open").await;
    for live in [&open, &open_again] {
        s.status(upload_auth(&token(live)), &id(&open))
            .await
            .assert_status_ok();
    }
    let browser = s.instance.browser();
    let me = common::login(&browser, "alice").await;
    browser
        .delete(&format!("/api/devices/{}", device_id(&s.phone)))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for dead in [&open, &open_again] {
        s.status(upload_auth(&token(dead)), &id(&open))
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_token_dies_with_its_session_or_its_device(pool: PgPool) {
    let s = setup(&pool).await;
    // Completed.
    let done = s.start("done.bin", CONTENT).await;
    let done_auth = upload_auth(&token(&done));
    for index in 0..2 {
        s.chunk(done_auth.clone(), &id(&done), index).await;
    }
    s.instance
        .browser()
        .post(&format!("/api/uploads/{}/complete", id(&done)))
        .add_header(AUTHORIZATION, s.bearer())
        .await
        .assert_status(StatusCode::CREATED);
    s.status(done_auth, &id(&done))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Aborted.
    let aborted = s.start("aborted.bin", b"aborted").await;
    s.instance
        .browser()
        .delete(&format!("/api/uploads/{}", id(&aborted)))
        .add_header(AUTHORIZATION, s.bearer())
        .await
        .assert_status(StatusCode::NO_CONTENT);
    s.status(upload_auth(&token(&aborted)), &id(&aborted))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Expired.
    let expired = s.start("expired.bin", b"expired").await;
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(id(&expired).parse::<Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    s.status(upload_auth(&token(&expired)), &id(&expired))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // The device revoked from the web.
    let open = s.start("open.bin", b"still open").await;
    let auth = upload_auth(&token(&open));
    s.status(auth.clone(), &id(&open)).await.assert_status_ok();
    let browser = s.instance.browser();
    let me = common::login(&browser, "alice").await;
    browser
        .delete(&format!("/api/devices/{}", device_id(&s.phone)))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    s.status(auth.clone(), &id(&open))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    s.chunk(auth, &id(&open), 0)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_and_bearers_still_work_on_those_routes(pool: PgPool) {
    let s = setup(&pool).await;
    let session = s.start("a.bin", CONTENT).await;
    let upload = id(&session);
    s.chunk(s.bearer(), &upload, 0)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    s.status(s.bearer(), &upload).await.assert_status_ok();

    let browser = s.instance.browser();
    let me = common::login(&browser, "alice").await;
    browser
        .get(&format!("/api/uploads/{upload}"))
        .await
        .assert_status_ok();
    browser
        .put(&format!("/api/uploads/{upload}/chunks/1"))
        .add_header(common::CSRF, common::csrf(&me))
        .bytes(Bytes::copy_from_slice(&CONTENT[1024..]))
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_hash_is_stored_and_a_near_miss_fails(pool: PgPool) {
    let s = setup(&pool).await;
    let session = s.start("a.bin", CONTENT).await;
    let token = token(&session);
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT upload_token_hash FROM upload_sessions WHERE id = $1")
            .bind(id(&session).parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, squirrel_auth::hash_token(&token));
    assert!(!String::from_utf8_lossy(&stored).contains(&token[4..]));

    let mut near = token.clone().into_bytes();
    let last = near.len() - 1;
    near[last] = if near[last] == b'A' { b'B' } else { b'A' };
    let near = String::from_utf8(near).unwrap();
    for bad in [near.as_str(), "squ_short", "garbage", ""] {
        s.status(upload_auth(bad), &id(&session))
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
}
