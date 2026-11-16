mod common;

use sqlx::PgPool;
use squirrel_db::device_assets::{self, SyncItem};
use squirrel_db::devices::{self, NewAccess, NewDevice};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn device(pool: &PgPool, owner: Uuid) -> Uuid {
    let refresh = Uuid::now_v7().as_bytes().to_vec();
    let access = Uuid::now_v7().as_bytes().to_vec();
    let now = OffsetDateTime::now_utc();
    devices::create(
        pool,
        &NewDevice {
            owner_id: owner,
            name: "Pixel",
            platform: "android",
            model: None,
            app_version: "0.1.0",
            refresh_hash: &refresh,
            refresh_expires_at: now + Duration::days(90),
            access: NewAccess {
                hash: &access,
                expires_at: now + Duration::minutes(15),
            },
        },
    )
    .await
    .unwrap()
    .id
}

fn sha(fill: char) -> String {
    fill.to_string().repeat(64)
}

fn item<'a>(id: &'a str, sha256: &'a str) -> SyncItem<'a> {
    SyncItem {
        device_asset_id: id,
        sha256,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_never_looks_at_other_users_content(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let bobs_files = common::root(&pool, bob.id, "Files").await;
    let mine = common::raw_file(&pool, alice.id, files.id, "mine.jpg", 'a', 10).await;
    let trashed = common::raw_file(&pool, alice.id, files.id, "old.jpg", 'b', 10).await;
    common::raw_trash(&pool, trashed).await;
    common::raw_file(&pool, bob.id, bobs_files.id, "bobs.jpg", 'c', 10).await;
    let phone = device(&pool, alice.id).await;

    let (a, b, c, d) = (sha('a'), sha('b'), sha('c'), sha('d'));
    let items = [
        item("images:1", &a),
        item("images:2", &b),
        item("images:3", &c),
        item("images:4", &d),
    ];
    let missing = device_assets::missing(&pool, phone, alice.id, &items)
        .await
        .unwrap();
    assert_eq!(missing, ["images:3", "images:4"]);

    // Found through the library: recorded with the node holding the content.
    let recorded = device_assets::list_for_device(&pool, phone).await.unwrap();
    let pairs: Vec<(&str, Option<Uuid>)> = recorded
        .iter()
        .map(|r| (r.device_asset_id.as_str(), r.node_id))
        .collect();
    assert_eq!(
        pairs,
        [("images:1", Some(mine)), ("images:2", Some(trashed))]
    );
    // A second check answers the same.
    let again = device_assets::missing(&pool, phone, alice.id, &items)
        .await
        .unwrap();
    assert_eq!(again, ["images:3", "images:4"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_recorded_asset_stays_present_even_once_deleted_on_the_server(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let phone = device(&pool, alice.id).await;
    let a = sha('a');
    let mut tx = pool.begin().await.unwrap();
    device_assets::record(&mut tx, phone, "images:1", &a, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let missing = device_assets::missing(&pool, phone, alice.id, &[item("images:1", &a)])
        .await
        .unwrap();
    assert!(missing.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_edited_asset_is_missing_again(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let phone = device(&pool, alice.id).await;
    let (old, new) = (sha('a'), sha('e'));
    let mut tx = pool.begin().await.unwrap();
    device_assets::record(&mut tx, phone, "images:1", &old, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let missing = device_assets::missing(&pool, phone, alice.id, &[item("images:1", &new)])
        .await
        .unwrap();
    assert_eq!(missing, ["images:1"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn record_upserts(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let node = common::raw_file(&pool, alice.id, files.id, "x.jpg", 'e', 1).await;
    let phone = device(&pool, alice.id).await;
    let mut tx = pool.begin().await.unwrap();
    device_assets::record(&mut tx, phone, "images:1", &sha('a'), None)
        .await
        .unwrap();
    device_assets::record(&mut tx, phone, "images:1", &sha('e'), Some(node))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let recorded = device_assets::list_for_device(&pool, phone).await.unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].sha256, sha('e'));
    assert_eq!(recorded[0].node_id, Some(node));

    // Deleting the node keeps the record; deleting the device drops it.
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(node)
        .execute(&pool)
        .await
        .unwrap();
    let recorded = device_assets::list_for_device(&pool, phone).await.unwrap();
    assert_eq!(recorded[0].node_id, None);
    sqlx::query("DELETE FROM devices")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        device_assets::list_for_device(&pool, phone)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_check_probes_indexes(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let phone = device(&pool, alice.id).await;
    let ids: Vec<String> = (0..500).map(|i| format!("images:{i}")).collect();
    let hashes: Vec<String> = (0..500).map(|i| format!("{i:064x}")).collect();
    let mut tx = pool.begin().await.unwrap();
    // A tiny table would be scanned sequentially: ask for the index plan.
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<String> = sqlx::query_scalar(&format!("EXPLAIN {}", device_assets::CHECK_SQL))
        .bind(phone)
        .bind(alice.id)
        .bind(&ids)
        .bind(&hashes)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let plan = plan.join("\n");
    assert!(plan.contains("device_assets_pkey"), "{plan}");
    assert!(plan.contains("nodes_owner_blob_idx"), "{plan}");
    assert!(plan.contains("deleted_hashes_pkey"), "{plan}");
    assert!(!plan.contains("Seq Scan on nodes"), "{plan}");
    assert!(plan.contains("Index Cond"), "{plan}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_schema_accepts_device_times_and_rejects_bad_shapes(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let node = common::raw_file(&pool, alice.id, files.id, "x.jpg", 'a', 1).await;
    sqlx::query("INSERT INTO media (sha256, kind) VALUES ($1, 'image') ON CONFLICT DO NOTHING")
        .bind(sha('a'))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         VALUES ($1, $2, $3, 'photo', '2024-07-14 18:30:05', 'device')",
    )
    .bind(node)
    .bind(alice.id)
    .bind(sha('a'))
    .execute(&pool)
    .await
    .unwrap();
    let offset_without_time =
        sqlx::query("UPDATE nodes SET client_taken_offset_min = 120 WHERE id = $1")
            .bind(node)
            .execute(&pool)
            .await;
    assert!(offset_without_time.is_err());
    sqlx::query(
        "UPDATE nodes SET client_taken_at = '2024-07-14 18:30:05', \
                          client_taken_offset_min = 120 WHERE id = $1",
    )
    .bind(node)
    .execute(&pool)
    .await
    .unwrap();
}

async fn tombstones(pool: &PgPool, owner: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT sha256 FROM deleted_hashes WHERE owner_id = $1 ORDER BY sha256")
        .bind(owner)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A file of `fill` content, recorded as `asset` by `phone`, then trashed.
async fn backed_up_and_trashed(
    pool: &PgPool,
    owner: Uuid,
    phone: Uuid,
    asset: &str,
    fill: char,
) -> Uuid {
    let files = common::root(pool, owner, "Files").await;
    let node = common::raw_file(pool, owner, files.id, &format!("{asset}.jpg"), fill, 1).await;
    let mut tx = pool.begin().await.unwrap();
    device_assets::record(&mut tx, phone, asset, &sha(fill), Some(node))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    common::raw_trash(pool, node).await;
    node
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_backed_up_content_for_good_leaves_a_tombstone(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let phone = device(&pool, alice.id).await;
    let backed_up = backed_up_and_trashed(&pool, alice.id, phone, "images:1", 'a').await;
    let files = common::root(&pool, alice.id, "Files").await;
    let web_only = common::raw_file(&pool, alice.id, files.id, "web.jpg", 'b', 1).await;
    common::raw_trash(&pool, web_only).await;
    // Bob's phone backing up the same content does not tombstone it for Alice.
    let bobs_phone = device(&pool, bob.id).await;
    let mut tx = pool.begin().await.unwrap();
    device_assets::record(&mut tx, bobs_phone, "images:9", &sha('b'), None)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    squirrel_db::trash::delete_forever(&pool, alice.id, backed_up)
        .await
        .unwrap();
    squirrel_db::trash::delete_forever(&pool, alice.id, web_only)
        .await
        .unwrap();
    assert_eq!(tombstones(&pool, alice.id).await, [sha('a')]);

    // The phone's own records go with the device; the tombstone stays.
    sqlx::query("DELETE FROM devices WHERE id = $1")
        .bind(phone)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(tombstones(&pool, alice.id).await, [sha('a')]);

    // A new install asks by hash: the deleted content is not missing, and is recorded.
    let reinstalled = device(&pool, alice.id).await;
    let (a, b) = (sha('a'), sha('b'));
    let missing = device_assets::missing(
        &pool,
        reinstalled,
        alice.id,
        &[item("images:1", &a), item("images:2", &b)],
    )
    .await
    .unwrap();
    assert_eq!(missing, ["images:2"]);
    let recorded = device_assets::list_for_device(&pool, reinstalled)
        .await
        .unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].device_asset_id, "images:1");
    assert_eq!(recorded[0].node_id, None);
    // Bob is unaffected by Alice's tombstone.
    let bobs_check = device_assets::missing(&pool, bobs_phone, bob.id, &[item("images:1", &a)])
        .await
        .unwrap();
    assert_eq!(bobs_check, ["images:1"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn emptying_the_trash_and_the_purge_tombstone_too(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let phone = device(&pool, alice.id).await;
    backed_up_and_trashed(&pool, alice.id, phone, "images:1", 'a').await;
    squirrel_db::trash::empty(&pool, alice.id).await.unwrap();
    assert_eq!(tombstones(&pool, alice.id).await, [sha('a')]);

    backed_up_and_trashed(&pool, alice.id, phone, "images:2", 'b').await;
    sqlx::query("UPDATE nodes SET deleted_at = now() - interval '40 days' WHERE trash_root = id")
        .execute(&pool)
        .await
        .unwrap();
    let cutoff = OffsetDateTime::now_utc() - Duration::days(30);
    squirrel_db::trash::purge_older_than(&pool, cutoff, 10)
        .await
        .unwrap();
    assert_eq!(tombstones(&pool, alice.id).await, [sha('a'), sha('b')]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn overlapping_checks_of_one_device_do_not_deadlock(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let ids: Vec<String> = (0..200).map(|i| format!("images:{i:03}")).collect();
    let hashes: Vec<String> = (0..200).map(|i| format!("{i:064x}")).collect();
    for (i, sha) in hashes.iter().enumerate() {
        sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, 1, 'image/jpeg')")
            .bind(sha)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, 1, 'image/jpeg')",
        )
        .bind(alice.id)
        .bind(files.id)
        .bind(format!("{i}.jpg"))
        .bind(sha)
        .execute(&pool)
        .await
        .unwrap();
    }
    let phone = device(&pool, alice.id).await;
    let forward: Vec<SyncItem<'_>> = ids.iter().zip(&hashes).map(|(i, s)| item(i, s)).collect();
    let backward: Vec<SyncItem<'_>> = forward.iter().rev().copied().collect();
    for _ in 0..5 {
        sqlx::query("DELETE FROM device_assets")
            .execute(&pool)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            device_assets::missing(&pool, phone, alice.id, &forward),
            device_assets::missing(&pool, phone, alice.id, &backward),
        );
        assert!(a.unwrap().is_empty());
        assert!(b.unwrap().is_empty());
    }
    assert_eq!(
        device_assets::list_for_device(&pool, phone)
            .await
            .unwrap()
            .len(),
        200
    );
}
