#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Extension;
use axum::extract::ConnectInfo;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, AppState, SetupToken};
use squirrel_storage::BlobStore;
use tempfile::TempDir;

pub const PASSWORD: &str = "correct horse battery";
pub const CSRF: HeaderName = HeaderName::from_static("x-csrf-token");
pub const SETUP_TOKEN: &str = "test-setup-token";

/// Small chunks keep multi-chunk tests fast.
pub const TEST_CHUNK_SIZE: u64 = 1024;

pub fn test_config() -> ApiConfig {
    ApiConfig {
        cookie_secure: false,
        login_rate_limit: None,
        join_check_rate_limit: None,
        setup_token: Some(SetupToken::new(SETUP_TOKEN)),
        upload_chunk_size: TEST_CHUNK_SIZE,
        min_free_bytes: 0,
        ..ApiConfig::default()
    }
}

/// One server: database, data directory and state. Each [`Instance::browser`]
/// has its own cookie jar but shares the storage, like real browsers.
pub struct Instance {
    pub state: AppState,
    data: Arc<TempDir>,
}

impl Instance {
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, test_config())
    }

    pub fn with_config(pool: PgPool, config: ApiConfig) -> Self {
        let data = Arc::new(tempfile::tempdir().expect("temporary data dir"));
        let storage = BlobStore::open(data.path()).expect("blob store");
        Self {
            state: AppState::new(pool, storage, config),
            data,
        }
    }

    pub fn data_dir(&self) -> &Path {
        self.data.path()
    }

    /// A real `ConnectInfo` is inserted (not `MockConnectInfo`) because
    /// tower_governor's peer-IP key extractor only reads `ConnectInfo<SocketAddr>`.
    /// The router holds the data directory, so it lives as long as any browser does.
    pub fn browser(&self) -> TestServer {
        let app = squirrel_api::router(self.state.clone())
            .layer(Extension(self.data.clone()))
            .layer(Extension(ConnectInfo(SocketAddr::from((
                [127, 0, 0, 1],
                40_000,
            )))));
        TestServer::builder()
            .save_cookies()
            .build(app)
            .expect("test server builds")
    }
}

pub fn server(pool: PgPool) -> TestServer {
    Instance::new(pool).browser()
}

pub fn server_with(pool: PgPool, config: ApiConfig) -> TestServer {
    Instance::with_config(pool, config).browser()
}

/// Alice (admin) and Bob (invited by her), each in their own browser.
pub async fn two_users(instance: &Instance) -> ((TestServer, Value), (TestServer, Value)) {
    let alice = instance.browser();
    let alice_me = setup_admin(&alice, "alice").await;
    let created: Value = alice
        .post("/api/invitations")
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({}))
        .await
        .json();
    let token = created["token"].as_str().expect("token");
    let bob = instance.browser();
    let joined = bob
        .post(&format!("/api/join/{token}"))
        .json(&account("bob"))
        .await;
    joined.assert_status(StatusCode::CREATED);
    let bob_me = joined.json();
    ((alice, alice_me), (bob, bob_me))
}

pub fn account(username: &str) -> Value {
    json!({ "username": username, "display_name": username, "password": PASSWORD })
}

pub fn setup_request(username: &str) -> Value {
    json!({
        "setup_token": SETUP_TOKEN,
        "username": username,
        "display_name": username,
        "password": PASSWORD,
    })
}

pub fn csrf(me: &Value) -> HeaderValue {
    HeaderValue::from_str(me["csrf_token"].as_str().expect("csrf_token present")).unwrap()
}

pub async fn setup_admin(server: &TestServer, username: &str) -> Value {
    let response = server
        .post("/api/setup/admin")
        .json(&setup_request(username))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}
