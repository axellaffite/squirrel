mod common;

use common::{admin, invited, photo, photo_of_blob, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::media::MediaState;
use squirrel_db::photo_filter::PhotoFilter;
use squirrel_db::timeline::{self, Filter, MonthCount, TimelineCursor};
use time::PrimitiveDateTime;
use time::macros::datetime;
use uuid::Uuid;

async fn names(pool: &PgPool, owner: Uuid, filter: Filter) -> Vec<String> {
    timeline::page(pool, owner, filter, &PhotoFilter::default(), None, 100)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pages_newest_first_without_gaps_or_repeats(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let times: [PrimitiveDateTime; 7] = [
        datetime!(2024-07-14 18:30:05),
        datetime!(2024-07-14 18:30:05),
        datetime!(2024-07-01 00:00),
        datetime!(2024-06-30 23:59:59),
        datetime!(2023-01-01 12:00),
        datetime!(2025-02-02 02:02),
        datetime!(2024-08-01 00:00),
    ];
    for (i, at) in times.iter().enumerate() {
        photo(&pool, alice.id, photos, &format!("p{i}.jpg"), *at).await;
    }
    let mut seen = Vec::new();
    let mut after: Option<TimelineCursor> = None;
    loop {
        let page = timeline::page(
            &pool,
            alice.id,
            Filter::All,
            &PhotoFilter::default(),
            after.as_ref(),
            3,
        )
        .await
        .unwrap();
        assert!(page.items.len() <= 3);
        seen.extend(page.items.iter().map(|a| (a.taken_at, a.node_id)));
        if !page.has_more {
            break;
        }
        after = Some(page.items.last().unwrap().cursor());
    }
    assert_eq!(seen.len(), 7);
    let mut sorted = seen.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(seen, sorted, "newest first, ties by node id descending");
    let first = timeline::page(
        &pool,
        alice.id,
        Filter::All,
        &PhotoFilter::default(),
        None,
        3,
    )
    .await
    .unwrap();
    assert!(first.has_more);
    assert_eq!(first.items[0].taken_at, datetime!(2025-02-02 02:02));
    assert_eq!(first.items[0].thumbs_state(), MediaState::Ready);
    assert_eq!(first.items[0].blurhash.as_deref(), Some("LEHV6nWB2yk8"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_month_filter_keeps_its_bounds(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    photo(
        &pool,
        alice.id,
        photos,
        "june.jpg",
        datetime!(2024-06-30 23:59:59.999),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "july1.jpg",
        datetime!(2024-07-01 00:00),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "july31.jpg",
        datetime!(2024-07-31 23:59:59),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "august.jpg",
        datetime!(2024-08-01 00:00),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "dec.jpg",
        datetime!(2024-12-31 23:00),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "jan.jpg",
        datetime!(2025-01-01 00:00),
    )
    .await;
    assert_eq!(
        names(&pool, alice.id, Filter::Month(2024, 7)).await,
        ["july31.jpg", "july1.jpg"]
    );
    assert_eq!(
        names(&pool, alice.id, Filter::Month(2024, 12)).await,
        ["dec.jpg"]
    );
    assert!(
        timeline::page(
            &pool,
            alice.id,
            Filter::Month(2024, 13),
            &PhotoFilter::default(),
            None,
            10
        )
        .await
        .is_err()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_owners_live_photos_appear(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let alice_photos = root(&pool, alice.id, "Photos").await.id;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let at = datetime!(2024-07-14 12:00);
    let sha = "ab".repeat(32);
    let shared = photo_of_blob(&pool, alice.id, alice_photos, "mine.jpg", &sha, at).await;
    photo_of_blob(&pool, bob.id, bob_photos, "bobs-copy.jpg", &sha, at).await;
    let trashed = photo(&pool, alice.id, alice_photos, "trashed.jpg", at).await;
    raw_trash(&pool, trashed).await;

    assert_eq!(names(&pool, alice.id, Filter::All).await, ["mine.jpg"]);
    assert_eq!(names(&pool, bob.id, Filter::All).await, ["bobs-copy.jpg"]);
    assert!(
        timeline::detail(&pool, bob.id, shared)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        timeline::detail(&pool, alice.id, trashed)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        timeline::months(&pool, alice.id, &PhotoFilter::default())
            .await
            .unwrap(),
        [MonthCount {
            month: "2024-07".into(),
            count: 1
        }]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn months_count_newest_first(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    photo(
        &pool,
        alice.id,
        photos,
        "a.jpg",
        datetime!(2024-06-02 10:00),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "b.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    photo(
        &pool,
        alice.id,
        photos,
        "c.jpg",
        datetime!(2024-07-31 23:59),
    )
    .await;
    let gone = photo(
        &pool,
        alice.id,
        photos,
        "d.jpg",
        datetime!(2024-08-01 10:00),
    )
    .await;
    raw_trash(&pool, gone).await;
    let months = timeline::months(&pool, alice.id, &PhotoFilter::default())
        .await
        .unwrap();
    let months: Vec<(&str, i64)> = months.iter().map(|m| (m.month.as_str(), m.count)).collect();
    assert_eq!(months, [("2024-07", 2), ("2024-06", 1)]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn detail_joins_node_media_and_asset(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let id = photo(
        &pool,
        alice.id,
        photos,
        "eiffel.jpg",
        datetime!(2024-07-14 18:30),
    )
    .await;
    sqlx::query(
        "UPDATE media SET latitude = 48.8584, longitude = 2.2945, camera_make = 'Canon', \
                place_name = 'Paris', place_country_code = 'FR' \
          WHERE sha256 = (SELECT sha256 FROM assets WHERE node_id = $1)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let detail = timeline::detail(&pool, alice.id, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.summary.name, "eiffel.jpg");
    assert_eq!(detail.summary.size, 10);
    assert_eq!(detail.summary.mime.as_deref(), Some("image/jpeg"));
    assert_eq!(detail.summary.kind, "photo");
    assert_eq!(detail.parent_id, Some(photos));
    assert_eq!(detail.latitude, Some(48.8584));
    assert_eq!(detail.camera_make.as_deref(), Some("Canon"));
    assert_eq!(detail.place_name.as_deref(), Some("Paris"));
    assert!(
        timeline::detail(&pool, alice.id, Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_keyset_is_an_index_condition(pool: PgPool) {
    let alice = admin(&pool).await;
    let cursor = TimelineCursor {
        taken_at: datetime!(2024-07-14 18:30),
        node_id: Uuid::now_v7(),
    };
    let mut tx = pool.begin().await.unwrap();
    // A tiny table would be scanned sequentially: ask for the index plan.
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let tagged = PhotoFilter {
        tag: Some("dog".into()),
        country: Some("FR".into()),
        ..PhotoFilter::default()
    };
    let cases = [
        (Filter::All, PhotoFilter::default()),
        (Filter::Month(2024, 7), PhotoFilter::default()),
        (Filter::All, tagged),
    ];
    for (filter, photos) in cases {
        let plan = timeline::explain_page(&mut tx, alice.id, filter, &photos, Some(&cursor), 50)
            .await
            .unwrap();
        assert!(plan.contains("assets_timeline_idx"), "{plan}");
        let cond = plan
            .lines()
            .find(|l| l.contains("Index Cond"))
            .unwrap_or_else(|| panic!("{plan}"));
        assert!(
            cond.contains("taken_at") && cond.contains("node_id"),
            "{plan}"
        );
    }
}
