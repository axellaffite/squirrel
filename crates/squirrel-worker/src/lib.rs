//! Media job handlers: glue between Postgres, the blob store and
//! `squirrel-media`. Untrusted decoding goes through [`exec::Executor`].

pub mod context;
pub mod exec;
mod facts;
pub mod jobs;
pub mod metadata;
pub mod process;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use squirrel_jobs::{JobContext, JobError, Registry};

pub use context::MediaContext;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const METADATA_TIMEOUT: Duration = Duration::from_secs(60);

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
}
