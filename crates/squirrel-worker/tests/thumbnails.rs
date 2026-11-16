mod common;

use std::time::Duration;

use common::{Fixture, admin, job, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media::{self, MediaRecord, MediaState};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::render::THUMBS_VERSION;
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
use squirrel_storage::Variant;
use squirrel_worker::exec::{Executor, SubprocessConfig};
use squirrel_worker::thumbnails;

/// Stores `bytes` as `name` for a new admin and creates its pending media row.
async fn pending(fx: &Fixture, name: &str, bytes: &[u8]) -> String {
    let alice = admin(fx.pool()).await;
    let parent = root(fx.pool(), alice.id, "Photos").await;
    let node = fx.store_file(alice.id, parent, name, bytes).await;
    let sha = node.blob_sha256.unwrap();
    let kind = MediaKind::for_mime(node.mime.as_deref().unwrap()).unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, kind).await.unwrap();
    sha
}

fn thumbs_job(sha: &str) -> JobContext {
    job("thumbnails", json!({"sha256": sha}))
}

async fn row(fx: &Fixture, sha: &str) -> MediaRecord {
    media::find(fx.pool(), sha).await.unwrap().unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renders_installs_and_records_both_variants(pool: PgPool) {
    let fx = Fixture::new(pool);
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(400, 300, &ExifSpec::default()),
    )
    .await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();

    let media = row(&fx, &sha).await;
    assert_eq!(media.thumbs_state(), MediaState::Ready);
    assert_eq!(media.thumbs_version, THUMBS_VERSION);
    assert_eq!((media.width, media.height), (Some(400), Some(300)));
    assert!(media.blurhash.is_some());
    let hash = ContentHash::parse(&sha).unwrap();
    let derived = fx.ctx.storage.derived();
    for variant in Variant::ALL {
        assert!(
            derived
                .variant_path(&hash, variant, THUMBS_VERSION)
                .is_file()
        );
    }

    // Ready at the current version: nothing is redone.
    let path = derived.variant_path(&hash, Variant::Thumb, THUMBS_VERSION);
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn garbage_fails_without_a_retry(pool: PgPool) {
    let fx = Fixture::new(pool);
    let sha = pending(&fx, "fake.jpg", b"not an image at all").await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.thumbs_state(), MediaState::Failed);
    assert!(media.last_error.is_some());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn heic_depends_on_the_build(pool: PgPool) {
    let fx = Fixture::new(pool);
    let heic = std::fs::read("../squirrel-media/tests/fixtures/exif.heic").unwrap();
    let sha = pending(&fx, "a.heic", &heic).await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    if squirrel_media::HEIC_SUPPORTED {
        assert_ne!(media.thumbs_state(), MediaState::Pending);
    } else {
        assert_eq!(media.thumbs_state(), MediaState::Unsupported);
        assert_eq!(media.last_error.as_deref(), Some(thumbnails::NO_HEIC));
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn blobs_over_the_size_cap_are_unsupported(pool: PgPool) {
    let fx = Fixture::new(pool);
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(40, 30, &ExifSpec::default()),
    )
    .await;
    sqlx::query("UPDATE blobs SET size = $2 WHERE sha256 = $1")
        .bind(&sha)
        .bind(513_i64 * 1024 * 1024)
        .execute(fx.pool())
        .await
        .unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    assert_eq!(row(&fx, &sha).await.thumbs_state(), MediaState::Unsupported);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sandbox_failures_are_retried_then_recorded(pool: PgPool) {
    let fx = Fixture::with_executor(
        pool,
        Executor::Subprocess(SubprocessConfig {
            program: "/nonexistent/squirrel".into(),
            max_memory_mb: 64,
            max_cpu_secs: 1,
            timeout: Duration::from_secs(5),
        }),
    );
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(40, 30, &ExifSpec::default()),
    )
    .await;
    let first = thumbnails::run(&fx.ctx, thumbs_job(&sha)).await;
    assert!(matches!(first, Err(JobError::Retry(_))));
    assert_eq!(row(&fx, &sha).await.thumbs_state(), MediaState::Pending);

    let mut last = thumbs_job(&sha);
    last.attempt = last.max_attempts;
    let last = thumbnails::run(&fx.ctx, last).await;
    assert!(matches!(last, Err(JobError::Retry(_))));
    assert_eq!(row(&fx, &sha).await.thumbs_state(), MediaState::Failed);
}
