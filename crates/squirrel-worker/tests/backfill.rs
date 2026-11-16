mod common;

use common::{Fixture, admin, job, queued, root, zip_bytes};
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif, jpeg_with_exif, pdf_with_text};
use squirrel_worker::backfill;
use uuid::Uuid;

/// Runs one page and takes back the follow-up page it queued, if any.
async fn page(fx: &Fixture, payload: Value) -> Option<Value> {
    backfill::run_page(&fx.ctx, job("backfill", payload), 2)
        .await
        .unwrap();
    let next: Option<Value> =
        sqlx::query_scalar("DELETE FROM jobs WHERE kind = 'backfill' RETURNING payload")
            .fetch_optional(fx.pool())
            .await
            .unwrap();
    next
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pages_through_unprocessed_files_including_the_trash(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let photos = root(fx.pool(), alice.id, "Photos").await;
    let docs = root(fx.pool(), alice.id, "Documents").await;
    let jpeg = fx
        .store_file(
            alice.id,
            photos,
            "a.jpg",
            &jpeg_with_exif(&ExifSpec::default()),
        )
        .await;
    let pdf = fx
        .store_file(alice.id, docs, "a.pdf", &pdf_with_text("Hi"))
        .await;
    fx.store_file(alice.id, docs, "a.zip", &zip_bytes()).await;
    let trashed = fx
        .store_file(
            alice.id,
            photos,
            "b.jpg",
            &jpeg_sized_with_exif(8, 8, &ExifSpec::default()),
        )
        .await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(trashed.id)
        .execute(fx.pool())
        .await
        .unwrap();

    let next = page(&fx, json!({})).await.expect("a second page");
    let after: Uuid = serde_json::from_value(next["after"].clone()).unwrap();
    assert!(
        page(&fx, json!({ "after": after })).await.is_none(),
        "last page"
    );

    let mut expected = vec![jpeg.id, pdf.id, trashed.id];
    expected.sort();
    let jobs = queued(fx.pool()).await;
    let mut nodes: Vec<Uuid> = jobs
        .iter()
        .map(|(kind, priority, payload)| {
            assert_eq!((kind.as_str(), *priority), ("process_media", 50));
            serde_json::from_value(payload["node_id"].clone()).unwrap()
        })
        .collect();
    nodes.sort();
    assert_eq!(nodes, expected);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn processed_files_are_skipped_and_old_thumbnails_redone(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let photos = root(fx.pool(), alice.id, "Photos").await;
    let jpeg = fx
        .store_file(
            alice.id,
            photos,
            "a.jpg",
            &jpeg_with_exif(&ExifSpec::default()),
        )
        .await;
    squirrel_worker::process::run(&fx.ctx, job("process_media", json!({"node_id": jpeg.id})))
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE media SET thumbs_state = 'ready', thumbs_version = 0")
        .execute(fx.pool())
        .await
        .unwrap();

    assert!(page(&fx, json!({})).await.is_none());
    let jobs = queued(fx.pool()).await;
    assert_eq!(jobs.len(), 1, "{jobs:?}");
    assert_eq!((jobs[0].0.as_str(), jobs[0].1), ("thumbnails", 30));
    let sha = jpeg.blob_sha256.as_deref().unwrap();
    let expected = squirrel_worker::jobs::thumbnails(sha, true);
    assert_eq!(jobs[0].2, expected.payload);
    let key: Option<String> = sqlx::query_scalar("SELECT dedupe_key FROM jobs")
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(key, expected.dedupe_key);
}
