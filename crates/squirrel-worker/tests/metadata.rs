mod common;

use common::{Fixture, admin, job, queued_kinds, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::ContentHash;
use squirrel_db::media::{self, MediaState};
use squirrel_db::{assets, nodes::NodeRecord};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif, jpeg_with_exif};
use squirrel_worker::{metadata, process};
use time::macros::datetime;

async fn processed(fx: &Fixture, name: &str, bytes: &[u8]) -> NodeRecord {
    // One processed file per test: the first account is created here.
    let alice = admin(fx.pool()).await;
    let parent = root(fx.pool(), alice.id, "Photos").await;
    let node = fx.store_file(alice.id, parent, name, bytes).await;
    process::run(&fx.ctx, job("process_media", json!({"node_id": node.id})))
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    node
}

async fn extract(fx: &Fixture, sha: &str) {
    metadata::run(&fx.ctx, job("extract_metadata", json!({"sha256": sha})))
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn exif_settles_media_and_every_asset(pool: PgPool) {
    let fx = Fixture::new(pool);
    let jpeg = jpeg_sized_with_exif(
        64,
        32,
        &ExifSpec {
            date_time_original: Some("2024:07:14 18:30:05"),
            offset_time_original: Some("+02:00"),
            gps: Some(((48, 51, 2400), b'N', (2, 21, 300), b'E')),
            make: Some("Google"),
            model: Some("Pixel 8"),
            orientation: Some(6),
            ..ExifSpec::default()
        },
    );
    let node = processed(&fx, "holiday.jpg", &jpeg).await;
    let sha = node.blob_sha256.clone().unwrap();
    extract(&fx, &sha).await;

    let row = media::find(fx.pool(), &sha).await.unwrap().unwrap();
    assert_eq!(row.metadata_state(), MediaState::Ready);
    assert_eq!(row.exif_taken_at, Some(datetime!(2024-07-14 18:30:05)));
    assert_eq!(row.exif_offset_min, Some(120));
    assert!((row.latitude.unwrap() - 48.8566).abs() < 1e-3);
    assert_eq!(
        (row.camera_make.as_deref(), row.camera_model.as_deref()),
        (Some("Google"), Some("Pixel 8"))
    );
    assert_eq!((row.width, row.height), (Some(32), Some(64)), "rotated");
    let asset = assets::find(fx.pool(), node.id).await.unwrap().unwrap();
    assert_eq!(
        (
            asset.taken_at,
            asset.taken_offset_min,
            asset.taken_source.as_str()
        ),
        (datetime!(2024-07-14 18:30:05), Some(120), "exif")
    );
    assert_eq!(
        queued_kinds(fx.pool()).await,
        vec![("geocode".to_owned(), 30)]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn screenshots_named_so_take_the_name_date(pool: PgPool) {
    let fx = Fixture::new(pool);
    let jpeg = jpeg_with_exif(&ExifSpec::default());
    let node = processed(&fx, "Screenshot_2024-07-14-18-30-05.jpg", &jpeg).await;
    let sha = node.blob_sha256.clone().unwrap();
    extract(&fx, &sha).await;
    let asset = assets::find(fx.pool(), node.id).await.unwrap().unwrap();
    assert_eq!(
        (asset.taken_source.as_str(), asset.kind.as_str()),
        ("filename", "screenshot")
    );
    assert!(
        queued_kinds(fx.pool()).await.is_empty(),
        "no GPS, no geocode"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn bytes_that_are_no_image_give_empty_metadata(pool: PgPool) {
    let fx = Fixture::new(pool);
    let node = processed(&fx, "fake.jpg", b"definitely not a jpeg").await;
    let sha = node.blob_sha256.clone().unwrap();
    extract(&fx, &sha).await;
    let row = media::find(fx.pool(), &sha).await.unwrap().unwrap();
    assert_eq!(row.metadata_state(), MediaState::Ready);
    assert_eq!(
        (row.exif_taken_at, row.width, row.height),
        (None, None, None)
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn huge_tiffs_skip_exif(pool: PgPool) {
    let fx = Fixture::new(pool);
    let sha = "f".repeat(64);
    let hash = ContentHash::parse(&sha).unwrap();
    let path = fx.ctx.storage.blob_path(&hash);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::File::create(&path).unwrap();
    std::io::Write::write_all(&mut &file, b"II*\0").unwrap();
    file.set_len(65 * 1024 * 1024).unwrap();
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'image/tiff')")
        .bind(&sha)
        .bind(65_i64 * 1024 * 1024)
        .execute(fx.pool())
        .await
        .unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, squirrel_core::MediaKind::Image)
        .await
        .unwrap();
    extract(&fx, &sha).await;
    let row = media::find(fx.pool(), &sha).await.unwrap().unwrap();
    assert_eq!(row.metadata_state(), MediaState::Ready);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_collected_blob_is_nothing_to_do(pool: PgPool) {
    let fx = Fixture::new(pool);
    extract(&fx, &"e".repeat(64)).await;
    let bad = metadata::run(&fx.ctx, job("extract_metadata", json!({"sha256": "nope"}))).await;
    assert!(matches!(bad, Err(squirrel_jobs::JobError::Permanent(_))));
}
