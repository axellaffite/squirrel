//! Which callers every route accepts, checked against both the running router
//! and the committed `openapi.json` (the phone's client is generated from it).
//! A route missing from these lists fails: a new route must choose.

mod common;

use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum_test::TestServer;
use common::devices::{access, bearer, device_login};
use serde_json::{Value, json};
use sqlx::PgPool;

/// No credentials needed.
const PUBLIC: &[&str] = &[
    "/api/healthz",
    "/api/setup/status",
    "/api/setup/admin",
    "/api/auth/login",
    "/api/join/{token}",
    "/api/auth/device/login",
    "/api/auth/device/refresh",
];

/// Browser session only: account, invitations, administration, device
/// management, and deleting files (a stolen phone token must not be able to
/// destroy the library). `METHOD /path` entries override a path entry.
const SESSION: &[&str] = &[
    "DELETE /api/nodes/{id}",
    "DELETE /api/trash",
    "DELETE /api/trash/{id}",
    "/api/auth/me",
    "/api/auth/logout",
    "/api/auth/logout-all",
    "/api/invitations",
    "/api/invitations/{id}",
    "/api/admin/settings",
    "/api/admin/jobs",
    "/api/admin/jobs/{id}/retry",
    "/api/devices",
    "/api/devices/{id}",
];

/// Device bearer only.
const DEVICE: &[&str] = &[
    "/api/devices/current",
    "/api/auth/device/logout",
    "/api/sync/check",
    "/api/sync/folder",
];

/// Data routes: a session or a device bearer.
const DATA: &[&str] = &[
    "/api/nodes",
    "/api/nodes/{id}",
    "/api/folders",
    "/api/files/{id}/content",
    "/api/files/{id}/thumbnail",
    "/api/trash",
    "/api/trash/{id}",
    "/api/trash/{id}/restore",
    "/api/storage",
    "/api/photos",
    "/api/photos/months",
    "/api/photos/{id}",
    "/api/media/status",
    "/api/albums",
    "/api/albums/{id}",
    "/api/albums/{id}/items",
    "/api/albums/{id}/items/remove",
    "/api/search/documents",
    "/api/uploads",
    "/api/uploads/{id}",
    "/api/uploads/{id}/chunks/{index}",
    "/api/uploads/{id}/complete",
];

/// Operations that also take a per-upload token (`Authorization: Upload squ_…`),
/// for the upload in their path only. Every other route refuses it.
const UPLOAD_TOKEN: &[&str] = &[
    "GET /api/uploads/{id}",
    "PUT /api/uploads/{id}/chunks/{index}",
];

fn takes_upload_token(method: &Method, path: &str) -> bool {
    UPLOAD_TOKEN.contains(&format!("{method} {path}").as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Public,
    Session,
    Device,
    Data,
}

fn access_of(method: &Method, path: &str) -> Access {
    let lists = [
        (PUBLIC, Access::Public),
        (SESSION, Access::Session),
        (DEVICE, Access::Device),
        (DATA, Access::Data),
    ];
    let find = |entry: &str| -> Vec<Access> {
        lists
            .iter()
            .filter(|(list, _)| list.contains(&entry))
            .map(|(_, access)| *access)
            .collect()
    };
    let operation = find(&format!("{method} {path}"));
    let found = if operation.is_empty() {
        find(path)
    } else {
        operation
    };
    assert_eq!(
        found.len(),
        1,
        "{method} {path} must be listed in exactly one of PUBLIC, SESSION, DEVICE, DATA \
         (route_auth_matrix.rs); found in {found:?}"
    );
    found[0]
}

fn contract() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("openapi.json")).expect("JSON")
}

/// Every `(method, path)` of the contract, logouts last (they end the credentials).
fn operations() -> Vec<(Method, String)> {
    let doc = contract();
    let mut ops: Vec<(Method, String)> = doc["paths"]
        .as_object()
        .expect("paths")
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .expect("path item")
                .keys()
                .filter_map(|method| method.to_uppercase().parse::<Method>().ok())
                .map(|method| (method, path.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    ops.sort_by_key(|(_, path)| path.contains("logout"));
    ops
}

fn concrete(path: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    path.replace("/api/admin/jobs/{id}", "/api/admin/jobs/1")
        .replace("{index}", "0")
        .replace("{token}", "some-token")
        .replace("{id}", &id)
}

fn security_of(method: &Method, path: &str, access: Access) -> Value {
    let session = if method == Method::GET {
        json!({ "session_cookie": [] })
    } else {
        json!({ "session_cookie": [], "csrf_token": [] })
    };
    match access {
        Access::Public => json!([{}]),
        Access::Session => json!([session]),
        Access::Device => json!([{ "device_bearer": [] }]),
        Access::Data if takes_upload_token(method, path) => {
            json!([{ "device_bearer": [] }, { "upload_token": [] }, session])
        }
        Access::Data => json!([{ "device_bearer": [] }, session]),
    }
}

#[test]
fn the_contract_declares_each_routes_credentials() {
    let doc = contract();
    for (method, path) in operations() {
        let operation = &doc["paths"][&path][method.as_str().to_lowercase()];
        assert_eq!(
            operation["security"],
            security_of(&method, &path, access_of(&method, &path)),
            "{method} {path}: `security` in openapi.json does not match its callers"
        );
    }
}

struct Credentials {
    browser: TestServer,
    csrf: HeaderValue,
    bearer: (HeaderName, HeaderValue),
    /// An open upload of the device and its per-upload token.
    upload_id: String,
    upload_token: HeaderValue,
}

async fn credentials(instance: &common::Instance) -> Credentials {
    let browser = instance.browser();
    let me = common::login(&browser, "alice").await;
    let login = device_login(instance, "alice", "Pixel").await;
    let (name, value) = bearer(&access(&login));
    let files = common::files::root_id(instance, &me, "Files").await;
    let content = uuid::Uuid::now_v7().to_string();
    let started = instance
        .browser()
        .post("/api/uploads")
        .add_header(name.clone(), value.clone())
        .json(&json!({
            "parent_id": files, "name": format!("{content}.bin"), "size": 4096,
            "sha256": common::files::sha256_hex(content.as_bytes()),
        }))
        .await;
    started.assert_status(StatusCode::CREATED);
    let session: Value = started.json();
    Credentials {
        browser,
        csrf: common::csrf(&me),
        bearer: (name, value),
        upload_id: session["upload_id"].as_str().unwrap().to_owned(),
        upload_token: HeaderValue::from_str(&format!(
            "Upload {}",
            session["upload_token"].as_str().unwrap()
        ))
        .unwrap(),
    }
}

fn send(server: &TestServer, method: &Method, path: &str) -> axum_test::TestRequest {
    let request = server.method(method.clone(), path);
    if method == Method::GET || method == Method::DELETE {
        request
    } else {
        request.json(&json!({}))
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn every_route_accepts_exactly_its_callers(pool: PgPool) {
    let instance = common::Instance::new(pool);
    common::setup_admin(&instance.browser(), "alice").await;
    let mut creds = credentials(&instance).await;
    for (method, path) in operations() {
        let access = access_of(&method, &path);
        let url = concrete(&path);
        if path.contains("logout") {
            creds = credentials(&instance).await;
        }

        let anonymous = send(&instance.browser(), &method, &url).await;
        let status = anonymous.status_code();
        match access {
            Access::Public => assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {path}"),
            _ => assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "anonymous {method} {path}"
            ),
        }
        if access == Access::Public {
            continue;
        }

        // A per-upload token, aimed at its own upload wherever the path has one.
        let own_upload = path
            .replace("{id}", &creds.upload_id)
            .replace("{index}", "0");
        let target = if path.starts_with("/api/uploads/{id}") {
            own_upload
        } else {
            url.clone()
        };
        let by_upload_token = send(&instance.browser(), &method, &target)
            .add_header(AUTHORIZATION, creds.upload_token.clone())
            .await;
        let status = by_upload_token.status_code();
        if takes_upload_token(&method, &path) {
            assert!(
                status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
                "upload token {method} {path}: {status}"
            );
        } else {
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "upload token {method} {path}"
            );
        }

        let (name, value) = creds.bearer.clone();
        let phone = instance.browser();
        let by_bearer = send(&phone, &method, &url).add_header(name, value).await;
        let status = by_bearer.status_code();
        match access {
            Access::Session => {
                assert_eq!(status, StatusCode::UNAUTHORIZED, "bearer {method} {path}")
            }
            _ => assert!(
                status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
                "bearer {method} {path}: {status}"
            ),
        }

        let by_session = send(&creds.browser, &method, &url)
            .add_header(common::CSRF, creds.csrf.clone())
            .await;
        let status = by_session.status_code();
        match access {
            Access::Device => {
                assert_eq!(status, StatusCode::FORBIDDEN, "session {method} {path}");
                assert_eq!(common::files::code(&by_session), "forbidden");
            }
            _ => assert!(
                status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN,
                "session {method} {path}: {status}"
            ),
        }
    }
}
