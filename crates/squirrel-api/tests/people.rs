mod common;

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use common::files::{root_id, user_id};
use common::photos::photo;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

async fn get(browser: &TestServer, url: &str) -> Value {
    let response = browser.get(url).await;
    response.assert_status_ok();
    response.json()
}

async fn send(
    browser: &TestServer,
    me: &Value,
    method: &str,
    url: &str,
    body: Value,
) -> TestResponse {
    let request = match method {
        "PATCH" => browser.patch(url),
        "POST" => browser.post(url),
        "PUT" => browser.put(url),
        "DELETE" => browser.delete(url),
        other => panic!("{other}"),
    };
    request
        .add_header(common::CSRF, common::csrf(me))
        .json(&body)
        .await
}

/// A person of `owner` (optionally named) with one face on each photo, the
/// face's score rising with its index; returns the person and the faces.
async fn person(
    pool: &PgPool,
    owner: Uuid,
    name: Option<&str>,
    photos: &[Uuid],
) -> (Uuid, Vec<Uuid>) {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id, name) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(owner)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    let mut faces = Vec::new();
    for (i, node) in photos.iter().enumerate() {
        faces.push(face(pool, owner, *node, Some(id), 0.1 + 0.1 * i as f32).await);
    }
    (id, faces)
}

async fn face(pool: &PgPool, owner: Uuid, node: Uuid, person: Option<Uuid>, x: f32) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, landmarks, \
                            score, embedding, person_id) \
         SELECT $1, $2, blob_sha256, 'yunet-sface-2021dec', $4, 0.1, 0.2, 0.2, \
                array_fill(0.2::real, ARRAY[10]), 0.4 + $4, \
                array_fill(0.0::real, ARRAY[128])::halfvec, $5 \
           FROM nodes WHERE id = $3",
    )
    .bind(id)
    .bind(owner)
    .bind(node)
    .bind(x)
    .bind(person)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn assigned(pool: &PgPool, face: Uuid) -> (Option<Uuid>, String) {
    sqlx::query_as("SELECT person_id, assigned_by FROM faces WHERE id = $1")
        .bind(face)
        .fetch_one(pool)
        .await
        .unwrap()
}

struct World {
    alice: TestServer,
    alice_me: Value,
    bob: TestServer,
    bob_me: Value,
    photos: Vec<Uuid>,
    owner: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let parent = root_id(&instance, &alice_me, "Photos").await;
    let mut photos = Vec::new();
    for (i, at) in [
        datetime!(2024-01-01 10:00),
        datetime!(2024-02-01 10:00),
        datetime!(2024-03-01 10:00),
    ]
    .into_iter()
    .enumerate()
    {
        photos.push(photo(pool, &alice_me, parent, &format!("p{i}.jpg"), at).await);
    }
    let owner = user_id(&alice_me);
    World {
        alice,
        alice_me,
        bob,
        bob_me,
        photos,
        owner,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn people_are_listed_named_first_then_by_face_count(pool: PgPool) {
    let w = world(&pool).await;
    let (many, faces_many) = person(&pool, w.owner, None, &w.photos).await;
    let (named, _) = person(&pool, w.owner, Some("Zoé"), &w.photos[..1]).await;
    let (hidden, _) = person(&pool, w.owner, None, &w.photos[..2]).await;
    sqlx::query("UPDATE people SET hidden = true WHERE id = $1")
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    let list = get(&w.alice, "/api/people").await;
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [named.to_string(), many.to_string()]);
    assert_eq!(list[1]["face_count"], 3);
    let cover = &list[1]["cover"];
    assert_eq!(
        cover["face_id"],
        faces_many[2].to_string(),
        "the best score"
    );
    assert_eq!(cover["photo_id"], w.photos[2].to_string());
    assert_eq!(cover["thumbnail_version"], 1);
    assert!((cover["box"]["x"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    let all = get(&w.alice, "/api/people?include_hidden=true").await;
    assert_eq!(all.as_array().unwrap().len(), 3);
    assert_eq!(get(&w.bob, "/api/people").await, json!([]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn rename_hide_and_choose_a_cover(pool: PgPool) {
    let w = world(&pool).await;
    let (id, faces) = person(&pool, w.owner, None, &w.photos).await;
    let url = format!("/api/people/{id}");
    let renamed: Value = send(
        &w.alice,
        &w.alice_me,
        "PATCH",
        &url,
        json!({"name": "  Anna  "}),
    )
    .await
    .json();
    assert_eq!(renamed["name"], "Anna");
    for bad in [
        json!({"name": "   "}),
        json!({"name": "x".repeat(101)}),
        json!({}),
    ] {
        send(&w.alice, &w.alice_me, "PATCH", &url, bad)
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let cover = send(
        &w.alice,
        &w.alice_me,
        "PATCH",
        &url,
        json!({"cover_face_id": faces[0], "hidden": true}),
    )
    .await
    .json::<Value>();
    assert_eq!(cover["cover"]["face_id"], faces[0].to_string());
    assert_eq!(cover["hidden"], true);
    let (_, other_faces) = person(&pool, w.owner, None, &w.photos[..1]).await;
    send(
        &w.alice,
        &w.alice_me,
        "PATCH",
        &url,
        json!({"cover_face_id": other_faces[0]}),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let cleared: Value = send(&w.alice, &w.alice_me, "PATCH", &url, json!({"name": null}))
        .await
        .json();
    assert_eq!(cleared["name"], Value::Null);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn merge_moves_and_pins_the_faces(pool: PgPool) {
    let w = world(&pool).await;
    let (into, _) = person(&pool, w.owner, Some("Anna"), &w.photos[..1]).await;
    let (from, moved) = person(&pool, w.owner, None, &w.photos[1..]).await;
    let url = format!("/api/people/{into}/merge");
    send(&w.alice, &w.alice_me, "POST", &url, json!({"from": [into]}))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    send(
        &w.alice,
        &w.alice_me,
        "POST",
        &url,
        json!({"from": [Uuid::now_v7()]}),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);
    let merged: Value = send(&w.alice, &w.alice_me, "POST", &url, json!({"from": [from]}))
        .await
        .json();
    assert_eq!(merged["face_count"], 3);
    for face in moved {
        assert_eq!(assigned(&pool, face).await, (Some(into), "user".into()));
    }
    w.alice
        .get(&format!("/api/people/{from}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn split_makes_a_new_pinned_person(pool: PgPool) {
    let w = world(&pool).await;
    let (id, faces) = person(&pool, w.owner, None, &w.photos).await;
    let (_, stranger) = person(&pool, w.owner, None, &w.photos[..1]).await;
    let url = format!("/api/people/{id}/split");
    send(
        &w.alice,
        &w.alice_me,
        "POST",
        &url,
        json!({"face_ids": [faces[0], stranger[0]]}),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let new: Value = send(
        &w.alice,
        &w.alice_me,
        "POST",
        &url,
        json!({"face_ids": [faces[0], faces[1]], "name": "Ben"}),
    )
    .await
    .json();
    assert_eq!(
        (new["name"].clone(), new["face_count"].clone()),
        (json!("Ben"), json!(2))
    );
    let new_id: Uuid = new["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        assigned(&pool, faces[0]).await,
        (Some(new_id), "user".into())
    );
    assert_eq!(assigned(&pool, faces[2]).await.0, Some(id));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_persons_faces_page_newest_photo_first(pool: PgPool) {
    let w = world(&pool).await;
    let (id, faces) = person(&pool, w.owner, None, &w.photos).await;
    let first = get(&w.alice, &format!("/api/people/{id}/faces?limit=2")).await;
    let items = first["items"].as_array().unwrap();
    assert_eq!(items[0]["id"], faces[2].to_string());
    assert_eq!(items[0]["photo_id"], w.photos[2].to_string());
    assert_eq!(items[0]["assigned_by"], "auto");
    assert_eq!(items[0]["thumbnail_version"], 1);
    assert!((items[0]["box"]["x"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    assert_eq!(items[1]["id"], faces[1].to_string());
    let cursor = first["next_cursor"].as_str().unwrap();
    let rest = get(
        &w.alice,
        &format!("/api/people/{id}/faces?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(rest["items"][0]["id"], faces[0].to_string());
    assert_eq!(rest["next_cursor"], Value::Null);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn faces_on_a_photo_and_assigning_them(pool: PgPool) {
    let w = world(&pool).await;
    let (anna, _) = person(&pool, w.owner, Some("Anna"), &w.photos[..1]).await;
    let loose = face(&pool, w.owner, w.photos[0], None, 0.6).await;
    let on_photo = get(&w.alice, &format!("/api/photos/{}/faces", w.photos[0])).await;
    assert_eq!(on_photo.as_array().unwrap().len(), 2);
    assert_eq!(on_photo[0]["person"]["name"], "Anna");
    assert_eq!(on_photo[1]["person"], Value::Null);

    let url = format!("/api/faces/{loose}/person");
    let to_anna: Value = send(
        &w.alice,
        &w.alice_me,
        "PUT",
        &url,
        json!({"person_id": anna}),
    )
    .await
    .json();
    assert_eq!(to_anna["person_id"], anna.to_string());
    assert_eq!(assigned(&pool, loose).await, (Some(anna), "user".into()));
    let created: Value = send(
        &w.alice,
        &w.alice_me,
        "PUT",
        &url,
        json!({"new_person": {"name": "Ben"}}),
    )
    .await
    .json();
    let ben: Uuid = created["person_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        get(&w.alice, &format!("/api/people/{ben}")).await["name"],
        "Ben"
    );
    send(&w.alice, &w.alice_me, "PUT", &url, json!({}))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let nobody: Value = send(&w.alice, &w.alice_me, "DELETE", &url, json!({}))
        .await
        .json();
    assert_eq!(nobody["person_id"], Value::Null);
    assert_eq!(assigned(&pool, loose).await, (None, "user".into()));
    // Ben was named: he stays, without faces.
    assert_eq!(
        get(&w.alice, &format!("/api/people/{ben}")).await["face_count"],
        0
    );

    let details = get(&w.alice, &format!("/api/photos/{}", w.photos[0])).await;
    assert_eq!(details["people"], json!([{"id": anna, "name": "Anna"}]));
    let filtered = get(&w.alice, &format!("/api/photos?person={anna}")).await;
    assert_eq!(filtered["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_users_people_and_faces_are_not_found(pool: PgPool) {
    let w = world(&pool).await;
    let (id, faces) = person(&pool, w.owner, Some("Anna"), &w.photos).await;
    for url in [
        format!("/api/people/{id}"),
        format!("/api/people/{id}/faces"),
        format!("/api/photos/{}/faces", w.photos[0]),
    ] {
        w.bob.get(&url).await.assert_status(StatusCode::NOT_FOUND);
    }
    for (method, url, body) in [
        ("PATCH", format!("/api/people/{id}"), json!({"name": "x"})),
        (
            "POST",
            format!("/api/people/{id}/merge"),
            json!({"from": [Uuid::now_v7()]}),
        ),
        (
            "POST",
            format!("/api/people/{id}/split"),
            json!({"face_ids": [faces[0]]}),
        ),
        (
            "PUT",
            format!("/api/faces/{}/person", faces[0]),
            json!({"new_person": {}}),
        ),
        (
            "DELETE",
            format!("/api/faces/{}/person", faces[0]),
            json!({}),
        ),
    ] {
        send(&w.bob, &w.bob_me, method, &url, body)
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }
    assert_eq!(assigned(&pool, faces[0]).await, (Some(id), "auto".into()));
    assert!(
        get(&w.bob, &format!("/api/photos?person={id}")).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
