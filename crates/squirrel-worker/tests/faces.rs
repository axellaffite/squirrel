mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{Fixture, admin, invited, job, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::faces;
use squirrel_db::media::{self, MediaState};
use squirrel_db::ml_settings;
use squirrel_jobs::{JobContext, priority};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
use squirrel_ml::faces::FaceFound;
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use squirrel_worker::context::MlContext;
use squirrel_worker::ml_client::{MlBackend, MlClient};
use squirrel_worker::{backfill, faces_job, thumbnails};
use uuid::Uuid;

fn found(x: f32) -> FaceFound {
    let mut embedding = vec![0.0; 128];
    embedding[0] = 1.0;
    FaceFound {
        bbox: [x, 0.1, 0.2, 0.3],
        landmarks: [x + 0.05; 10],
        score: 0.9,
        embedding,
    }
}

/// A fake child: `faces` answers `answer`; counts the inferences.
fn fake(answer: Vec<FaceFound>) -> (Arc<AtomicUsize>, Arc<MlContext>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let client = MlClient::new(MlBackend::Fake(Arc::new(move |op: &MlOp| {
        if let MlOp::Faces { input } = op {
            assert!(input.is_file());
            seen.fetch_add(1, Ordering::SeqCst);
            return Ok(MlBody::Ok(MlOutput::Faces {
                faces: answer.clone(),
            }));
        }
        Ok(MlBody::Ok(MlOutput::Pong {}))
    })));
    (calls, Arc::new(MlContext::new(client, false, true)))
}

/// A file node of `bytes` for `owner`, with its timeline entry.
async fn copy(fx: &Fixture, owner: Uuid, bytes: &[u8]) -> String {
    let parent = root(fx.pool(), owner, "Photos").await;
    let node = fx.store_file(owner, parent, "p.jpg", bytes).await;
    let sha = node.blob_sha256.unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, MediaKind::Image)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         VALUES ($1, $2, $3, 'photo', now(), 'exif')",
    )
    .bind(node.id)
    .bind(owner)
    .bind(&sha)
    .execute(fx.pool())
    .await
    .unwrap();
    sha
}

fn faces_job_for(sha: &str) -> JobContext {
    job("faces", json!({ "sha256": sha }))
}

fn thumbs_job(sha: &str) -> JobContext {
    JobContext {
        priority: priority::THUMBNAILS,
        ..job("thumbnails", json!({ "sha256": sha }))
    }
}

struct Pair {
    fx: Fixture,
    alice: Uuid,
    bob: Uuid,
    sha: String,
}

/// Alice and Bob own copies of one rendered photo; Bob has faces off.
async fn pair(ml: Arc<MlContext>, pool: PgPool) -> Pair {
    let fx = Fixture::with_ml(pool, ml);
    let alice = admin(fx.pool()).await;
    let bob = invited(fx.pool(), &alice, "bob").await;
    let bytes = jpeg_sized_with_exif(400, 300, &ExifSpec::default());
    let sha = copy(&fx, alice.id, &bytes).await;
    copy(&fx, bob.id, &bytes).await;
    ml_settings::set_faces_enabled(fx.pool(), bob.id, false)
        .await
        .unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    Pair {
        fx,
        alice: alice.id,
        bob: bob.id,
        sha,
    }
}

async fn cluster_jobs(pool: &PgPool) -> Vec<(String, bool)> {
    sqlx::query_as(
        "SELECT dedupe_key, run_at > now() + interval '9 minutes' FROM jobs \
          WHERE kind = 'cluster_faces' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn faces_are_stored_only_for_owners_with_faces_on(pool: PgPool) {
    let (calls, ml) = fake(vec![found(0.1), found(0.6)]);
    let p = pair(ml.clone(), pool).await;
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'faces'")
        .fetch_one(p.fx.pool())
        .await
        .unwrap();
    assert_eq!(
        queued, 1,
        "thumbnails queue faces when some owner wants them"
    );
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    let alice = faces::faces_of(p.fx.pool(), p.alice, &p.sha).await.unwrap();
    assert_eq!(alice.len(), 2);
    assert_eq!(alice[0].box_x, 0.1);
    assert!(
        faces::faces_of(p.fx.pool(), p.bob, &p.sha)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        faces::scan_count(p.fx.pool(), p.bob, &p.sha).await.unwrap(),
        None
    );
    assert_eq!(
        cluster_jobs(p.fx.pool()).await,
        [(format!("cluster_faces:{}", p.alice), true)]
    );
    let row = media::find(p.fx.pool(), &p.sha).await.unwrap().unwrap();
    assert_eq!(row.faces_state(), MediaState::Ready);
    // Again: every owner is scanned, so no inference and nothing new.
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        faces::faces_of(p.fx.pool(), p.alice, &p.sha)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_without_faces_is_scanned_once(pool: PgPool) {
    let (_calls, ml) = fake(Vec::new());
    let p = pair(ml.clone(), pool).await;
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    assert_eq!(
        faces::scan_count(p.fx.pool(), p.alice, &p.sha)
            .await
            .unwrap(),
        Some(0)
    );
    assert!(
        cluster_jobs(p.fx.pool()).await.is_empty(),
        "nothing to cluster"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_nobody_wants_faces_for_is_not_analysed(pool: PgPool) {
    let (calls, ml) = fake(vec![found(0.1)]);
    let p = pair(ml.clone(), pool).await;
    ml_settings::set_faces_enabled(p.fx.pool(), p.alice, false)
        .await
        .unwrap();
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!faces::wanted(p.fx.pool(), &p.sha).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_faces_are_never_stored(pool: PgPool) {
    let mut outside = found(0.1);
    outside.bbox = [0.9, 0.1, 0.5, 0.3];
    let mut short = found(0.1);
    short.embedding.truncate(64);
    for bad in [outside, short] {
        let (_calls, ml) = fake(vec![bad]);
        let p = pair(ml.clone(), pool.clone()).await;
        assert!(
            faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
                .await
                .is_err()
        );
        assert_eq!(
            faces::scan_count(p.fx.pool(), p.alice, &p.sha)
                .await
                .unwrap(),
            None
        );
        sqlx::query("TRUNCATE users, blobs, jobs CASCADE")
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_queues_faces_for_owners_without_a_scan(pool: PgPool) {
    let (_calls, ml) = fake(vec![found(0.1)]);
    let p = pair(ml.clone(), pool).await;
    sqlx::query("DELETE FROM jobs")
        .execute(p.fx.pool())
        .await
        .unwrap();
    backfill::run_page(&p.fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    let faces_jobs: Vec<String> =
        sqlx::query_scalar("SELECT dedupe_key FROM jobs WHERE kind = 'faces'")
            .fetch_all(p.fx.pool())
            .await
            .unwrap();
    assert_eq!(faces_jobs, [format!("faces:{}", p.sha)]);
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(p.fx.pool())
        .await
        .unwrap();
    backfill::run_page(&p.fx.ctx, job("backfill", json!({})), 500)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'faces'")
        .fetch_one(p.fx.pool())
        .await
        .unwrap();
    assert_eq!(left, 0, "Alice is scanned and Bob has faces off");
}

/// With the real models: a portrait's single face, relative to its preview.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_real_portrait_gets_one_face(pool: PgPool) {
    let Some((dylib, models)) =
        squirrel_ml::testutil::test_ml_for(&[squirrel_ml::ModelGroup::Faces])
    else {
        return;
    };
    let engine = squirrel_ml::serve::Engine::load(&squirrel_ml::serve::EngineConfig {
        groups: vec![squirrel_ml::ModelGroup::Faces],
        models,
        ort_dylib: Some(dylib),
        threads: 2,
    })
    .unwrap();
    let client = MlClient::new(MlBackend::InProcess(Arc::new(std::sync::Mutex::new(
        engine,
    ))));
    let ml = Arc::new(MlContext::new(client, false, true));
    let fx = Fixture::with_ml(pool, ml.clone());
    let alice = admin(fx.pool()).await;
    let portrait = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../squirrel-ml/tests/fixtures/faces/harris-2021.jpg"
    ))
    .unwrap();
    let sha = copy(&fx, alice.id, &portrait).await;
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    faces_job::run(&fx.ctx, &ml, faces_job_for(&sha))
        .await
        .unwrap();
    let found = faces::faces_of(fx.pool(), alice.id, &sha).await.unwrap();
    assert_eq!(found.len(), 1);
    let f = &found[0];
    assert!(f.box_x > 0.2 && f.box_x + f.box_w < 0.8, "{f:?}");
    assert!(f.box_y > 0.05 && f.box_y + f.box_h < 0.7, "{f:?}");
}

/// Bob turns faces on later: his backfill queues the photo, whose run writes
/// his rows without touching Alice's.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn turning_faces_on_later_backfills_only_that_user(pool: PgPool) {
    let (calls, ml) = fake(vec![found(0.1)]);
    let p = pair(ml.clone(), pool).await;
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    let alice_before = faces::faces_of(p.fx.pool(), p.alice, &p.sha).await.unwrap();
    ml_settings::set_faces_enabled(p.fx.pool(), p.bob, true)
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(p.fx.pool())
        .await
        .unwrap();
    let backfill = job("face_backfill", json!({ "owner_id": p.bob, "after": null }));
    squirrel_worker::face_backfill::run_page(&p.fx.ctx, backfill, 500)
        .await
        .unwrap();
    let queued: Vec<(String, i16)> =
        sqlx::query_as("SELECT dedupe_key, priority FROM jobs WHERE kind = 'faces'")
            .fetch_all(p.fx.pool())
            .await
            .unwrap();
    assert_eq!(
        queued,
        [(
            format!("faces:{}", p.sha),
            priority::background(priority::FACES)
        )]
    );
    faces_job::run(&p.fx.ctx, &ml, faces_job_for(&p.sha))
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        faces::faces_of(p.fx.pool(), p.bob, &p.sha)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        faces::faces_of(p.fx.pool(), p.alice, &p.sha).await.unwrap(),
        alice_before
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_backfill_for_a_user_with_faces_off_queues_nothing(pool: PgPool) {
    let (_calls, ml) = fake(vec![found(0.1)]);
    let p = pair(ml.clone(), pool).await;
    sqlx::query("DELETE FROM jobs")
        .execute(p.fx.pool())
        .await
        .unwrap();
    let backfill = job("face_backfill", json!({ "owner_id": p.bob, "after": null }));
    squirrel_worker::face_backfill::run_page(&p.fx.ctx, backfill, 500)
        .await
        .unwrap();
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(p.fx.pool())
        .await
        .unwrap();
    assert_eq!(queued, 0);
}
