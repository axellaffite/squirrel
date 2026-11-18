//! Executes claimed jobs: one claim fills the free slots; LISTEN/NOTIFY wakes
//! the loop, a poll interval catches delayed retries and expired leases.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::{Semaphore, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

use crate::backoff::{jitter, retry_delay};
use crate::job::{JobContext, JobError};
use crate::queue::{self, CHANNEL, ClaimedJob, Finished};

type JobFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;
type HandlerFn = Arc<dyn Fn(JobContext) -> JobFuture + Send + Sync>;
/// Tests: `true` makes the next heartbeat fail as a database error would.
type HeartbeatFault = Arc<dyn Fn() -> bool + Send + Sync>;

/// Heartbeats never come closer together than this, whatever the lease.
const MIN_HEARTBEAT: Duration = Duration::from_millis(100);

struct Handler {
    run: HandlerFn,
    timeout: Duration,
}

/// Job kinds this worker executes, each with its handler and time limit.
#[derive(Default)]
pub struct Registry {
    handlers: HashMap<&'static str, Handler>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// `timeout` bounds one run; it may exceed the lease, which heartbeats
    /// extend while the handler runs. It must not be zero ([`Worker::new`]).
    pub fn register<F, Fut>(&mut self, kind: &'static str, timeout: Duration, handler: F)
    where
        F: Fn(JobContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobError>> + Send + 'static,
    {
        let run: HandlerFn = Arc::new(move |ctx| Box::pin(handler(ctx)));
        self.handlers.insert(kind, Handler { run, timeout });
    }

    /// Keeps only the kinds for which `keep` is true.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.handlers.retain(|kind, _| keep(kind));
    }

    /// Registered kinds, sorted.
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut kinds: Vec<_> = self.handlers.keys().copied().collect();
        kinds.sort_unstable();
        kinds
    }
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Jobs run at once (at least 1).
    pub concurrency: usize,
    pub lease: Duration,
    pub poll_interval: Duration,
    /// After a stop request, running jobs get this long before being released.
    pub shutdown_grace: Duration,
    /// Shown to admins as `claimed_by`, e.g. "minipc:4242".
    pub name: String,
    /// Kinds that run model inference: at most `ml_concurrency` of them run at
    /// once, so the other slots stay free for fresh uploads.
    pub ml_kinds: Vec<&'static str>,
    pub ml_concurrency: usize,
}

type InFlight = Arc<Mutex<HashMap<i64, ClaimedJob>>>;

fn lock(in_flight: &InFlight) -> MutexGuard<'_, HashMap<i64, ClaimedJob>> {
    // Never held across a panic point; recover rather than cascade if it were.
    in_flight.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Worker {
    pool: PgPool,
    registry: Arc<Registry>,
    config: WorkerConfig,
    /// Claimed and not yet finished: released on shutdown.
    in_flight: InFlight,
    heartbeat_fault: Option<HeartbeatFault>,
}

impl Worker {
    /// Refuses a configuration heartbeats cannot keep alive: a lease shorter
    /// than three heartbeats, or a handler with a zero timeout.
    pub fn new(pool: PgPool, registry: Registry, config: WorkerConfig) -> anyhow::Result<Self> {
        if config.lease < MIN_HEARTBEAT * 3 {
            anyhow::bail!(
                "the lease ({:?}) must be at least three heartbeats ({:?})",
                config.lease,
                MIN_HEARTBEAT * 3
            );
        }
        if let Some((kind, _)) = registry.handlers.iter().find(|(_, h)| h.timeout.is_zero()) {
            anyhow::bail!("the {kind} handler has a zero timeout");
        }
        Ok(Self {
            pool,
            registry: Arc::new(registry),
            config,
            in_flight: Arc::default(),
            heartbeat_fault: None,
        })
    }

    /// Tests: heartbeats fail (as if the database were unreachable) while
    /// `fault` returns `true`.
    #[doc(hidden)]
    pub fn with_heartbeat_fault(self, fault: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            heartbeat_fault: Some(Arc::new(fault)),
            ..self
        }
    }

    /// Runs until `stop` turns true (or its sender is dropped), then drains
    /// (see [`WorkerConfig::shutdown_grace`]).
    pub async fn run(self, mut stop: watch::Receiver<bool>) -> anyhow::Result<()> {
        let kinds = ClaimKinds::new(self.registry.kinds(), &self.config.ml_kinds);
        let ml_running = Arc::new(AtomicUsize::new(0));
        let mut wakeups = Wakeups::default();
        wakeups.connect(&self.pool, self.config.poll_interval).await;
        let slots = Arc::new(Semaphore::new(self.config.concurrency.max(1)));
        let mut running = JoinSet::new();
        tracing::info!(
            worker = %self.config.name,
            kinds = ?kinds.all,
            concurrency = self.config.concurrency,
            ml_concurrency = self.config.ml_concurrency,
            "worker started"
        );
        while !*stop.borrow() {
            while running.try_join_next().is_some() {}
            if let Err(err) = self.fill(&kinds, &slots, &ml_running, &mut running).await {
                tracing::error!(error = %err, "claiming jobs failed");
            }
            if wakeups.should_reconnect() {
                wakeups.connect(&self.pool, self.config.poll_interval).await;
            }
            let listening = wakeups.is_listening();
            tokio::select! {
                note = wakeups.recv(), if listening => {
                    if let Err(err) = note {
                        tracing::warn!(error = %err, "job notifications interrupted; polling");
                        wakeups.pause(self.config.poll_interval);
                    }
                }
                () = tokio::time::sleep(self.config.poll_interval) => {}
                Some(_) = running.join_next(), if !running.is_empty() => {}
                changed = stop.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
            }
        }
        self.drain(running).await;
        Ok(())
    }

    /// Reaps expired leases, then claims as many jobs as there are free slots,
    /// never more ML jobs than [`WorkerConfig::ml_concurrency`] allows.
    async fn fill(
        &self,
        kinds: &ClaimKinds,
        slots: &Arc<Semaphore>,
        ml_running: &Arc<AtomicUsize>,
        running: &mut JoinSet<()>,
    ) -> anyhow::Result<()> {
        let reaped = queue::reap_expired(&self.pool).await?;
        if reaped > 0 {
            tracing::warn!(reaped, "jobs with expired leases were requeued");
        }
        loop {
            let free = slots.available_permits();
            let free_ml = self
                .config
                .ml_concurrency
                .saturating_sub(ml_running.load(Ordering::SeqCst));
            let Some((claimable, limit)) = kinds.plan(free, free_ml) else {
                return Ok(());
            };
            // Taken before the claim: the lease cannot have started earlier.
            let claimed_at = Instant::now();
            let jobs = queue::claim(
                &self.pool,
                claimable,
                i64::try_from(limit)?,
                self.config.lease,
                &self.config.name,
            )
            .await?;
            let claimed = jobs.len();
            for job in jobs {
                let permit = slots.clone().acquire_owned().await?;
                let ml_slot = kinds
                    .is_ml(&job.kind)
                    .then(|| MlSlot::take(ml_running.clone()));
                lock(&self.in_flight).insert(job.id, job.clone());
                let pool = self.pool.clone();
                let registry = self.registry.clone();
                let config = self.config.clone();
                let in_flight = self.in_flight.clone();
                let fault = self.heartbeat_fault.clone();
                running.spawn(async move {
                    let _permit = permit;
                    let _ml_slot = ml_slot;
                    let run = Run {
                        pool: &pool,
                        config: &config,
                        job: &job,
                        fault: fault.as_ref(),
                    };
                    execute(&registry, run, claimed_at).await;
                    lock(&in_flight).remove(&job.id);
                });
            }
            if claimed < limit {
                return Ok(());
            }
        }
    }

    async fn drain(&self, mut running: JoinSet<()>) {
        let grace = tokio::time::sleep(self.config.shutdown_grace);
        tokio::pin!(grace);
        loop {
            tokio::select! {
                next = running.join_next() => if next.is_none() { break },
                () = &mut grace => {
                    running.abort_all();
                    while running.join_next().await.is_some() {}
                    break;
                }
            }
        }
        let unfinished: Vec<ClaimedJob> = lock(&self.in_flight).drain().map(|(_, j)| j).collect();
        for job in unfinished {
            match queue::release(&self.pool, &job).await {
                Ok(_) => {
                    tracing::info!(job_id = job.id, kind = %job.kind, "released an unfinished job")
                }
                Err(err) => tracing::error!(
                    job_id = job.id,
                    error = %err,
                    "could not release a job; its lease will expire"
                ),
            }
        }
        tracing::info!(worker = %self.config.name, "worker stopped");
    }
}

/// The registered kinds, split for claims that must not take an ML slot.
#[derive(Debug)]
struct ClaimKinds {
    all: Vec<&'static str>,
    ml: Vec<&'static str>,
    not_ml: Vec<&'static str>,
}

impl ClaimKinds {
    fn new(all: Vec<&'static str>, ml_kinds: &[&'static str]) -> Self {
        let (ml, not_ml) = all.iter().partition(|kind| ml_kinds.contains(kind));
        Self { all, ml, not_ml }
    }

    fn is_ml(&self, kind: &str) -> bool {
        self.ml.contains(&kind)
    }

    /// What to claim with `free` slots of which `free_ml` may run ML jobs:
    /// every kind when the claim cannot exceed the ML budget, only non-ML
    /// kinds when that budget is spent, otherwise every kind but at most
    /// `free_ml` jobs (the loop claims again for the remaining slots).
    fn plan(&self, free: usize, free_ml: usize) -> Option<(&[&'static str], usize)> {
        if free == 0 {
            return None;
        }
        if self.ml.is_empty() || free_ml >= free {
            return Some((&self.all, free));
        }
        if free_ml == 0 {
            return (!self.not_ml.is_empty()).then_some((self.not_ml.as_slice(), free));
        }
        Some((&self.all, free_ml))
    }
}

/// One running ML job, counted until dropped (also when the task is aborted).
struct MlSlot(Arc<AtomicUsize>);

impl MlSlot {
    fn take(running: Arc<AtomicUsize>) -> Self {
        running.fetch_add(1, Ordering::SeqCst);
        Self(running)
    }
}

impl Drop for MlSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `LISTEN` on the jobs channel, best effort: while it is unavailable the
/// worker only polls, and it tries again on later loop turns. Never sleeps
/// itself, so a stop request or a finished job is never kept waiting.
#[derive(Default)]
struct Wakeups {
    listener: Option<PgListener>,
    /// No `recv` (nor reconnection) before this instant: the poll interval is the backoff.
    paused_until: Option<Instant>,
}

impl Wakeups {
    async fn connect(&mut self, pool: &PgPool, timeout: Duration) {
        let attempt = async {
            let mut listener = PgListener::connect_with(pool).await?;
            listener.listen(CHANNEL).await?;
            Ok::<_, sqlx::Error>(listener)
        };
        match tokio::time::timeout(timeout, attempt).await {
            Ok(Ok(listener)) => {
                if self.paused_until.is_some() {
                    tracing::info!("job notifications restored");
                }
                self.listener = Some(listener);
                self.paused_until = None;
            }
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "cannot listen for job notifications; polling");
                self.pause(timeout);
            }
            Err(_) => {
                tracing::warn!("listening for job notifications timed out; polling");
                self.pause(timeout);
            }
        }
    }

    fn pause(&mut self, delay: Duration) {
        self.paused_until = Some(Instant::now() + delay);
    }

    fn is_paused(&self) -> bool {
        self.paused_until
            .is_some_and(|until| Instant::now() < until)
    }

    fn should_reconnect(&self) -> bool {
        self.listener.is_none() && !self.is_paused()
    }

    fn is_listening(&self) -> bool {
        self.listener.is_some() && !self.is_paused()
    }

    /// Pending forever without a listener (the `select!` guard disables it then).
    async fn recv(&mut self) -> Result<(), sqlx::Error> {
        match &mut self.listener {
            // `PgListener` reconnects and listens again by itself on the next call.
            Some(listener) => listener.recv().await.map(drop),
            None => std::future::pending().await,
        }
    }
}

/// Aborts the task when dropped: a handler never outlives its job's claim
/// (timeout, lost lease, shutdown).
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One claimed job and what running it needs.
struct Run<'a> {
    pool: &'a PgPool,
    config: &'a WorkerConfig,
    job: &'a ClaimedJob,
    fault: Option<&'a HeartbeatFault>,
}

/// Extends the lease every lease/3 until it is lost. Returns when the handler
/// must be abandoned: another worker owns the job, or heartbeats keep failing
/// and the lease (as of the last success) runs out before the next one.
async fn keep_lease(run: &Run<'_>, claimed_at: Instant) {
    let (job, lease) = (run.job, run.config.lease);
    let interval = (lease / 3).max(MIN_HEARTBEAT);
    let mut deadline = claimed_at + lease;
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let sent = Instant::now();
        let beat = if run.fault.is_some_and(|fault| fault()) {
            Err(sqlx::Error::PoolTimedOut)
        } else {
            queue::heartbeat(run.pool, job, lease).await
        };
        match beat {
            Ok(true) => deadline = sent + lease,
            Ok(false) => {
                tracing::warn!(job_id = job.id, kind = %job.kind, "lease lost; abandoning the result");
                return;
            }
            Err(err) => {
                tracing::warn!(job_id = job.id, error = %err, "heartbeat failed");
                if deadline.saturating_duration_since(Instant::now()) <= interval {
                    tracing::warn!(
                        job_id = job.id,
                        kind = %job.kind,
                        "lease about to lapse without a heartbeat; abandoning the job to the reaper"
                    );
                    return;
                }
            }
        }
    }
}

/// Runs one job with a timeout and heartbeats, then records the outcome.
async fn execute(registry: &Registry, run: Run<'_>, claimed_at: Instant) {
    let (pool, job) = (run.pool, run.job);
    let Some(handler) = registry.handlers.get(job.kind.as_str()) else {
        if let Err(err) = queue::fail(pool, job, "no handler for this job kind").await {
            tracing::error!(job_id = job.id, error = %err, "could not record a job outcome");
        }
        return;
    };
    let ctx = JobContext {
        id: job.id,
        kind: job.kind.clone(),
        payload: job.payload.clone(),
        attempt: job.attempts,
        max_attempts: job.max_attempts,
    };
    // Its own task, so a panic becomes a JoinError instead of killing the loop.
    let mut work = AbortOnDrop(tokio::spawn((handler.run)(ctx)));
    let outcome = tokio::select! {
        result = tokio::time::timeout(handler.timeout, &mut work.0) => match result {
            Ok(Ok(result)) => result,
            Ok(Err(join)) => Err(JobError::retry(anyhow::anyhow!("handler panicked: {join}"))),
            Err(_) => Err(JobError::retry(anyhow::anyhow!("timed out after {:?}", handler.timeout))),
        },
        // Dropping `work` aborts the handler; nothing is recorded or released.
        () = keep_lease(&run, claimed_at) => return,
    };
    drop(work);
    record(pool, job, outcome).await;
}

async fn record(pool: &PgPool, job: &ClaimedJob, outcome: Result<(), JobError>) {
    let result = match &outcome {
        Ok(()) => queue::complete(pool, job.id, job.claim_id)
            .await
            .map(|done| (!done).then_some(Finished::LostLease)),
        Err(JobError::Retry(err)) => {
            let delay = retry_delay(job.attempts, jitter());
            queue::retry(pool, job, &format!("{err:#}"), delay)
                .await
                .map(Some)
        }
        Err(JobError::Permanent(err)) => {
            queue::fail(pool, job, &format!("{err:#}")).await.map(Some)
        }
        Err(JobError::Reschedule(delay)) => queue::reschedule(pool, job, *delay)
            .await
            .map(|kept| (!kept).then_some(Finished::LostLease)),
    };
    match (result, &outcome) {
        (Err(err), _) => {
            tracing::error!(job_id = job.id, error = %err, "could not record a job outcome")
        }
        (Ok(Some(Finished::Failed)), Err(err)) => tracing::error!(
            job_id = job.id,
            kind = %job.kind,
            attempts = job.attempts,
            error = %err,
            "job failed"
        ),
        (Ok(Some(Finished::Requeued)), Err(err)) => tracing::warn!(
            job_id = job.id,
            kind = %job.kind,
            attempt = job.attempts,
            error = %err,
            "job will be retried"
        ),
        (Ok(Some(Finished::LostLease)), _) => {
            tracing::warn!(job_id = job.id, "job outcome dropped: lease lost")
        }
        (Ok(None), Err(JobError::Reschedule(delay))) => {
            tracing::debug!(job_id = job.id, kind = %job.kind, ?delay, "job rescheduled")
        }
        _ => tracing::debug!(job_id = job.id, kind = %job.kind, "job done"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_never_exceed_the_ml_budget() {
        let kinds = ClaimKinds::new(
            vec!["clip_embed", "faces", "thumbnails"],
            &["clip_embed", "faces"],
        );
        assert!(kinds.is_ml("faces") && !kinds.is_ml("thumbnails"));
        assert_eq!(kinds.plan(0, 1), None);
        assert_eq!(kinds.plan(1, 1), Some((&kinds.all[..], 1)));
        assert_eq!(kinds.plan(2, 1), Some((&kinds.all[..], 1)));
        assert_eq!(kinds.plan(2, 0), Some((&["thumbnails"][..], 2)));
        let only_ml = ClaimKinds::new(vec!["faces"], &["faces"]);
        assert_eq!(only_ml.plan(2, 0), None);
        let no_ml = ClaimKinds::new(vec!["thumbnails"], &["faces"]);
        assert_eq!(no_ml.plan(2, 0), Some((&["thumbnails"][..], 2)));
    }
}
