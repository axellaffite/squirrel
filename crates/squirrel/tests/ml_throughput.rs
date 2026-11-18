//! Manual measurement (ignored): the backfill of N fresh photos through the
//! real pipeline (sandboxed thumbnails, `ml-exec` child, CLIP tags), as the
//! worker runs it. Needs the runtime and models:
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo test --release -p squirrel \
//!       --test ml_throughput -- --ignored --nocapture

use std::ffi::OsString;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::PgPool;
use squirrel_jobs::{Registry, Worker, WorkerConfig};
use squirrel_media::geo_data::GeoSource;
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
use squirrel_media::thumbs::DecodeLimits;
use squirrel_storage::BlobStore;
use squirrel_worker::context::{MediaContext, MlContext};
use squirrel_worker::exec::{Executor, SubprocessConfig};
use squirrel_worker::geocode::GeoCache;
use squirrel_worker::ml_client::{MlBackend, MlChildConfig, MlClient};
use squirrel_worker::{clip_job, jobs, label_bank};
use uuid::Uuid;

const PHOTOS: u32 = 40;

async fn store_photos(pool: &PgPool, storage: &BlobStore) {
    let owner: Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, username, display_name, password_hash, role) \
         VALUES (gen_random_uuid(), 'bench', 'bench', 'x', 'admin') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let root: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, name, kind) \
         VALUES (gen_random_uuid(), $1, 'Photos', 'folder') RETURNING id",
    )
    .bind(owner)
    .fetch_one(pool)
    .await
    .unwrap();
    for n in 0..PHOTOS {
        // Distinct sizes: distinct blobs (no deduplication).
        let bytes = jpeg_sized_with_exif(1600 + n, 1200, &ExifSpec::default());
        let (hash, size) = storage.put_reader(Cursor::new(&bytes)).unwrap();
        let size = i64::try_from(size).unwrap();
        sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'image/jpeg')")
            .bind(hash.as_str())
            .bind(size)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
             VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, $5, 'image/jpeg')",
        )
        .bind(owner)
        .bind(root)
        .bind(format!("{n:03}.jpg"))
        .bind(hash.as_str())
        .bind(size)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[ignore = "manual measurement"]
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_throughput(pool: PgPool) {
    let Some((dylib, models)) = squirrel_ml::testutil::test_ml() else {
        return;
    };
    let program = std::path::PathBuf::from(env!("CARGO_BIN_EXE_squirrel"));
    let child = |groups: &str| MlChildConfig {
        program: program.clone(),
        args: [
            "--groups",
            groups,
            "--models-dir",
            models.path().to_str().unwrap(),
            "--ort-dylib",
            dylib.to_str().unwrap(),
            "--threads",
            "2",
        ]
        .iter()
        .map(OsString::from)
        .collect(),
        ready_timeout: Duration::from_secs(120),
        request_timeout: Duration::from_secs(60),
        max_requests: 1000,
        idle: Duration::from_secs(600),
    };
    let started = Instant::now();
    let text = MlClient::new(MlBackend::Subprocess(child("clip-text")));
    label_bank::ensure(&pool, &text, squirrel_ml::CLIP_MODEL_ID)
        .await
        .unwrap();
    text.stop().await;
    println!("label bank computed in {:.1?}", started.elapsed());

    let dir = tempfile::tempdir().unwrap();
    let storage = BlobStore::open(dir.path()).unwrap();
    store_photos(&pool, &storage).await;
    let ml = Arc::new(MlContext::new(
        MlClient::new(MlBackend::Subprocess(child("clip-image"))),
        true,
        false,
    ));
    let ctx = Arc::new(MediaContext {
        pool: pool.clone(),
        storage,
        executor: Executor::Subprocess(SubprocessConfig {
            program: program.clone(),
            max_memory_mb: 2048,
            max_cpu_secs: 120,
            timeout: Duration::from_secs(150),
        }),
        pdfium_dir: None,
        decode_limits: DecodeLimits::default(),
        geo: GeoCache::new(GeoSource::Disabled),
        ml: Some(ml.clone()),
    });
    let mut registry = Registry::new();
    squirrel_worker::register_all(&mut registry, ctx.clone());
    // The same handler, timed.
    let durations = Arc::new(Mutex::new(Vec::new()));
    let (timed_ctx, timed_ml, timed) = (ctx.clone(), ml.clone(), durations.clone());
    registry.register(jobs::CLIP_EMBED, Duration::from_secs(90), move |job| {
        let (ctx, ml, timed) = (timed_ctx.clone(), timed_ml.clone(), timed.clone());
        async move {
            let start = Instant::now();
            let result = clip_job::run(&ctx, &ml, job).await;
            timed.lock().unwrap().push(start.elapsed());
            result
        }
    });
    let config = WorkerConfig {
        concurrency: 2,
        lease: Duration::from_secs(300),
        poll_interval: Duration::from_millis(200),
        shutdown_grace: Duration::from_secs(5),
        name: "bench".into(),
        ml_kinds: jobs::ML_KINDS.to_vec(),
        ml_concurrency: 1,
    };
    let mut conn = pool.acquire().await.unwrap();
    squirrel_jobs::enqueue(&mut conn, &jobs::backfill(None))
        .await
        .unwrap();
    drop(conn);
    let (stop, stop_rx) = tokio::sync::watch::channel(false);
    let started = Instant::now();
    let worker = tokio::spawn(
        Worker::new(pool.clone(), registry, config)
            .unwrap()
            .run(stop_rx),
    );
    let mut first_clip = None;
    loop {
        let (ready, thumbs): (i64, i64) = sqlx::query_as(
            "SELECT count(*) FILTER (WHERE clip_state <> 'pending'), \
                    count(*) FILTER (WHERE thumbs_state = 'ready') FROM media",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if first_clip.is_none() && ready > 0 {
            first_clip = Some(started.elapsed());
        }
        if ready == i64::from(PHOTOS) {
            println!(
                "{PHOTOS} photos: thumbnails {thumbs}, all CLIP done in {:.1?} \
                 (first CLIP result after {:.1?})",
                started.elapsed(),
                first_clip.unwrap()
            );
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(600), "too slow");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    stop.send(true).unwrap();
    worker.await.unwrap().unwrap();
    let mut clip = durations.lock().unwrap().clone();
    clip.sort_unstable();
    let total: Duration = clip.iter().sum();
    println!(
        "clip_embed per photo (handler wall time, {} runs): median {:.1?}, p90 {:.1?}, \
         max {:.1?} (includes the cold child start), mean {:.1?}",
        clip.len(),
        clip[clip.len() / 2],
        clip[clip.len() * 9 / 10],
        clip[clip.len() - 1],
        total / u32::try_from(clip.len()).unwrap()
    );
    let tags: Vec<(String, i64)> =
        sqlx::query_as("SELECT tag, count(*) FROM asset_tags GROUP BY 1 ORDER BY 2 DESC LIMIT 5")
            .fetch_all(&pool)
            .await
            .unwrap();
    println!("top tags: {tags:?}");
}
