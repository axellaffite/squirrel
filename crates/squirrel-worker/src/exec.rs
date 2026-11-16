//! Runs `ExecRequest`s in a sandboxed child process (production) or on the
//! blocking pool (tests, `SQUIRREL_MEDIA_EXEC=in-process`).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse, execute};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

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

async fn run_child(
    config: &SubprocessConfig,
    request: &ExecRequest,
) -> Result<ExecResponse, ExecError> {
    let body = serde_json::to_vec(request).map_err(|err| ExecError::Protocol(err.to_string()))?;
    let mut child = Command::new(&config.program)
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
    let stdin = child.stdin.take();
    let exchange = async move {
        if let Some(mut stdin) = stdin {
            // A child that died before reading reports itself through its exit status.
            match stdin.write_all(&body).await {
                Err(err) if err.kind() != std::io::ErrorKind::BrokenPipe => return Err(err),
                _ => {}
            }
        }
        child.wait_with_output().await
    };
    // Dropping the future on timeout drops the child, which kills it.
    let output = tokio::time::timeout(config.timeout, exchange)
        .await
        .map_err(|_| ExecError::TimedOut(config.timeout))?
        .map_err(ExecError::Spawn)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr
            .chars()
            .rev()
            .take(STDERR_TAIL)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(ExecError::Crashed {
            status: output.status.to_string(),
            stderr: tail,
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|err| ExecError::Protocol(err.to_string()))
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
}
