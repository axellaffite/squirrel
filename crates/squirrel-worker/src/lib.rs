//! Media job handlers: glue between Postgres, the blob store and
//! `squirrel-media`. Untrusted decoding goes through [`exec::Executor`].

pub mod backfill;
pub mod clip_job;
pub mod cluster_job;
pub mod context;
pub mod exec;
pub mod face_backfill;
pub mod faces_job;
mod facts;
pub mod geocode;
pub mod jobs;
pub mod label_bank;
pub mod metadata;
/// The `ml-exec` supervisor (in `squirrel-ml`, shared with the server).
pub use squirrel_ml::client as ml_client;
mod outcome;
pub mod pdf_job;
pub mod process;
pub mod text;
pub mod thumbnails;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use squirrel_jobs::{JobContext, JobError, Registry};

pub use context::MediaContext;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const METADATA_TIMEOUT: Duration = Duration::from_secs(60);
const BACKFILL_TIMEOUT: Duration = Duration::from_secs(60);
/// Includes loading the dataset (~16 MB) on first use.
const GEOCODE_TIMEOUT: Duration = Duration::from_secs(60);
/// The sandbox's wall clock is 150 s and the lease 300 s: the sandbox gives up first.
const THUMBNAILS_TIMEOUT: Duration = Duration::from_secs(180);
/// One inference (≈ 0.1–0.2 s on a mini PC) plus a cold `ml-exec` start
/// (hashing and loading the models, a few seconds).
const CLIP_TIMEOUT: Duration = Duration::from_secs(90);
/// YuNet on a 640 px copy plus SFace per face (tens of milliseconds), and a
/// cold `ml-exec` start.
const FACES_TIMEOUT: Duration = Duration::from_secs(120);
/// About 1 400 prompts in batches of 32 (≈ 30 s on a fast CPU, minutes on a
/// mini PC).
const LABEL_BANK_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// SQL only: exact neighbour scans over one owner's faces.
const CLUSTER_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// PDF text runs in the sandbox too.
const TEXT_TIMEOUT: Duration = Duration::from_secs(180);

/// Registers `kind` with a handler that borrows the shared context.
fn register<F, Fut>(
    registry: &mut Registry,
    ctx: &Arc<MediaContext>,
    kind: &'static str,
    timeout: Duration,
    handler: F,
) where
    F: Fn(Arc<MediaContext>, JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), JobError>> + Send + 'static,
{
    let ctx = ctx.clone();
    registry.register(kind, timeout, move |job| handler(ctx.clone(), job));
}

/// Adds every media handler to `registry`.
pub fn register_all(registry: &mut Registry, ctx: Arc<MediaContext>) {
    register(
        registry,
        &ctx,
        squirrel_jobs::kinds::PROCESS_MEDIA,
        PROCESS_TIMEOUT,
        |ctx, job| async move { process::run(&ctx, job).await },
    );
    register(
        registry,
        &ctx,
        jobs::EXTRACT_METADATA,
        METADATA_TIMEOUT,
        |ctx, job| async move { metadata::run(&ctx, job).await },
    );
    register(
        registry,
        &ctx,
        jobs::THUMBNAILS,
        THUMBNAILS_TIMEOUT,
        |ctx, job| async move { thumbnails::run(&ctx, job).await },
    );
    register(
        registry,
        &ctx,
        jobs::EXTRACT_TEXT,
        TEXT_TIMEOUT,
        |ctx, job| async move { text::run(&ctx, job).await },
    );
    register(
        registry,
        &ctx,
        jobs::GEOCODE,
        GEOCODE_TIMEOUT,
        |ctx, job| async move { geocode::run(&ctx, job).await },
    );
    register(
        registry,
        &ctx,
        jobs::BACKFILL,
        BACKFILL_TIMEOUT,
        |ctx, job| async move { backfill::run(&ctx, job).await },
    );
    register_ml(registry, &ctx);
}

/// The ML kinds this worker's models allow (none without `ctx.ml`).
fn register_ml(registry: &mut Registry, ctx: &Arc<MediaContext>) {
    let Some(ml) = ctx.ml.clone() else {
        return;
    };
    if ml.clip {
        let clip = ml.clone();
        register(
            registry,
            ctx,
            jobs::CLIP_EMBED,
            CLIP_TIMEOUT,
            move |ctx, job| {
                let ml = clip.clone();
                async move { clip_job::run(&ctx, &ml, job).await }
            },
        );
    }
    if ml.faces {
        register(
            registry,
            ctx,
            jobs::FACE_BACKFILL,
            BACKFILL_TIMEOUT,
            |ctx, job| async move { face_backfill::run(&ctx, job).await },
        );
        register(
            registry,
            ctx,
            jobs::CLUSTER_FACES,
            CLUSTER_TIMEOUT,
            |ctx, job| async move { cluster_job::run(&ctx, job).await },
        );
        let faces = ml.clone();
        register(
            registry,
            ctx,
            jobs::FACES,
            FACES_TIMEOUT,
            move |ctx, job| {
                let ml = faces.clone();
                async move { faces_job::run(&ctx, &ml, job).await }
            },
        );
    }
    if ml.clip && ml.text.is_some() {
        register(
            registry,
            ctx,
            jobs::LABEL_BANK,
            LABEL_BANK_TIMEOUT,
            move |ctx, job| {
                let ml = ml.clone();
                async move { label_bank::run(&ctx, &ml, job).await }
            },
        );
    }
}
