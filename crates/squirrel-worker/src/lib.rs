//! Media job handlers: glue between Postgres, the blob store and
//! `squirrel-media`. Untrusted decoding goes through [`exec::Executor`].

pub mod backfill;
pub mod context;
pub mod exec;
mod facts;
pub mod geocode;
pub mod jobs;
pub mod metadata;
mod outcome;
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
}
