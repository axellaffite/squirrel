mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files::{code, root_id, user_id};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_db::documents;
use uuid::Uuid;

/// What a seeded document looks like.
struct Doc<'a> {
    name: &'a str,
    mime: &'a str,
    /// Shared by every document seeded with the same value (deduplicated content).
    sha256: String,
    text: Option<&'a str>,
    thumbs_ready: bool,
}

impl<'a> Doc<'a> {
    fn new(name: &'a str, text: Option<&'a str>) -> Self {
        let simple = Uuid::now_v7().simple().to_string();
        Self {
            name,
            mime: "text/plain",
            sha256: format!("{simple}{simple}"),
            text,
            thumbs_ready: false,
        }
    }
}

/// A file node in `parent`, its blob and media rows, and its text.
async fn seed(pool: &PgPool, me: &Value, parent: Uuid, doc: &Doc<'_>) -> Uuid {
    let kind = if doc.mime == "application/pdf" {
        "pdf"
    } else {
        "text"
    };
    sqlx::query(
        "INSERT INTO blobs (sha256, size, mime) VALUES ($1, 10, $2) ON CONFLICT DO NOTHING",
    )
    .bind(&doc.sha256)
    .bind(doc.mime)
    .execute(pool)
    .await
    .unwrap();
    let (thumbs, version, blurhash) = if doc.thumbs_ready {
        ("ready", 1_i16, Some("LEHV6nWB2yk8"))
    } else {
        ("unsupported", 0, None)
    };
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, thumbs_state, thumbs_version, \
                            blurhash, text_state, page_count) \
         VALUES ($1, $2, 'unsupported', $3, $4, $5, 'pending', $6) ON CONFLICT DO NOTHING",
    )
    .bind(&doc.sha256)
    .bind(kind)
    .bind(thumbs)
    .bind(version)
    .bind(blurhash)
    .bind((kind == "pdf").then_some(3))
    .execute(pool)
    .await
    .unwrap();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, 10, $5) RETURNING id",
    )
    .bind(user_id(me))
    .bind(parent)
    .bind(doc.name)
    .bind(&doc.sha256)
    .bind(doc.mime)
    .fetch_one(pool)
    .await
    .unwrap();
    if let Some(text) = doc.text {
        documents::store_text(pool, &doc.sha256, text)
            .await
            .unwrap();
    }
    id
}

async fn search(browser: &TestServer, query: &str) -> Value {
    let response = browser
        .get("/api/search/documents")
        .add_query_param("q", query)
        .await;
    response.assert_status_ok();
    response.json()
}

fn ids(results: &Value) -> Vec<Uuid> {
    results["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["node"]["id"].as_str().unwrap().parse().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_documents_by_content_and_by_name(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let docs = root_id(&instance, &me, "Documents").await;
    let invoice = seed(
        &pool,
        &me,
        docs,
        &Doc {
            mime: "application/pdf",
            thumbs_ready: true,
            ..Doc::new("invoice.pdf", Some("Facture École Électricité 2026"))
        },
    )
    .await;
    let named = seed(&pool, &me, docs, &Doc::new("ecole-inscription.txt", None)).await;

    let results = search(&browser, "ecole elec").await;
    assert_eq!(ids(&results), vec![invoice]);
    assert_eq!(results["has_more"], false);
    let hit = &results["items"][0];
    assert_eq!(hit["page_count"], 3);
    assert_eq!(
        hit["node"]["thumbnail"],
        json!({ "version": 1, "blurhash": "LEHV6nWB2yk8" })
    );
    let snippet = hit["snippet"].as_array().unwrap();
    assert!(
        snippet.contains(&json!({ "text": "École", "highlight": true })),
        "{snippet:?}"
    );
    let mut highlight = true;
    for part in snippet {
        let text = part["text"].as_str().unwrap();
        assert!(!text.chars().any(char::is_control), "{text:?}");
        // Neighbouring parts never share a kind.
        assert_ne!(part["highlight"], highlight, "{snippet:?}");
        highlight = part["highlight"].as_bool().unwrap();
    }

    let results = search(&browser, "École").await;
    assert_eq!(ids(&results), vec![named, invoice], "name matches first");
    assert_eq!(results["items"][0]["snippet"], json!([]));
    assert_eq!(results["items"][0]["node"]["thumbnail"], Value::Null);
    assert_eq!(search(&browser, "' & !").await["items"], json!([]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refuses_empty_long_or_far_queries(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    common::setup_admin(&browser, "alice").await;
    let long = "x".repeat(201);
    for params in [
        vec![],
        vec![("q", "")],
        vec![("q", "   ")],
        vec![("q", long.as_str())],
        vec![("q", "ok"), ("offset", "500")],
        vec![("q", "ok"), ("limit", "51")],
    ] {
        let mut request = browser.get("/api/search/documents");
        for (name, value) in &params {
            request = request.add_query_param(name, value);
        }
        let response = request.await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{params:?}");
    }
    let response = browser.get("/api/search/documents?q=ok&offset=-1").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn never_returns_another_users_files(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_docs = root_id(&instance, &alice_me, "Documents").await;
    let bob_docs = root_id(&instance, &bob_me, "Documents").await;
    let shared = Doc::new("plan.txt", Some("secret acorn stash"));
    let alices = seed(&pool, &alice_me, alice_docs, &shared).await;
    // Bob holds the same content (one blob), then trashes it.
    let bobs = seed(
        &pool,
        &bob_me,
        bob_docs,
        &Doc {
            sha256: shared.sha256.clone(),
            ..Doc::new("copy.txt", None)
        },
    )
    .await;

    assert_eq!(ids(&search(&bob, "acorn").await), vec![bobs]);
    assert_eq!(ids(&search(&alice, "acorn").await), vec![alices]);
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(bobs)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(search(&bob, "acorn stash").await["items"], json!([]));
    assert_eq!(search(&bob, "plan").await["items"], json!([]));

    let anonymous = instance.browser();
    anonymous
        .get("/api/search/documents?q=acorn")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pages_with_offset(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let docs = root_id(&instance, &me, "Documents").await;
    for i in 0..3 {
        seed(
            &pool,
            &me,
            docs,
            &Doc::new(&format!("report-{i}.txt"), None),
        )
        .await;
    }
    let first: Value = browser
        .get("/api/search/documents?q=report&limit=2")
        .await
        .json();
    assert_eq!(ids(&first).len(), 2);
    assert_eq!(first["has_more"], true);
    let second: Value = browser
        .get("/api/search/documents?q=report&limit=2&offset=2")
        .await
        .json();
    assert_eq!(ids(&second).len(), 1);
    assert_eq!(second["has_more"], false);
}
