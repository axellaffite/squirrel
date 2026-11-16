mod common;

use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::maintenance;
use squirrel_core::ContentHash;
use time::Duration;
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_sessions_and_orphan_files_are_removed(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let expired: Value = files::start_upload(&browser, &me, parent, "old.bin", b"old")
        .await
        .json();
    let live: Value = files::start_upload(&browser, &me, parent, "new.bin", b"new")
        .await
        .json();
    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(
        expired["upload_id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap(),
    )
    .execute(&pool)
    .await
    .unwrap();
    let orphan = Uuid::now_v7();
    instance.state.storage.create_upload(orphan, 1).unwrap();

    let cleanup = maintenance::purge_expired_uploads(&instance.state)
        .await
        .unwrap();
    assert_eq!((cleanup.expired_sessions, cleanup.files_removed), (1, 2));

    let uploads_dir = instance.data_dir().join("tmp/uploads");
    let left: Vec<String> = std::fs::read_dir(uploads_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, [live["upload_id"].as_str().unwrap()]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn old_trash_is_purged_and_unreferenced_blobs_collected(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        trash_retention: Duration::ZERO,
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let gone = files::upload(&browser, &me, parent, "gone.txt", b"gone").await;
    files::upload(&browser, &me, parent, "kept.txt", b"kept").await;
    browser
        .delete(&format!("/api/nodes/{}", gone["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);

    assert_eq!(
        maintenance::purge_old_trash(&instance.state).await.unwrap(),
        1
    );
    assert_eq!(
        maintenance::collect_garbage(&instance.state).await.unwrap(),
        1
    );

    let store = &instance.state.storage;
    assert!(!store.contains(&ContentHash::parse(&files::sha256_hex(b"gone")).unwrap()));
    assert!(store.contains(&ContentHash::parse(&files::sha256_hex(b"kept")).unwrap()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn gc_waits_for_the_grace_period(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"recent").await;
    let id = node["id"].as_str().unwrap();
    for path in [format!("/api/nodes/{id}"), format!("/api/trash/{id}")] {
        browser
            .delete(&path)
            .add_header(common::CSRF, common::csrf(&me))
            .await
            .assert_status(axum::http::StatusCode::NO_CONTENT);
    }
    assert_eq!(
        maintenance::collect_garbage(&instance.state).await.unwrap(),
        0
    );
    assert!(
        instance
            .state
            .storage
            .contains(&ContentHash::parse(&files::sha256_hex(b"recent")).unwrap())
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn one_undeletable_orphan_does_not_stop_the_sweep(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let uploads_dir = instance.data_dir().join("tmp/uploads");
    // A directory under an upload id cannot be removed as a file.
    let stuck = Uuid::now_v7();
    std::fs::create_dir(uploads_dir.join(stuck.to_string())).unwrap();
    let orphan = Uuid::now_v7();
    instance.state.storage.create_upload(orphan, 1).unwrap();

    let cleanup = maintenance::purge_expired_uploads(&instance.state)
        .await
        .unwrap();
    assert_eq!((cleanup.files_removed, cleanup.files_failed), (1, 1));
    assert!(!uploads_dir.join(orphan.to_string()).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn blob_files_without_a_row_are_swept_after_the_grace_period(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "kept.txt", b"kept").await;
    let store = &instance.state.storage;
    let (orphan, _) = store.put_reader(&b"crashed before its row"[..]).unwrap();

    let sweep = maintenance::sweep_orphan_blobs(&instance.state)
        .await
        .unwrap();
    assert_eq!((sweep.removed, sweep.failed), (1, 0));
    assert!(!store.contains(&orphan));
    assert!(store.contains(&ContentHash::parse(&files::sha256_hex(b"kept")).unwrap()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn recent_orphan_blob_files_are_kept(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let store = &instance.state.storage;
    let (fresh, _) = store.put_reader(&b"just written"[..]).unwrap();
    let sweep = maintenance::sweep_orphan_blobs(&instance.state)
        .await
        .unwrap();
    assert!(sweep.is_empty());
    assert!(store.contains(&fresh));
}

fn no_grace() -> squirrel_api::ApiConfig {
    squirrel_api::ApiConfig {
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_sweep_deletes_nothing_when_the_catalogue_is_empty(pool: PgPool) {
    // A data directory served against an empty (wrong or fresh) database.
    let instance = common::Instance::with_config(pool, no_grace());
    let store = &instance.state.storage;
    let (blob, _) = store.put_reader(&b"uploaded long ago"[..]).unwrap();

    let sweep = maintenance::sweep_orphan_blobs(&instance.state)
        .await
        .unwrap();
    assert_eq!((sweep.removed, sweep.held_back), (0, 1));
    assert!(store.contains(&blob));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_sweep_deletes_nothing_when_too_many_files_look_orphaned(pool: PgPool) {
    let instance = common::Instance::with_config(pool, no_grace());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "kept.txt", b"kept").await;
    let store = &instance.state.storage;
    let orphans: Vec<ContentHash> = (0..=maintenance::SWEEP_BRAKE_MIN_ORPHANS)
        .map(|i| {
            store
                .put_reader(format!("orphan {i}").as_bytes())
                .unwrap()
                .0
        })
        .collect();

    let sweep = maintenance::sweep_orphan_blobs(&instance.state)
        .await
        .unwrap();
    assert_eq!((sweep.removed, sweep.held_back), (0, orphans.len()));
    assert!(orphans.iter().all(|hash| store.contains(hash)));
}

/// Installs a thumbnail for `hash` as the worker would.
fn install_thumbnail(store: &squirrel_storage::BlobStore, hash: &ContentHash) {
    let derived = store.derived();
    let scratch = derived.scratch().unwrap();
    std::fs::write(scratch.path().join("thumb.webp"), b"webp").unwrap();
    derived.install(hash, 1, &scratch).unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn collecting_a_blob_removes_its_derived_files(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        trash_retention: Duration::ZERO,
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let gone = files::upload(&browser, &me, parent, "gone.jpg", b"gone").await;
    let store = &instance.state.storage;
    let hash = ContentHash::parse(&files::sha256_hex(b"gone")).unwrap();
    install_thumbnail(store, &hash);
    browser
        .delete(&format!("/api/nodes/{}", gone["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
    maintenance::purge_old_trash(&instance.state).await.unwrap();

    assert_eq!(
        maintenance::collect_garbage(&instance.state).await.unwrap(),
        1
    );
    assert!(!store.derived().dir(&hash).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn derived_files_without_a_blob_row_are_swept(pool: PgPool) {
    let instance = common::Instance::with_config(pool, no_grace());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "kept.jpg", b"kept").await;
    let store = &instance.state.storage;
    let kept = ContentHash::parse(&files::sha256_hex(b"kept")).unwrap();
    let orphan = ContentHash::parse(&files::sha256_hex(b"collected long ago")).unwrap();
    install_thumbnail(store, &kept);
    install_thumbnail(store, &orphan);
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for hash in [&kept, &orphan] {
        std::fs::File::open(store.derived().dir(hash))
            .unwrap()
            .set_modified(past)
            .unwrap();
    }

    let sweep = maintenance::sweep_orphan_derived(&instance.state)
        .await
        .unwrap();
    assert_eq!((sweep.removed, sweep.failed, sweep.held_back), (1, 0, 0));
    assert!(!store.derived().dir(&orphan).exists());
    assert!(store.derived().dir(&kept).is_dir());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn recent_derived_files_are_kept(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let store = &instance.state.storage;
    let fresh = ContentHash::parse(&files::sha256_hex(b"being processed")).unwrap();
    install_thumbnail(store, &fresh);
    let sweep = maintenance::sweep_orphan_derived(&instance.state)
        .await
        .unwrap();
    assert!(sweep.is_empty());
    assert!(store.derived().dir(&fresh).is_dir());
}
