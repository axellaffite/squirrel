mod common;

use std::time::Duration;

use common::{Fixture, admin, job, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media::{self, MediaRecord, MediaState};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::render::THUMBS_VERSION;
use squirrel_media::testutil::{ExifSpec, ci_fixture, jpeg_sized_with_exif};
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
async fn renders_installs_and_records_the_thumbnail_only(pool: PgPool) {
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
    assert!(
        derived
            .variant_path(&hash, Variant::Thumb, THUMBS_VERSION)
            .is_file()
    );
    // Photo previews are rendered on request.
    assert!(
        !derived
            .variant_path(&hash, Variant::Preview, THUMBS_VERSION)
            .exists()
    );

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
async fn heic_is_unsupported_without_the_feature(pool: PgPool) {
    if squirrel_media::HEIC_SUPPORTED {
        return;
    }
    let fx = Fixture::new(pool);
    let heic = std::fs::read("../squirrel-media/tests/fixtures/exif.heic").unwrap();
    let sha = pending(&fx, "a.heic", &heic).await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.thumbs_state(), MediaState::Unsupported);
    assert_eq!(media.last_error.as_deref(), Some(thumbnails::NO_HEIC));
}

/// With `heic`: a real HEIC (`rotated.heic`, generated in CI) gets thumbnails, rotated once.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn heic_gets_thumbnails_with_the_feature(pool: PgPool) {
    if !squirrel_media::HEIC_SUPPORTED {
        return;
    }
    let Some(heic) = ci_fixture("../squirrel-media/tests/fixtures/rotated.heic") else {
        return;
    };
    let fx = Fixture::new(pool);
    let sha = pending(&fx, "rotated.heic", &heic).await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(
        media.thumbs_state(),
        MediaState::Ready,
        "{:?}",
        media.last_error
    );
    assert_eq!((media.width, media.height), (Some(32), Some(64)));
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

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_sandbox_timeout_leaves_no_scratch_directory(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let program = bin.path().join("fake-squirrel");
    std::fs::write(&program, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let fx = Fixture::with_executor(
        pool,
        Executor::Subprocess(SubprocessConfig {
            program,
            max_memory_mb: 64,
            max_cpu_secs: 1,
            timeout: Duration::from_millis(300),
        }),
    );
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(40, 30, &ExifSpec::default()),
    )
    .await;
    let result = thumbnails::run(&fx.ctx, thumbs_job(&sha)).await;
    assert!(matches!(result, Err(JobError::Retry(_))));
    let leftovers: Vec<_> = std::fs::read_dir(fx._dir.path().join("tmp"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("derived-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_sandbox_planting_a_symlink_gets_nothing_installed(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let program = bin.path().join("fake-squirrel");
    // Claims success after planting a link to a file it wants served.
    std::fs::write(
        &program,
        "#!/bin/sh\n\
         dir=$(cat | sed 's/.*\"out_dir\":\"\\([^\"]*\\)\".*/\\1/')\n\
         ln -s /etc/hosts \"$dir/thumb.webp\"\n\
         echo '{\"status\":\"image\",\"rendered\":{\"width\":1,\"height\":1,\"blurhash\":\"x\"}}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let fx = Fixture::with_executor(
        pool,
        Executor::Subprocess(SubprocessConfig {
            program,
            max_memory_mb: 64,
            max_cpu_secs: 1,
            timeout: Duration::from_secs(10),
        }),
    );
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(40, 30, &ExifSpec::default()),
    )
    .await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.thumbs_state(), MediaState::Failed);
    let hash = ContentHash::parse(&sha).unwrap();
    let path = fx
        .ctx
        .storage
        .derived()
        .variant_path(&hash, Variant::Thumb, THUMBS_VERSION);
    assert!(
        std::fs::symlink_metadata(path).is_err(),
        "nothing installed"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn thumbnails_of_a_blob_collected_while_rendering_are_removed(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let program = bin.path().join("fake-squirrel");
    // Renders slowly, giving the test time to collect the blob.
    std::fs::write(
        &program,
        "#!/bin/sh\n\
         dir=$(cat | sed 's/.*\"out_dir\":\"\\([^\"]*\\)\".*/\\1/')\n\
         sleep 1\n\
         printf webp > \"$dir/thumb.webp\"\n\
         echo '{\"status\":\"image\",\"rendered\":{\"width\":1,\"height\":1,\"blurhash\":\"x\"}}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let fx = Fixture::with_executor(
        pool,
        Executor::Subprocess(SubprocessConfig {
            program,
            max_memory_mb: 64,
            max_cpu_secs: 5,
            timeout: Duration::from_secs(10),
        }),
    );
    let sha = pending(
        &fx,
        "a.jpg",
        &jpeg_sized_with_exif(40, 30, &ExifSpec::default()),
    )
    .await;
    let ctx = fx.ctx.clone();
    let job_sha = sha.clone();
    let rendering = tokio::spawn(async move { thumbnails::run(&ctx, thumbs_job(&job_sha)).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Garbage collection: the node, then the blob row (media cascades).
    sqlx::query("DELETE FROM nodes WHERE blob_sha256 = $1")
        .bind(&sha)
        .execute(fx.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM blobs WHERE sha256 = $1")
        .bind(&sha)
        .execute(fx.pool())
        .await
        .unwrap();
    rendering.await.unwrap().unwrap();
    let hash = ContentHash::parse(&sha).unwrap();
    assert!(!fx.ctx.storage.derived().dir(&hash).exists());
}
