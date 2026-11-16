//! Which callers every route accepts, checked against both the running router
//! and the committed `openapi.json` (the phone's client is generated from it).
//! A route missing from these lists fails: a new route must choose.

mod common;

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

/// Browser session only: account, invitations, administration, device management.
const SESSION: &[&str] = &[
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Public,
    Session,
    Device,
    Data,
}

fn access_of(path: &str) -> Access {
    let lists = [
        (PUBLIC, Access::Public),
        (SESSION, Access::Session),
        (DEVICE, Access::Device),
        (DATA, Access::Data),
    ];
    let found: Vec<Access> = lists
        .iter()
        .filter(|(list, _)| list.contains(&path))
        .map(|(_, access)| *access)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{path} must be listed in exactly one of PUBLIC, SESSION, DEVICE, DATA \
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

fn security_of(method: &Method, access: Access) -> Value {
    let session = if method == Method::GET {
        json!({ "session_cookie": [] })
    } else {
        json!({ "session_cookie": [], "csrf_token": [] })
    };
    match access {
        Access::Public => json!([{}]),
        Access::Session => json!([session]),
        Access::Device => json!([{ "device_bearer": [] }]),
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
            security_of(&method, access_of(&path)),
            "{method} {path}: `security` in openapi.json does not match its callers"
        );
    }
}

struct Credentials {
    browser: TestServer,
    csrf: HeaderValue,
    bearer: (HeaderName, HeaderValue),
}

async fn credentials(instance: &common::Instance) -> Credentials {
    let browser = instance.browser();
    let me = common::login(&browser, "alice").await;
    let login = device_login(instance, "alice", "Pixel").await;
    Credentials {
        browser,
        csrf: common::csrf(&me),
        bearer: bearer(&access(&login)),
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
        let access = access_of(&path);
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
