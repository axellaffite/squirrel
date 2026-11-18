//! Supervises the long-lived `squirrel ml-exec` child: started on the first
//! request, one request at a time, killed and restarted on a crash, a timeout
//! or a protocol error (that request fails, retryably), recycled after
//! `max_requests` and stopped after `idle` without requests.
//! `in-process` runs the same engine on the blocking pool instead (tests).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Weak};
use std::time::Duration;

use crate::protocol::{MAX_RESPONSE_LINE, MlBody, MlOp, MlRequest, MlResponse, Ready};
use crate::serve::Engine;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::Instant;

/// The only variables the child inherits (no database URL, no secrets).
const PASSED_ENV: [&str; 1] = ["PATH"];
/// The child's glibc malloc arenas (see `start`).
const MALLOC_ARENA_MAX: &str = "2";
/// After a child fails to start, no new start for this long, doubling up to
/// [`MAX_RESTART_BACKOFF`] (a broken install must not respawn on every job).
pub const RESTART_BACKOFF: Duration = Duration::from_secs(30);
pub const MAX_RESTART_BACKOFF: Duration = Duration::from_secs(10 * 60);
/// When a request gave up waiting for the child, try again after this.
pub const BUSY_RETRY: Duration = Duration::from_secs(30);
/// Longest stderr line logged from the child.
const MAX_LOG_LINE: usize = 2000;

#[derive(Debug, Clone)]
pub struct MlChildConfig {
    /// The `squirrel` executable.
    pub program: PathBuf,
    /// Arguments after `ml-exec` (`--groups …`, `--models-dir …`, …).
    pub args: Vec<OsString>,
    /// Starting includes hashing the models: a few seconds.
    pub ready_timeout: Duration,
    pub request_timeout: Duration,
    /// Recycled after this many requests (bounds fragmentation).
    pub max_requests: u32,
    /// Stopped after this long without a request (frees its memory).
    pub idle: Duration,
    /// First pause after a failed start, doubled after each further failure.
    pub restart_backoff: Duration,
    pub max_restart_backoff: Duration,
    /// How long a request waits for the requests ahead of it (and a start)
    /// before giving up with a retryable [`MlClientError::Busy`].
    pub queue_timeout: Duration,
    /// While a child is starting, other requests fail at once with `Busy`
    /// instead of waiting for it (the server: a search answers 503 and the
    /// client retries; the worker's jobs just wait).
    pub busy_while_starting: bool,
}

pub enum MlBackend {
    Subprocess(MlChildConfig),
    /// The engine in this process (tests; `SQUIRREL_ML_EXEC=in-process`).
    InProcess(Arc<std::sync::Mutex<Engine>>),
    /// Tests: answers every request with this function, without any model.
    #[doc(hidden)]
    Fake(FakeFn),
}

/// What a [`MlBackend::Fake`] answers.
pub type FakeFn = Arc<dyn Fn(&MlOp) -> Result<MlBody, MlClientError> + Send + Sync>;

/// Every variant is worth retrying (the job's attempt budget bounds it).
#[derive(Debug, thiserror::Error)]
pub enum MlClientError {
    #[error("could not start the ML child: {0}")]
    Spawn(std::io::Error),
    #[error("the ML child did not get ready: {0}")]
    NotReady(String),
    #[error("the ML child timed out after {0:?}")]
    TimedOut(Duration),
    #[error("the ML child crashed: {0}")]
    Crashed(String),
    #[error("ML child protocol error: {0}")]
    Protocol(String),
    /// The child failed to start recently: no new start before this delay.
    /// Handlers reschedule (no attempt used) rather than fail.
    #[error("the ML child failed to start; next start in {0:?}")]
    Backoff(Duration),
    /// Other requests held the child longer than one start and one request
    /// would take: try again later rather than queue without bound.
    #[error("the ML child stayed busy for {0:?}")]
    Busy(Duration),
}

impl MlClientError {
    /// When to try again without spending an attempt (`Backoff`).
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Backoff(delay) => Some(*delay),
            Self::Busy(_) => Some(BUSY_RETRY),
            _ => None,
        }
    }
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    served: u32,
    last_used: Instant,
    /// Tells the idle watcher of a stopped child from its successor.
    generation: u64,
    /// Set around each exchange: still set when a request starts means the
    /// previous caller was cancelled mid-exchange, leaving its answer (or
    /// half a request) in the pipes. That child is replaced.
    busy: bool,
}

#[derive(Default)]
struct State {
    running: Option<Running>,
    /// What the last child said when it got ready.
    ready: Option<Ready>,
    next_id: u64,
    generation: u64,
    /// No start before this instant (after a failed start).
    blocked_until: Option<Instant>,
    /// The pause after the next failed start (`None`: the configured first one).
    next_backoff: Option<Duration>,
}

pub struct MlClient {
    backend: MlBackend,
    state: Arc<Mutex<State>>,
    /// A child is being started (read without the state lock).
    starting: Arc<std::sync::atomic::AtomicBool>,
}

/// Clears `starting` however the start ends (cancelled included).
struct Starting(Arc<std::sync::atomic::AtomicBool>);

impl Drop for Starting {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl MlClient {
    pub fn new(backend: MlBackend) -> Self {
        Self {
            backend,
            state: Arc::default(),
            starting: Arc::default(),
        }
    }

    pub async fn request(&self, op: MlOp) -> Result<MlBody, MlClientError> {
        match &self.backend {
            MlBackend::InProcess(engine) => {
                let engine = engine.clone();
                tokio::task::spawn_blocking(move || {
                    let mut engine = engine
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    engine.handle(&op)
                })
                .await
                .map_err(|err| MlClientError::Crashed(err.to_string()))
            }
            MlBackend::Subprocess(config) => self.request_child(config, op).await,
            MlBackend::Fake(answer) => answer(&op),
        }
    }

    /// Starts the child if needed and pings it: what it loaded.
    pub async fn health(&self) -> Result<Ready, MlClientError> {
        let body = self.request(MlOp::Ping).await?;
        if !matches!(body, MlBody::Ok(_)) {
            return Err(MlClientError::Protocol(format!("ping answered {body:?}")));
        }
        Ok(match &self.backend {
            MlBackend::InProcess(engine) => engine
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ready(),
            MlBackend::Fake(_) => Ready {
                ready: true,
                groups: Vec::new(),
                onnxruntime: None,
            },
            MlBackend::Subprocess(_) => self.state.lock().await.ready.clone().unwrap_or(Ready {
                ready: true,
                groups: Vec::new(),
                onnxruntime: None,
            }),
        })
    }

    /// The running child's process id (`None` when stopped or in-process).
    pub async fn child_id(&self) -> Option<u32> {
        let state = self.state.lock().await;
        state
            .running
            .as_ref()
            .and_then(|running| running.child.id())
    }

    /// Stops the child now (it starts again on the next request).
    pub async fn stop(&self) {
        let mut state = self.state.lock().await;
        if let Some(running) = state.running.take() {
            kill(running).await;
        }
    }

    async fn request_child(
        &self,
        config: &MlChildConfig,
        op: MlOp,
    ) -> Result<MlBody, MlClientError> {
        use std::sync::atomic::Ordering;
        if config.busy_while_starting && self.starting.load(Ordering::SeqCst) {
            return Err(MlClientError::Busy(Duration::ZERO));
        }
        let wait = config.queue_timeout;
        let Ok(mut state) = tokio::time::timeout(wait, self.state.lock()).await else {
            return Err(MlClientError::Busy(wait));
        };
        if state.running.as_ref().is_some_and(|running| running.busy)
            && let Some(stale) = state.running.take()
        {
            tracing::warn!("a cancelled ML request left the child mid-exchange; restarting it");
            kill(stale).await;
        }
        if state.running.is_none() {
            if let Some(until) = state.blocked_until {
                let left = until.saturating_duration_since(Instant::now());
                if !left.is_zero() {
                    return Err(MlClientError::Backoff(left));
                }
            }
            state.generation += 1;
            self.starting.store(true, Ordering::SeqCst);
            let started = {
                let _starting = Starting(self.starting.clone());
                start(config, state.generation).await
            };
            match started {
                Ok((running, ready)) => {
                    state.running = Some(running);
                    state.ready = Some(ready);
                    state.blocked_until = None;
                    state.next_backoff = None;
                }
                Err(err) => {
                    let pause = state.next_backoff.unwrap_or(config.restart_backoff);
                    state.blocked_until = Some(Instant::now() + pause);
                    state.next_backoff = Some((pause * 2).min(config.max_restart_backoff));
                    tracing::error!(error = %err, next_start_in = ?pause, "the ML child failed to start");
                    return Err(err);
                }
            }
            watch_idle(Arc::downgrade(&self.state), config.idle, state.generation);
        }
        state.next_id += 1;
        // Health checks and warm-ups do not wear the child out.
        let counts = !matches!(op, MlOp::Ping);
        let request = MlRequest {
            id: state.next_id,
            op,
        };
        let Some(running) = state.running.as_mut() else {
            unreachable!("started above");
        };
        running.busy = true;
        let outcome =
            tokio::time::timeout(config.request_timeout, exchange(running, &request)).await;
        running.busy = false;
        let result = match outcome {
            Ok(result) => result,
            Err(_) => Err(MlClientError::TimedOut(config.request_timeout)),
        };
        match result {
            Ok(body) => {
                running.served += u32::from(counts);
                running.last_used = Instant::now();
                if running.served >= config.max_requests
                    && let Some(done) = state.running.take()
                {
                    kill(done).await;
                }
                Ok(body)
            }
            Err(err) => {
                if let Some(broken) = state.running.take() {
                    kill(broken).await;
                }
                Err(err)
            }
        }
    }
}

async fn start(config: &MlChildConfig, generation: u64) -> Result<(Running, Ready), MlClientError> {
    let mut command = Command::new(&config.program);
    command.env_clear();
    for name in PASSED_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // glibc gives each thread up to 8 × cores malloc arenas of 64 MB reserved
    // address space: with 32 threads the child reserved 9.3 GB (measured on
    // Linux arm64) and failed under its RLIMIT_AS. Two arenas: 2.0 GB, and
    // 3.0 GB at 64 threads; the resident set is the same (1.07 GB).
    command.env("MALLOC_ARENA_MAX", MALLOC_ARENA_MAX);
    // Its own process group: whatever it starts is killed with it.
    command.process_group(0);
    let mut child = command
        .arg("ml-exec")
        .args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(MlClientError::Spawn)?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(log_stderr(BufReader::new(stderr)));
    }
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(MlClientError::NotReady("no pipes".into()));
    };
    let mut running = Running {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        served: 0,
        last_used: Instant::now(),
        generation,
        busy: false,
    };
    let ready = tokio::time::timeout(config.ready_timeout, read_line(&mut running.stdout)).await;
    let ready = match ready {
        Ok(Ok(line)) => serde_json::from_slice::<Ready>(&line)
            .map_err(|err| MlClientError::NotReady(format!("bad ready line: {err}"))),
        Ok(Err(err)) => Err(MlClientError::NotReady(err.to_string())),
        Err(_) => Err(MlClientError::NotReady(format!(
            "no ready line within {:?}",
            config.ready_timeout
        ))),
    };
    match ready {
        Ok(ready) if ready.ready => {
            tracing::info!(
                groups = ?ready.groups,
                onnxruntime = ready.onnxruntime.as_deref().unwrap_or("none"),
                pid = running.child.id(),
                "ML child ready"
            );
            Ok((running, ready))
        }
        Ok(_) => {
            kill(running).await;
            Err(MlClientError::NotReady(
                "the child said it is not ready".into(),
            ))
        }
        Err(err) => {
            kill(running).await;
            Err(err)
        }
    }
}

async fn exchange(running: &mut Running, request: &MlRequest) -> Result<MlBody, MlClientError> {
    let mut line =
        serde_json::to_vec(request).map_err(|err| MlClientError::Protocol(err.to_string()))?;
    line.push(b'\n');
    let crashed = |err: std::io::Error| MlClientError::Crashed(err.to_string());
    running.stdin.write_all(&line).await.map_err(crashed)?;
    running.stdin.flush().await.map_err(crashed)?;
    let line = read_line(&mut running.stdout).await?;
    let response: MlResponse = serde_json::from_slice(&line)
        .map_err(|err| MlClientError::Protocol(format!("bad response: {err}")))?;
    if response.id != request.id {
        return Err(MlClientError::Protocol(format!(
            "response {} to request {}",
            response.id, request.id
        )));
    }
    Ok(response.body)
}

/// One line, at most [`MAX_RESPONSE_LINE`] bytes. End of file is a crash.
async fn read_line(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Vec<u8>, MlClientError> {
    let mut line = Vec::new();
    let read = reader
        .take(MAX_RESPONSE_LINE as u64 + 1)
        .read_until(b'\n', &mut line)
        .await
        .map_err(|err| MlClientError::Crashed(err.to_string()))?;
    if read == 0 {
        return Err(MlClientError::Crashed("the child exited".into()));
    }
    if line.last() != Some(&b'\n') {
        return Err(if line.len() > MAX_RESPONSE_LINE {
            MlClientError::Protocol("response line too long".into())
        } else {
            MlClientError::Crashed("the child exited mid-line".into())
        });
    }
    Ok(line)
}

/// Kills the child's process group and reaps the child.
async fn kill(mut running: Running) {
    if let Some(pid) = running
        .child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw)
    {
        // Already gone (ESRCH) is fine.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = running.child.kill().await;
}

/// Stops the child of `generation` once it has been idle for `idle`.
fn watch_idle(state: Weak<Mutex<State>>, idle: Duration, generation: u64) {
    tokio::spawn(async move {
        let mut wait = idle;
        loop {
            tokio::time::sleep(wait).await;
            let Some(state) = state.upgrade() else {
                return;
            };
            let mut state = state.lock().await;
            let Some(running) = state.running.as_ref() else {
                return;
            };
            if running.generation != generation {
                return;
            }
            let quiet = running.last_used.elapsed();
            if quiet >= idle {
                if let Some(running) = state.running.take() {
                    tracing::info!(idle = ?idle, "stopping the idle ML child");
                    kill(running).await;
                }
                return;
            }
            wait = idle - quiet;
        }
    });
}

/// At most this many child log lines per [`LOG_WINDOW`]; the rest are counted
/// and summarised (a looping child must not flood the worker's log).
const LOG_LINES_PER_WINDOW: u32 = 20;
const LOG_WINDOW: Duration = Duration::from_secs(60);

/// A fixed-window line budget.
pub(crate) struct LogBudget {
    window_start: Instant,
    logged: u32,
    dropped: u32,
}

impl LogBudget {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            logged: 0,
            dropped: 0,
        }
    }

    /// Whether to log a line seen at `now`, and how many lines were dropped
    /// in the window that just ended (to report once).
    pub(crate) fn admit(&mut self, now: Instant) -> (bool, u32) {
        let mut dropped_before = 0;
        if now.duration_since(self.window_start) >= LOG_WINDOW {
            dropped_before = std::mem::take(&mut self.dropped);
            self.window_start = now;
            self.logged = 0;
        }
        if self.logged < LOG_LINES_PER_WINDOW {
            self.logged += 1;
            (true, dropped_before)
        } else {
            self.dropped += 1;
            (false, dropped_before)
        }
    }
}

async fn log_stderr(mut stderr: BufReader<tokio::process::ChildStderr>) {
    let mut line = Vec::new();
    let mut budget = LogBudget::new(Instant::now());
    loop {
        line.clear();
        match (&mut stderr)
            .take(64 * 1024)
            .read_until(b'\n', &mut line)
            .await
        {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let (admitted, dropped) = budget.admit(Instant::now());
                if dropped > 0 {
                    tracing::warn!(target: "ml_exec", dropped, "ML child log lines dropped");
                }
                let text = String::from_utf8_lossy(&line);
                let text: String = text.trim_end().chars().take(MAX_LOG_LINE).collect();
                if admitted && !text.is_empty() {
                    tracing::warn!(target: "ml_exec", "{text}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
