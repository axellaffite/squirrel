mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files::{code, root_id, user_id};
use common::photos::photo;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use uuid::Uuid;

/// An open group of `nodes` (anchored at the first) for `owner`.
async fn group(pool: &PgPool, owner: Uuid, nodes: &[Uuid]) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO duplicate_groups (id, owner_id, anchor_phash, anchor_taken_at) \
         VALUES ($1, $2, 0, '2024-07-01 10:00')",
    )
    .bind(id)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    for (i, node) in nodes.iter().enumerate() {
        sqlx::query(
            "INSERT INTO duplicate_members (node_id, owner_id, group_id, distance) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(node)
        .bind(owner)
        .bind(id)
        .bind(i as i16)
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

struct World {
    instance: common::Instance,
    alice: TestServer,
    me: Value,
    bob: TestServer,
    bob_me: Value,
    group: Uuid,
    a: Uuid,
    b: Uuid,
    c: Uuid,
}

async fn world(pool: &PgPool) -> World {
    let instance = common::Instance::new(pool.clone());
    let ((alice, me), (bob, bob_me)) = common::two_users(&instance).await;
    let photos = root_id(&instance, &me, "Photos").await;
    let when = datetime!(2024-07-01 10:00);
    let a = photo(pool, &me, photos, "a.jpg", when).await;
    let b = photo(pool, &me, photos, "b.jpg", when).await;
    let c = photo(pool, &me, photos, "c.jpg", when).await;
    let group = group(pool, user_id(&me), &[a, b, c]).await;
    World {
        instance,
        alice,
        me,
        bob,
        bob_me,
        group,
        a,
        b,
        c,
    }
}

async fn get(browser: &TestServer, url: &str) -> Value {
    let response = browser.get(url).await;
    response.assert_status_ok();
    response.json()
}

async fn post(browser: &TestServer, me: &Value, url: &str, body: Value) -> axum_test::TestResponse {
    browser
        .post(url)
        .add_header(common::CSRF, common::csrf(me))
        .json(&body)
        .await
}

fn timeline(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Whether each of `nodes` is hidden in `group`, in that order.
fn hidden_flags(group: &Value, nodes: &[Uuid]) -> Vec<bool> {
    let members = group["members"].as_array().unwrap();
    assert_eq!(members.len(), nodes.len(), "{group}");
    nodes
        .iter()
        .map(|node| {
            members
                .iter()
                .find(|m| m["asset"]["id"] == json!(node))
                .map(|m| m["hidden"].as_bool().unwrap())
                .unwrap_or_else(|| panic!("{node} not in {group}"))
        })
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn lists_open_groups_with_their_photos(pool: PgPool) {
    let w = world(&pool).await;
    sqlx::query("UPDATE media SET camera_make = 'Fujifilm', camera_model = 'X100'")
        .execute(&pool)
        .await
        .unwrap();
    let page = get(&w.alice, "/api/duplicates").await;
    assert_eq!(page["open_count"], 1);
    assert_eq!(page["next_cursor"], Value::Null);
    let group = &page["items"][0];
    assert_eq!(group["id"], json!(w.group));
    assert_eq!(group["state"], "open");
    assert_eq!(group["taken_at"], "2024-07-01T10:00:00");
    assert_eq!(group["kept_id"], Value::Null);
    let member = &group["members"][0];
    assert_eq!(member["asset"]["width"], 4);
    assert_eq!(member["size"], 10);
    assert_eq!(member["camera_make"], "Fujifilm");
    assert_eq!(member["hidden"], false);
    assert_eq!(group["members"].as_array().unwrap().len(), 3);
    let resolved = get(&w.alice, "/api/duplicates?state=resolved").await;
    assert_eq!(resolved["items"], json!([]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn keeping_one_hides_the_others_everywhere(pool: PgPool) {
    let w = world(&pool).await;
    let url = format!("/api/duplicates/{}/keep", w.group);
    let kept = post(&w.alice, &w.me, &url, json!({ "asset": w.b })).await;
    kept.assert_status_ok();
    let group: Value = kept.json();
    assert_eq!(group["state"], "resolved");
    assert_eq!(group["kept_id"], json!(w.b));
    assert_eq!(hidden_flags(&group, &[w.a, w.b, w.c]), [true, false, true]);
    assert_eq!(
        timeline(&get(&w.alice, "/api/photos").await),
        [w.b.to_string()]
    );
    assert_eq!(get(&w.alice, "/api/duplicates").await["open_count"], 0);
    let resolved = get(&w.alice, "/api/duplicates?state=resolved").await;
    assert_eq!(resolved["items"][0]["id"], json!(w.group));
    // A hidden photo still opens by id.
    get(&w.alice, &format!("/api/photos/{}", w.a)).await;

    // Keeping another one swaps them.
    let swapped: Value = post(&w.alice, &w.me, &url, json!({ "asset": w.a }))
        .await
        .json();
    assert_eq!(
        hidden_flags(&swapped, &[w.a, w.b, w.c]),
        [false, true, true]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restoring_shows_hidden_photos_again(pool: PgPool) {
    let w = world(&pool).await;
    let keep = format!("/api/duplicates/{}/keep", w.group);
    let restore = format!("/api/duplicates/{}/restore", w.group);
    post(&w.alice, &w.me, &keep, json!({ "asset": w.a }))
        .await
        .assert_status_ok();

    let one: Value = post(&w.alice, &w.me, &restore, json!({ "asset": w.c }))
        .await
        .json();
    assert_eq!(hidden_flags(&one, &[w.a, w.b, w.c]), [false, true, false]);
    assert_eq!(one["state"], "resolved", "b is still hidden");

    let all: Value = post(&w.alice, &w.me, &restore, json!({})).await.json();
    assert_eq!(hidden_flags(&all, &[w.a, w.b, w.c]), [false, false, false]);
    assert_eq!(all["state"], "open", "nothing hidden: decide again");
    assert_eq!(timeline(&get(&w.alice, "/api/photos").await).len(), 3);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restoring_the_last_photo_of_a_group_ends_it(pool: PgPool) {
    let w = world(&pool).await;
    post(
        &w.alice,
        &w.me,
        &format!("/api/duplicates/{}/keep", w.group),
        json!({ "asset": w.a }),
    )
    .await
    .assert_status_ok();
    for node in [w.a, w.c] {
        sqlx::query("DELETE FROM nodes WHERE id = $1")
            .bind(node)
            .execute(&pool)
            .await
            .unwrap();
    }
    let resolved = get(&w.alice, "/api/duplicates?state=resolved").await;
    assert_eq!(
        hidden_flags(&resolved["items"][0], &[w.b]),
        [true],
        "kept for its restore"
    );
    let response = post(
        &w.alice,
        &w.me,
        &format!("/api/duplicates/{}/restore", w.group),
        json!({}),
    )
    .await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        timeline(&get(&w.alice, "/api/photos").await),
        [w.b.to_string()]
    );
    assert_eq!(
        get(&w.alice, "/api/duplicates?state=resolved").await["items"],
        json!([])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn another_users_group_is_not_found(pool: PgPool) {
    let w = world(&pool).await;
    assert_eq!(get(&w.bob, "/api/duplicates").await["items"], json!([]));
    assert_eq!(get(&w.bob, "/api/duplicates").await["open_count"], 0);
    for action in ["keep", "restore"] {
        let response = post(
            &w.bob,
            &w.bob_me,
            &format!("/api/duplicates/{}/{action}", w.group),
            json!({ "asset": w.a }),
        )
        .await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
    // Nothing changed for Alice.
    assert_eq!(timeline(&get(&w.alice, "/api/photos").await).len(), 3);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_a_photo_of_the_group_can_be_kept(pool: PgPool) {
    let w = world(&pool).await;
    let photos = root_id(&w.instance, &w.me, "Photos").await;
    let other = photo(
        &pool,
        &w.me,
        photos,
        "other.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    let response = post(
        &w.alice,
        &w.me,
        &format!("/api/duplicates/{}/keep", w.group),
        json!({ "asset": other }),
    )
    .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code(&response), "validation_error");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn decisions_need_the_session_and_csrf(pool: PgPool) {
    let w = world(&pool).await;
    let url = format!("/api/duplicates/{}/keep", w.group);
    let response = w.alice.post(&url).json(&json!({ "asset": w.a })).await;
    response.assert_status(StatusCode::FORBIDDEN);
    let anonymous = w.instance.browser().get("/api/duplicates").await;
    anonymous.assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn groups_are_paged_newest_first(pool: PgPool) {
    let w = world(&pool).await;
    let photos = root_id(&w.instance, &w.me, "Photos").await;
    let when = datetime!(2024-08-01 10:00);
    let x = photo(&pool, &w.me, photos, "x.jpg", when).await;
    let y = photo(&pool, &w.me, photos, "y.jpg", when).await;
    let newer = group(&pool, user_id(&w.me), &[x, y]).await;
    sqlx::query("UPDATE duplicate_groups SET anchor_taken_at = '2024-08-01 10:00' WHERE id = $1")
        .bind(newer)
        .execute(&pool)
        .await
        .unwrap();
    let first = get(&w.alice, "/api/duplicates?limit=1").await;
    assert_eq!(first["items"][0]["id"], json!(newer));
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = get(
        &w.alice,
        &format!("/api/duplicates?limit=1&cursor={cursor}"),
    )
    .await;
    assert_eq!(second["items"][0]["id"], json!(w.group));
    assert_eq!(second["next_cursor"], Value::Null);
}
