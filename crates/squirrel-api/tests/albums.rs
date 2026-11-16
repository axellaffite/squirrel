mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{code, root_id};
use common::photos::photo;
use common::{CSRF, csrf};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn post(browser: &TestServer, me: &Value, url: &str, body: Value) -> TestResponse {
    browser
        .post(url)
        .add_header(CSRF, csrf(me))
        .json(&body)
        .await
}

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_album_lives_and_dies(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let a = photo(&pool, &me, photos, "a.jpg", datetime!(2024-07-01 10:00)).await;
    let b = photo(&pool, &me, photos, "b.jpg", datetime!(2024-07-02 10:00)).await;
    let c = photo(&pool, &me, photos, "c.jpg", datetime!(2024-07-03 10:00)).await;

    let created = post(
        &browser,
        &me,
        "/api/albums",
        json!({ "name": " Summer ", "node_ids": [a] }),
    )
    .await;
    created.assert_status(StatusCode::CREATED);
    let album: Value = created.json();
    let id = album["id"].as_str().unwrap().to_owned();
    assert_eq!(album["name"], "Summer");
    assert_eq!(album["item_count"], 1);
    assert_eq!(album["cover"]["id"], a.to_string());
    assert!(album["updated_at"].is_string());

    let added = post(
        &browser,
        &me,
        &format!("/api/albums/{id}/items"),
        json!({ "node_ids": [a, b, c] }),
    )
    .await;
    added.assert_status_ok();
    assert_eq!(added.json::<Value>(), json!({ "changed": 2 }));

    let first: Value = browser
        .get(&format!("/api/albums/{id}/items?limit=2"))
        .await
        .json();
    assert_eq!(names(&first), ["c.jpg", "b.jpg"]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let rest: Value = browser
        .get(&format!("/api/albums/{id}/items?limit=2&cursor={cursor}"))
        .await
        .json();
    assert_eq!(names(&rest), ["a.jpg"]);
    assert_eq!(rest["next_cursor"], Value::Null);

    let patched = browser
        .patch(&format!("/api/albums/{id}"))
        .add_header(CSRF, csrf(&me))
        .json(&json!({ "name": "Summer 2024", "cover_node_id": b }))
        .await;
    patched.assert_status_ok();
    let patched: Value = patched.json();
    assert_eq!(
        (&patched["name"], &patched["cover"]["id"]),
        (&json!("Summer 2024"), &json!(b))
    );
    assert_eq!(patched["item_count"], 3);

    let removed = post(
        &browser,
        &me,
        &format!("/api/albums/{id}/items/remove"),
        json!({ "node_ids": [b] }),
    )
    .await;
    assert_eq!(removed.json::<Value>(), json!({ "changed": 1 }));
    let fetched: Value = browser.get(&format!("/api/albums/{id}")).await.json();
    assert_eq!(
        fetched["cover"]["id"],
        c.to_string(),
        "the newest photo once the cover left"
    );

    let listed: Value = browser.get("/api/albums").await.json();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["item_count"], 2);

    let details: Value = browser.get(&format!("/api/photos/{a}")).await.json();
    assert_eq!(
        details["albums"],
        json!([{ "id": id, "name": "Summer 2024" }])
    );

    browser
        .delete(&format!("/api/albums/{id}"))
        .add_header(CSRF, csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    browser
        .get(&format!("/api/albums/{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let timeline: Value = browser.get("/api/photos").await.json();
    assert_eq!(names(&timeline), ["c.jpg", "b.jpg", "a.jpg"], "photos stay");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn names_ids_and_covers_are_validated(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let photos = root_id(&instance, &me, "Photos").await;
    let inside = photo(&pool, &me, photos, "in.jpg", datetime!(2024-07-01 10:00)).await;
    let outside = photo(&pool, &me, photos, "out.jpg", datetime!(2024-07-02 10:00)).await;
    for name in [
        json!(""),
        json!("   "),
        json!("x".repeat(101)),
        json!("a\u{0007}b"),
        json!(5),
    ] {
        let response = post(&browser, &me, "/api/albums", json!({ "name": name })).await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{name}");
    }
    let too_many: Vec<Uuid> = (0..501).map(|_| Uuid::now_v7()).collect();
    let response = post(
        &browser,
        &me,
        "/api/albums",
        json!({ "name": "x", "node_ids": too_many }),
    )
    .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let album: Value = post(
        &browser,
        &me,
        "/api/albums",
        json!({ "name": "A", "node_ids": [inside] }),
    )
    .await
    .json();
    let id = album["id"].as_str().unwrap();
    for body in [
        json!({ "node_ids": [] }),
        json!({ "node_ids": too_many }),
        json!({}),
    ] {
        for path in ["items", "items/remove"] {
            let response = post(
                &browser,
                &me,
                &format!("/api/albums/{id}/{path}"),
                body.clone(),
            )
            .await;
            response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    for body in [
        json!({}),
        json!({ "cover_node_id": outside }),
        json!({ "name": "" }),
    ] {
        let response = browser
            .patch(&format!("/api/albums/{id}"))
            .add_header(CSRF, csrf(&me))
            .json(&body)
            .await;
        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&response), "validation_error", "{body}");
    }
    let response = browser
        .get(&format!("/api/albums/{id}/items?cursor=nope"))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_users_albums_and_photos_are_out_of_reach(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &alice_me, "Photos").await;
    let mine = photo(
        &pool,
        &alice_me,
        photos,
        "a.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let album: Value = post(
        &alice,
        &alice_me,
        "/api/albums",
        json!({ "name": "Private", "node_ids": [mine] }),
    )
    .await
    .json();
    let id = album["id"].as_str().unwrap();
    let ids = json!({ "node_ids": [mine] });

    let responses = vec![
        bob.get(&format!("/api/albums/{id}")).await,
        bob.get(&format!("/api/albums/{id}/items")).await,
        bob.patch(&format!("/api/albums/{id}"))
            .add_header(CSRF, csrf(&bob_me))
            .json(&json!({ "name": "Mine now" }))
            .await,
        bob.delete(&format!("/api/albums/{id}"))
            .add_header(CSRF, csrf(&bob_me))
            .await,
        post(
            &bob,
            &bob_me,
            &format!("/api/albums/{id}/items"),
            ids.clone(),
        )
        .await,
        post(
            &bob,
            &bob_me,
            &format!("/api/albums/{id}/items/remove"),
            ids.clone(),
        )
        .await,
    ];
    for response in responses {
        response.assert_status(StatusCode::NOT_FOUND);
        assert_eq!(code(&response), "not_found");
    }
    let bobs: Value = bob.get("/api/albums").await.json();
    assert_eq!(bobs, json!([]));

    // Alice's photo ids do nothing in Bob's own album.
    let own: Value = post(
        &bob,
        &bob_me,
        "/api/albums",
        json!({ "name": "Bob", "node_ids": [mine] }),
    )
    .await
    .json();
    assert_eq!(own["item_count"], 0);
    let own_id = own["id"].as_str().unwrap();
    let added = post(&bob, &bob_me, &format!("/api/albums/{own_id}/items"), ids).await;
    assert_eq!(added.json::<Value>(), json!({ "changed": 0 }));
    let cover = bob
        .patch(&format!("/api/albums/{own_id}"))
        .add_header(CSRF, csrf(&bob_me))
        .json(&json!({ "cover_node_id": mine }))
        .await;
    cover.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    let still: Value = alice.get(&format!("/api/albums/{id}")).await.json();
    assert_eq!(
        (&still["name"], &still["item_count"]),
        (&json!("Private"), &json!(1))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn writes_need_the_csrf_token_and_reads_a_session(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let album: Value = post(&browser, &me, "/api/albums", json!({ "name": "A" }))
        .await
        .json();
    let id = album["id"].as_str().unwrap();
    let nodes = json!({ "node_ids": [Uuid::now_v7()] });
    let responses = vec![
        browser
            .post("/api/albums")
            .json(&json!({ "name": "B" }))
            .await,
        browser
            .patch(&format!("/api/albums/{id}"))
            .json(&json!({ "name": "B" }))
            .await,
        browser.delete(&format!("/api/albums/{id}")).await,
        browser
            .post(&format!("/api/albums/{id}/items"))
            .json(&nodes)
            .await,
        browser
            .post(&format!("/api/albums/{id}/items/remove"))
            .json(&nodes)
            .await,
    ];
    for response in responses {
        response.assert_status(StatusCode::FORBIDDEN);
        assert_eq!(code(&response), "csrf_invalid");
    }
    let anonymous = common::server(pool);
    for url in [
        "/api/albums".to_owned(),
        format!("/api/albums/{id}"),
        format!("/api/albums/{id}/items"),
    ] {
        anonymous
            .get(&url)
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
}
