mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{Fixture, admin, job, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::media::{self, MediaState, Step};
use squirrel_jobs::{JobContext, JobError, priority};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
use squirrel_ml::CLIP_MODEL_ID;
use squirrel_ml::clip::labels::LABELS_VERSION;
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use squirrel_worker::context::MlContext;
use squirrel_worker::ml_client::{MlBackend, MlClient, MlClientError};
use squirrel_worker::{backfill, clip_job, label_bank, thumbnails};

/// A unit vector along axis `axis` of 512.
fn axis(axis: usize) -> Vec<f32> {
    (0..512)
        .map(|i| if i == axis { 1.0 } else { 0.0 })
        .collect()
}

/// A fake ML client: images embed to axis 0, texts to axis 1; counts calls.
struct Fake {
    images: AtomicUsize,
    texts: AtomicUsize,
    fail: bool,
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_embeddings_are_never_stored(pool: PgPool) {
    let mut nan = axis(0);
    nan[3] = f32::NAN;
    for bad in [vec![0.5; 3], nan, vec![f32::INFINITY; 512], vec![0.5; 512]] {
        let answer = bad.clone();
        let client = MlClient::new(MlBackend::Fake(Arc::new(move |_: &MlOp| {
            Ok(MlBody::Ok(MlOutput::Embedding {
                embedding: answer.clone(),
            }))
        })));
        let ml = Arc::new(MlContext::new(client, true, false));
        let fx = Fixture::with_ml(pool.clone(), ml.clone());
        sqlx::query("TRUNCATE users, blobs CASCADE")
            .execute(fx.pool())
            .await
            .unwrap();
        let sha = photo(&fx, true).await;
        let err = clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
            .await
            .unwrap_err();
        assert!(matches!(err, JobError::Retry(_)), "{err}");
        let stored = squirrel_db::clip::embedding(fx.pool(), &sha, CLIP_MODEL_ID)
            .await
            .unwrap();
        assert!(stored.is_none(), "{} values stored", bad.len());
        // On the last attempt the step ends `failed`, not pending forever.
        let mut last = clip_job_for(&sha);
        last.attempt = last.max_attempts;
        clip_job::run(&fx.ctx, &ml, last).await.unwrap_err();
        assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Failed);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_unsupported_preview_ends_the_step(pool: PgPool) {
    let client = MlClient::new(MlBackend::Fake(Arc::new(|_: &MlOp| {
        Ok(MlBody::Unsupported {
            reason: "cannot decode".into(),
        })
    })));
    let ml = Arc::new(MlContext::new(client, true, false));
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Unsupported);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_child_in_restart_backoff_reschedules_without_an_attempt(pool: PgPool) {
    let client = MlClient::new(MlBackend::Fake(Arc::new(|_: &MlOp| {
        Err(MlClientError::Backoff(std::time::Duration::from_secs(42)))
    })));
    let ml = Arc::new(MlContext::new(client, true, false));
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    let err = clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap_err();
    assert!(
        matches!(err, JobError::Reschedule(d) if d == std::time::Duration::from_secs(42)),
        "{err}"
    );
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Pending);
}

fn fake_ml(fail: bool) -> (Arc<Fake>, Arc<MlContext>) {
    let fake = Arc::new(Fake {
        images: AtomicUsize::new(0),
        texts: AtomicUsize::new(0),
        fail,
    });
    let seen = fake.clone();
    let client = MlClient::new(MlBackend::Fake(Arc::new(move |op: &MlOp| match op {
        MlOp::ClipImage { input } => {
            seen.images.fetch_add(1, Ordering::SeqCst);
            assert!(input.is_file(), "the preview exists: {}", input.display());
            if seen.fail {
                return Err(MlClientError::Crashed("boom".into()));
            }
            Ok(MlBody::Ok(MlOutput::Embedding { embedding: axis(0) }))
        }
        MlOp::ClipText { texts } => {
            seen.texts.fetch_add(texts.len(), Ordering::SeqCst);
            Ok(MlBody::Ok(MlOutput::Embeddings {
                embeddings: texts.iter().map(|_| axis(1)).collect(),
            }))
        }
        _ => Ok(MlBody::Ok(MlOutput::Pong {})),
    })));
    (fake, Arc::new(MlContext::new(client, true, false)))
}

/// A stored photo with its media row; thumbnails rendered when `thumbnailed`.
async fn photo(fx: &Fixture, thumbnailed: bool) -> String {
    let alice = admin(fx.pool()).await;
    let parent = root(fx.pool(), alice.id, "Photos").await;
    let bytes = jpeg_sized_with_exif(400, 300, &ExifSpec::default());
    let node = fx.store_file(alice.id, parent, "a.jpg", &bytes).await;
    let sha = node.blob_sha256.unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, MediaKind::Image)
        .await
        .unwrap();
    if thumbnailed {
        thumbnails::run(&fx.ctx, thumbs_job(&sha, priority::THUMBNAILS))
            .await
            .unwrap();
    }
    sha
}

fn thumbs_job(sha: &str, priority: i16) -> JobContext {
    JobContext {
        priority,
        ..job("thumbnails", json!({"sha256": sha}))
    }
}

fn clip_job_for(sha: &str) -> JobContext {
    job("clip_embed", json!({"sha256": sha}))
}

/// A two-label bank: "dog" along the images' axis, "beach" orthogonal.
async fn store_bank(pool: &PgPool) {
    let bank = vec![("beach".to_owned(), axis(1)), ("dog".to_owned(), axis(0))];
    squirrel_db::clip::store_label_embeddings(pool, CLIP_MODEL_ID, LABELS_VERSION, &bank)
        .await
        .unwrap();
}

async fn queued(pool: &PgPool, kind: &str) -> Vec<(String, i16)> {
    sqlx::query_as("SELECT dedupe_key, priority FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn clip_state(pool: &PgPool, sha: &str) -> MediaState {
    media::find(pool, sha).await.unwrap().unwrap().clip_state()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn fresh_and_backfilled_thumbnails_queue_clip_at_their_priority(pool: PgPool) {
    let fx = Fixture::new(pool);
    let sha = photo(&fx, true).await;
    assert_eq!(
        queued(fx.pool(), "clip_embed").await,
        [(format!("clip_embed:{sha}"), priority::CLIP)]
    );
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    // Re-rendered from backfill (an older thumbnails version).
    sqlx::query("UPDATE media SET thumbs_version = 0 WHERE sha256 = $1")
        .bind(&sha)
        .execute(fx.pool())
        .await
        .unwrap();
    let background = priority::background(priority::THUMBNAILS);
    thumbnails::run(&fx.ctx, thumbs_job(&sha, background))
        .await
        .unwrap();
    assert_eq!(
        queued(fx.pool(), "clip_embed").await,
        [(
            format!("clip_embed:{sha}"),
            priority::background(priority::CLIP)
        )]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_handler_stores_the_embedding_and_tags_once(pool: PgPool) {
    let (fake, ml) = fake_ml(false);
    let fx = Fixture::with_ml(pool, ml.clone());
    store_bank(fx.pool()).await;
    let sha = photo(&fx, true).await;
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    let stored = squirrel_db::clip::embedding(fx.pool(), &sha, CLIP_MODEL_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.tagged_labels_version, Some(LABELS_VERSION));
    let tags = squirrel_db::tags::tags_of(fx.pool(), &sha).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "dog");
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Ready);
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    assert_eq!(fake.images.load(Ordering::SeqCst), 1, "no second inference");
    assert_eq!(
        squirrel_db::tags::tags_of(fx.pool(), &sha).await.unwrap(),
        tags
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn without_a_label_bank_photos_get_an_embedding_but_no_tag(pool: PgPool) {
    let (_fake, ml) = fake_ml(false);
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    let stored = squirrel_db::clip::embedding(fx.pool(), &sha, CLIP_MODEL_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.tagged_labels_version, None);
    assert!(
        squirrel_db::tags::tags_of(fx.pool(), &sha)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Ready);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pending_thumbnails_reschedule_and_missing_ones_mark_unsupported(pool: PgPool) {
    let (fake, ml) = fake_ml(false);
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, false).await;
    let err = clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap_err();
    assert!(matches!(err, JobError::Reschedule(_)), "{err}");
    media::set_state(
        fx.pool(),
        &sha,
        Step::Thumbs,
        MediaState::Unsupported,
        Some("heic"),
    )
    .await
    .unwrap();
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Unsupported);
    assert_eq!(fake.images.load(Ordering::SeqCst), 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_failing_child_retries_then_marks_failed(pool: PgPool) {
    let (_fake, ml) = fake_ml(true);
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    let err = clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap_err();
    assert!(matches!(err, JobError::Retry(_)), "{err}");
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Pending);
    let mut last = clip_job_for(&sha);
    last.attempt = last.max_attempts;
    let err = clip_job::run(&fx.ctx, &ml, last).await.unwrap_err();
    assert!(matches!(err, JobError::Retry(_)), "{err}");
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Failed);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_queues_missing_embeddings_and_stale_tags(pool: PgPool) {
    let (_fake, ml) = fake_ml(false);
    let fx = Fixture::with_ml(pool, ml.clone());
    store_bank(fx.pool()).await;
    let sha = photo(&fx, true).await;
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    assert_eq!(
        queued(fx.pool(), "clip_embed").await,
        [(
            format!("clip_embed:{sha}"),
            priority::background(priority::CLIP)
        )]
    );
    // Embedded and tagged with an older labels version: queued to re-tag.
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    assert!(
        queued(fx.pool(), "clip_embed").await.is_empty(),
        "up to date"
    );
    sqlx::query("UPDATE clip_embeddings SET tagged_labels_version = 0")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    assert_eq!(queued(fx.pool(), "clip_embed").await.len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_retries_any_image_without_an_embedding(pool: PgPool) {
    let (_fake, ml) = fake_ml(false);
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    media::set_state(
        fx.pool(),
        &sha,
        Step::Clip,
        MediaState::Unsupported,
        Some("bad preview"),
    )
    .await
    .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    assert_eq!(queued(fx.pool(), "clip_embed").await.len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_label_bank_stored_later_is_picked_up(pool: PgPool) {
    let (_fake, ml) = fake_ml(false);
    assert!(ml.label_bank(&pool).await.unwrap().is_empty());
    store_bank(&pool).await;
    assert_eq!(
        ml.label_bank(&pool).await.unwrap().len(),
        2,
        "empty was not cached"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_worker_without_clip_queues_no_clip_work(pool: PgPool) {
    let fx = Fixture::new(pool);
    photo(&fx, true).await;
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    assert!(queued(fx.pool(), "clip_embed").await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_label_bank_is_computed_once(pool: PgPool) {
    let (fake, ml) = fake_ml(false);
    assert!(
        label_bank::ensure(&pool, &ml.client, CLIP_MODEL_ID)
            .await
            .unwrap()
    );
    let prompts = fake.texts.load(Ordering::SeqCst);
    assert!(prompts > 1000, "{prompts}");
    let bank = squirrel_db::clip::label_embeddings(&pool, CLIP_MODEL_ID, LABELS_VERSION)
        .await
        .unwrap();
    assert_eq!(bank.len(), squirrel_ml::clip::labels::bank().len());
    assert!(
        !label_bank::ensure(&pool, &ml.client, CLIP_MODEL_ID)
            .await
            .unwrap()
    );
    assert_eq!(
        fake.texts.load(Ordering::SeqCst),
        prompts,
        "no inference the second time"
    );
}

/// With the real runtime and models: a generated picture end to end, in-process.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_real_photo_gets_a_real_embedding(pool: PgPool) {
    let Some((dylib, models)) =
        squirrel_ml::testutil::test_ml_for(&[squirrel_ml::ModelGroup::ClipImage])
    else {
        return;
    };
    let engine = squirrel_ml::serve::Engine::load(&squirrel_ml::serve::EngineConfig {
        groups: vec![squirrel_ml::ModelGroup::ClipImage],
        models,
        ort_dylib: Some(dylib),
        threads: 2,
    })
    .unwrap();
    let client = MlClient::new(MlBackend::InProcess(Arc::new(std::sync::Mutex::new(
        engine,
    ))));
    let ml = Arc::new(MlContext::new(client, true, false));
    let fx = Fixture::with_ml(pool, ml.clone());
    let sha = photo(&fx, true).await;
    clip_job::run(&fx.ctx, &ml, clip_job_for(&sha))
        .await
        .unwrap();
    let stored = squirrel_db::clip::embedding(fx.pool(), &sha, CLIP_MODEL_ID)
        .await
        .unwrap()
        .unwrap();
    let norm: f32 = stored.values.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-2, "{norm}");
    assert_eq!(clip_state(fx.pool(), &sha).await, MediaState::Ready);
}
