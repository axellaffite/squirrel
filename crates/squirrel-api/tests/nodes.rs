mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn mkdir(
    browser: &axum_test::TestServer,
    me: &Value,
    parent: Uuid,
    name: &str,
) -> axum_test::TestResponse {
    browser
        .post("/api/folders")
        .add_header(common::CSRF, common::csrf(me))
        .json(&json!({ "parent_id": parent, "name": name }))
        .await
}

async fn patch(
    browser: &axum_test::TestServer,
    me: &Value,
    id: &str,
    body: Value,
) -> axum_test::TestResponse {
    browser
        .patch(&format!("/api/nodes/{id}"))
        .add_header(common::CSRF, common::csrf(me))
        .json(&body)
        .await
}

fn names(listing: &Value) -> Vec<String> {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn lists_the_top_level_folders(pool: PgPool) {
    let browser = common::server(pool);
    common::setup_admin(&browser, "alice").await;
    let listing: Value = browser.get("/api/nodes").await.json();
    assert_eq!(names(&listing), ["Documents", "Files", "Photos"]);
    assert!(
        listing["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["is_root"] == true && n["kind"] == "folder")
    );
    assert_eq!(listing["next_cursor"], Value::Null);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_lists_and_paginates_folders(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    for name in ["c", "a", "b"] {
        mkdir(&browser, &me, parent, name)
            .await
            .assert_status(StatusCode::CREATED);
    }
    let dup = mkdir(&browser, &me, parent, "a").await;
    dup.assert_status(StatusCode::CONFLICT);
    assert_eq!(files::code(&dup), "name_conflict");

    let first: Value = browser
        .get(&format!("/api/nodes?parent_id={parent}&limit=2"))
        .await
        .json();
    let cursor = first["next_cursor"].as_str().unwrap();
    let second: Value = browser
        .get(&format!(
            "/api/nodes?parent_id={parent}&limit=2&cursor={cursor}"
        ))
        .await
        .json();
    let all: Vec<String> = names(&first).into_iter().chain(names(&second)).collect();
    assert_eq!(all, ["a", "b", "c"]);
    assert_eq!(second["next_cursor"], Value::Null);
    browser
        .get(&format!("/api/nodes?parent_id={parent}&cursor=garbage!"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn node_details_include_the_breadcrumb(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let docs = files::root_id(&instance, &me, "Documents").await;
    let taxes: Value = mkdir(&browser, &me, docs, "Taxes").await.json();
    let details: Value = browser
        .get(&format!("/api/nodes/{}", taxes["id"].as_str().unwrap()))
        .await
        .json();
    assert_eq!(details["node"]["name"], "Taxes");
    assert_eq!(
        details["path"],
        json!([{ "id": docs, "name": "Documents" }])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renames_and_moves_with_conflict_and_cycle_checks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let a: Value = mkdir(&browser, &me, parent, "a").await.json();
    let b: Value = mkdir(&browser, &me, parent, "b").await.json();
    let (a_id, b_id) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());

    let renamed: Value = patch(&browser, &me, a_id, json!({ "name": "alpha" }))
        .await
        .json();
    assert_eq!(renamed["name"], "alpha");
    patch(&browser, &me, a_id, json!({ "name": "b" }))
        .await
        .assert_status(StatusCode::CONFLICT);
    let moved: Value = patch(&browser, &me, b_id, json!({ "parent_id": a_id }))
        .await
        .json();
    assert_eq!(moved["parent_id"], a_id);
    patch(&browser, &me, a_id, json!({ "parent_id": b_id }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(&browser, &me, a_id, json!({}))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(&browser, &me, a_id, json!({ "name": "x/y" }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(
        &browser,
        &me,
        &parent.to_string(),
        json!({ "name": "Stuff" }),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_users_nodes_are_invisible(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_files = files::root_id(&instance, &alice_me, "Files").await;
    let folder: Value = mkdir(&alice, &alice_me, alice_files, "private")
        .await
        .json();
    let id = folder["id"].as_str().unwrap();

    bob.get(&format!("/api/nodes/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    bob.get(&format!("/api/nodes?parent_id={alice_files}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    patch(&bob, &bob_me, id, json!({ "name": "mine" }))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    mkdir(&bob, &bob_me, alice_files, "intruder")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
