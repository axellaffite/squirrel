//! Runs `ExecRequest`s in a sandboxed child process (production) or on the
//! blocking pool (tests, `SQUIRREL_MEDIA_EXEC=in-process`).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse, execute};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

const STDERR_TAIL: usize = 2000;

#[derive(Debug, Clone)]
pub struct SubprocessConfig {
    /// The `squirrel` executable (`std::env::current_exe()` in production).
    pub program: PathBuf,
    pub max_memory_mb: u64,
    pub max_cpu_secs: u64,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum Executor {
    InProcess,
    Subprocess(SubprocessConfig),
}

/// Every variant is worth retrying (the job's attempt budget bounds it).
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("could not start the media sandbox: {0}")]
    Spawn(std::io::Error),
    #[error("media sandbox timed out after {0:?}")]
    TimedOut(Duration),
    #[error("media sandbox crashed ({status}): {stderr}")]
    Crashed { status: String, stderr: String },
    #[error("media sandbox protocol error: {0}")]
    Protocol(String),
}

impl Executor {
    pub async fn run(&self, request: ExecRequest) -> Result<ExecResponse, ExecError> {
        match self {
            Self::InProcess => tokio::task::spawn_blocking(move || execute(&request))
                .await
                .map_err(|err| ExecError::Crashed {
                    status: "panic".into(),
                    stderr: err.to_string(),
                }),
            Self::Subprocess(config) => run_child(config, &request).await,
        }
    }
}

/// The response is one small JSON object: anything bigger is a broken child.
const MAX_STDOUT: u64 = 1024 * 1024;
const MAX_STDERR: u64 = 64 * 1024;

/// What the child said, read with caps, and how it exited.
struct Exchange {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum ExchangeError {
    Io(std::io::Error),
    TooMuchOutput(&'static str),
}

/// The only variables the sandbox inherits: where to find programs, and
/// libheif's decoder plugins (a packaged libheif may need it to find libde265).
const PASSED_ENV: [&str; 2] = ["PATH", "LIBHEIF_PLUGIN_PATH"];

async fn run_child(
    config: &SubprocessConfig,
    request: &ExecRequest,
) -> Result<ExecResponse, ExecError> {
    let body = serde_json::to_vec(request).map_err(|err| ExecError::Protocol(err.to_string()))?;
    let mut command = Command::new(&config.program);
    // Nothing of the worker's environment (database URL, secrets) reaches the
    // process that parses untrusted bytes.
    command.env_clear();
    for name in PASSED_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command
        .arg("media-exec")
        .arg("--max-memory-mb")
        .arg(config.max_memory_mb.to_string())
        .arg("--max-cpu-secs")
        .arg(config.max_cpu_secs.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(ExecError::Spawn)?;
    let outcome = tokio::time::timeout(config.timeout, exchange(&mut child, body)).await;
    let exchange = match outcome {
        Ok(Ok(exchange)) => Ok(exchange),
        Ok(Err(ExchangeError::Io(err))) => Err(ExecError::Spawn(err)),
        Ok(Err(ExchangeError::TooMuchOutput(stream))) => Err(ExecError::Protocol(format!(
            "the child wrote too much on {stream}"
        ))),
        Err(_) => Err(ExecError::TimedOut(config.timeout)),
    };
    let exchange = match exchange {
        Ok(exchange) => exchange,
        Err(err) => {
            // Kill and reap now rather than leave it to `kill_on_drop`'s best effort.
            let _ = child.kill().await;
            return Err(err);
        }
    };
    if !exchange.status.success() {
        let stderr = String::from_utf8_lossy(&exchange.stderr);
        let tail: String = stderr
            .chars()
            .rev()
            .take(STDERR_TAIL)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(ExecError::Crashed {
            status: exchange.status.to_string(),
            stderr: tail,
        });
    }
    serde_json::from_slice(&exchange.stdout).map_err(|err| ExecError::Protocol(err.to_string()))
}

/// Sends the request, reads stdout and stderr concurrently (each capped),
/// then waits for the exit.
async fn exchange(child: &mut Child, body: Vec<u8>) -> Result<Exchange, ExchangeError> {
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let send = async move {
        if let Some(mut stdin) = stdin {
            // A child that died before reading reports itself through its exit status.
            match stdin.write_all(&body).await {
                Err(err) if err.kind() != std::io::ErrorKind::BrokenPipe => {
                    return Err(ExchangeError::Io(err));
                }
                _ => {}
            }
        }
        Ok(())
    };
    let (_, stdout, stderr) = tokio::try_join!(
        send,
        read_capped(stdout, MAX_STDOUT, "stdout"),
        read_capped(stderr, MAX_STDERR, "stderr"),
    )?;
    let status = child.wait().await.map_err(ExchangeError::Io)?;
    Ok(Exchange {
        status,
        stdout,
        stderr,
    })
}

async fn read_capped(
    stream: Option<impl AsyncRead + Unpin>,
    limit: u64,
    name: &'static str,
) -> Result<Vec<u8>, ExchangeError> {
    let mut buf = Vec::new();
    if let Some(stream) = stream {
        stream
            .take(limit + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(ExchangeError::Io)?;
    }
    if u64::try_from(buf.len()).unwrap_or(u64::MAX) > limit {
        return Err(ExchangeError::TooMuchOutput(name));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
    use squirrel_media::thumbs::DecodeLimits;

    fn request(dir: &std::path::Path) -> ExecRequest {
        let input = dir.join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(40, 30, &ExifSpec::default())).unwrap();
        ExecRequest::Image {
            input,
            out_dir: dir.into(),
            limits: DecodeLimits::default(),
        }
    }

    #[tokio::test]
    async fn in_process_execution_renders() {
        let dir = tempfile::tempdir().unwrap();
        let response = Executor::InProcess.run(request(dir.path())).await.unwrap();
        assert!(
            matches!(response, ExecResponse::Image { .. }),
            "{response:?}"
        );
    }

    #[tokio::test]
    async fn a_missing_sandbox_program_cannot_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::Subprocess(SubprocessConfig {
            program: "/nonexistent/squirrel".into(),
            max_memory_mb: 64,
            max_cpu_secs: 1,
            timeout: Duration::from_secs(5),
        });
        let err = executor.run(request(dir.path())).await.unwrap_err();
        assert!(matches!(err, ExecError::Spawn(_)), "{err}");
    }

    /// An executable shell script standing in for `squirrel media-exec`.
    fn script(dir: &std::path::Path, body: &str) -> SubprocessConfig {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-squirrel");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        SubprocessConfig {
            program: path,
            max_memory_mb: 64,
            max_cpu_secs: 1,
            timeout: Duration::from_secs(10),
        }
    }

    #[tokio::test]
    async fn the_child_sees_none_of_the_workers_environment() {
        // Cargo sets DATABASE_URL for this process (.cargo/config.toml).
        assert!(std::env::var_os("DATABASE_URL").is_some());
        let dir = tempfile::tempdir().unwrap();
        let dump = dir.path().join("env.txt");
        let config = script(dir.path(), &format!("env > '{}'", dump.display()));
        let err = Executor::Subprocess(config)
            .run(request(dir.path()))
            .await
            .unwrap_err();
        assert!(matches!(err, ExecError::Protocol(_)), "{err}");
        let env = std::fs::read_to_string(&dump).unwrap();
        assert!(env.lines().any(|l| l.starts_with("PATH=")), "{env}");
        for secret in ["DATABASE_URL", "SQUIRREL_", "CARGO"] {
            assert!(
                !env.lines().any(|l| l.starts_with(secret)),
                "{secret} leaked: {env}"
            );
        }
    }

    #[tokio::test]
    async fn a_child_writing_too_much_is_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        let config = script(dir.path(), "head -c 3000000 /dev/zero");
        let err = Executor::Subprocess(config)
            .run(request(dir.path()))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ExecError::Protocol(m) if m.contains("stdout")),
            "{err}"
        );
        let config = script(dir.path(), "head -c 3000000 /dev/zero >&2");
        let err = Executor::Subprocess(config)
            .run(request(dir.path()))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, ExecError::Protocol(m) if m.contains("stderr")),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_timed_out_child_is_killed_and_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let mut config = script(
            dir.path(),
            &format!("echo $$ > '{}'\nexec sleep 30", pid_file.display()),
        );
        config.timeout = Duration::from_millis(500);
        let err = Executor::Subprocess(config)
            .run(request(dir.path()))
            .await
            .unwrap_err();
        assert!(matches!(err, ExecError::TimedOut(_)), "{err}");
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        // A zombie still answers `kill -0`: failing means it was reaped too.
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(!alive.success(), "process {} still exists", pid.trim());
    }
}
