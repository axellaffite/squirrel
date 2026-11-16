mod common;

use common::{Fixture, admin, invited, job, queued_kinds, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_db::media::{self, MediaState};
use squirrel_db::{assets, users::UserRecord};
use squirrel_jobs::JobError;
use squirrel_media::testutil::{ExifSpec, jpeg_with_exif, pdf_with_text};
use squirrel_worker::process;
use uuid::Uuid;

async fn process(fx: &Fixture, node: Uuid, background: bool) -> Result<(), JobError> {
    process::run(
        &fx.ctx,
        job(
            "process_media",
            json!({"node_id": node, "background": background}),
        ),
    )
    .await
}

async fn photos(fx: &Fixture, user: &UserRecord) -> Uuid {
    root(fx.pool(), user.id, "Photos").await
}

fn kinds(pairs: &[(&str, i16)]) -> Vec<(String, i16)> {
    pairs.iter().map(|(k, p)| ((*k).to_owned(), *p)).collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_image_gets_media_an_asset_and_its_jobs(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let parent = photos(&fx, &alice).await;
    let jpeg = jpeg_with_exif(&ExifSpec::default());
    let node = fx.store_file(alice.id, parent, "holiday.jpg", &jpeg).await;
    process(&fx, node.id, false).await.unwrap();

    let sha = node.blob_sha256.clone().unwrap();
    let row = media::find(fx.pool(), &sha).await.unwrap().unwrap();
    assert_eq!(row.kind().as_str(), "image");
    assert_eq!(row.metadata_state(), MediaState::Pending);
    let asset = assets::find(fx.pool(), node.id).await.unwrap().unwrap();
    assert_eq!(
        (asset.taken_source.as_str(), asset.kind.as_str()),
        ("upload", "photo")
    );
    assert_eq!(
        queued_kinds(fx.pool()).await,
        kinds(&[("extract_metadata", 70), ("thumbnails", 80)])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn file_name_dates_are_used_before_metadata_is_known(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let parent = photos(&fx, &alice).await;
    let jpeg = jpeg_with_exif(&ExifSpec::default());
    let node = fx
        .store_file(alice.id, parent, "IMG_20240714_183005.jpg", &jpeg)
        .await;
    process(&fx, node.id, false).await.unwrap();
    let asset = assets::find(fx.pool(), node.id).await.unwrap().unwrap();
    assert_eq!(asset.taken_source, "filename");
    assert_eq!(asset.taken_at, time::macros::datetime!(2024-07-14 18:30:05));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_work_runs_at_background_priority(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let parent = photos(&fx, &alice).await;
    let node = fx
        .store_file(
            alice.id,
            parent,
            "a.jpg",
            &jpeg_with_exif(&ExifSpec::default()),
        )
        .await;
    process(&fx, node.id, true).await.unwrap();
    assert_eq!(
        queued_kinds(fx.pool()).await,
        kinds(&[("extract_metadata", 20), ("thumbnails", 30)])
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_same_bytes_elsewhere_share_media_and_jobs(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let bob = invited(fx.pool(), &alice, "bob").await;
    let jpeg = jpeg_with_exif(&ExifSpec {
        date_time_original: Some("2023:01:02 03:04:05"),
        ..ExifSpec::default()
    });
    let mine = fx
        .store_file(alice.id, photos(&fx, &alice).await, "a.jpg", &jpeg)
        .await;
    process(&fx, mine.id, false).await.unwrap();
    let sha = mine.blob_sha256.clone().unwrap();
    // Metadata extracted meanwhile (Task 15's handler, simulated).
    media::set_metadata(
        fx.pool(),
        &sha,
        &media::MetadataUpdate {
            exif_taken_at: Some(time::macros::datetime!(2023-01-02 03:04:05)),
            ..media::MetadataUpdate::default()
        },
    )
    .await
    .unwrap();
    let theirs = fx
        .store_file(bob.id, photos(&fx, &bob).await, "b.jpg", &jpeg)
        .await;
    process(&fx, theirs.id, false).await.unwrap();

    let asset = assets::find(fx.pool(), theirs.id).await.unwrap().unwrap();
    assert_eq!(asset.owner_id, bob.id);
    assert_eq!(
        (asset.taken_source.as_str(), asset.taken_at),
        ("exif", time::macros::datetime!(2023-01-02 03:04:05))
    );
    assert_eq!(
        queued_kinds(fx.pool()).await,
        kinds(&[("extract_metadata", 70), ("thumbnails", 80)]),
        "no new jobs: the blob's work is already queued (dedupe)"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn documents_get_media_but_no_asset(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let docs = root(fx.pool(), alice.id, "Documents").await;
    let pdf = fx
        .store_file(alice.id, docs, "a.pdf", &pdf_with_text("Hello"))
        .await;
    process(&fx, pdf.id, false).await.unwrap();
    let row = media::find(fx.pool(), pdf.blob_sha256.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.kind().as_str(), "pdf");
    assert!(assets::find(fx.pool(), pdf.id).await.unwrap().is_none());
    assert_eq!(
        queued_kinds(fx.pool()).await,
        kinds(&[("extract_text", 40), ("thumbnails", 80)])
    );
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();

    let txt = fx.store_file(alice.id, docs, "notes.txt", b"hello").await;
    process(&fx, txt.id, false).await.unwrap();
    assert_eq!(
        queued_kinds(fx.pool()).await,
        kinds(&[("extract_text", 40)])
    );
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();

    let zip = fx
        .store_file(alice.id, docs, "a.zip", &common::zip_bytes())
        .await;
    process(&fx, zip.id, false).await.unwrap();
    assert!(queued_kinds(fx.pool()).await.is_empty());
    assert!(
        media::find(fx.pool(), zip.blob_sha256.as_deref().unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleted_nodes_and_bad_payloads(pool: PgPool) {
    let fx = Fixture::new(pool);
    process(&fx, Uuid::now_v7(), false).await.unwrap();
    assert!(queued_kinds(fx.pool()).await.is_empty());
    let bad = process::run(&fx.ctx, job("process_media", json!({"node": 1}))).await;
    assert!(matches!(bad, Err(JobError::Permanent(_))));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_late_process_media_keeps_the_exif_time(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let parent = photos(&fx, &alice).await;
    let jpeg = jpeg_with_exif(&ExifSpec {
        date_time_original: Some("2023:01:02 03:04:05"),
        ..ExifSpec::default()
    });
    let node = fx.store_file(alice.id, parent, "a.jpg", &jpeg).await;
    process(&fx, node.id, false).await.unwrap();
    let sha = node.blob_sha256.clone().unwrap();
    squirrel_worker::metadata::run(&fx.ctx, job("extract_metadata", json!({"sha256": sha})))
        .await
        .unwrap();
    // A process_media that read the media row before metadata was ready.
    sqlx::query("UPDATE media SET metadata_state = 'pending'")
        .execute(fx.pool())
        .await
        .unwrap();
    process(&fx, node.id, false).await.unwrap();
    let asset = assets::find(fx.pool(), node.id).await.unwrap().unwrap();
    assert_eq!(
        (asset.taken_source.as_str(), asset.taken_at),
        ("exif", time::macros::datetime!(2023-01-02 03:04:05))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn process_media_waits_for_the_media_row_lock(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let parent = photos(&fx, &alice).await;
    let jpeg = jpeg_with_exif(&ExifSpec::default());
    let first = fx.store_file(alice.id, parent, "a.jpg", &jpeg).await;
    process(&fx, first.id, false).await.unwrap();
    let second = fx.store_file(alice.id, parent, "b.jpg", &jpeg).await;

    // `extract_metadata` holding the row while it settles the blob's entries.
    let mut settling = fx.pool().begin().await.unwrap();
    media::lock(&mut settling, first.blob_sha256.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let ctx = fx.ctx.clone();
    let late = tokio::spawn(async move {
        process::run(&ctx, job("process_media", json!({"node_id": second.id}))).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!late.is_finished(), "process_media must wait for the lock");
    settling.commit().await.unwrap();
    late.await.unwrap().unwrap();
}
