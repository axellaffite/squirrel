//! `squirrel media-exec`: decode one untrusted input under resource limits.
//! Reads one JSON `ExecRequest` on stdin, writes one JSON `ExecResponse` on
//! stdout. Logs go to stderr (the parent keeps the tail for error messages).
//! `--probe` prints what libvips looks like here instead.

use std::io::{Read, Write};

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
use squirrel_media::exec::ExecRequest;

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(clap::Args)]
pub struct MediaExecArgs {
    #[arg(long, default_value_t = 2048)]
    max_memory_mb: u64,
    #[arg(long, default_value_t = 120)]
    max_cpu_secs: u64,
    /// Print libvips' version and loaders as seen from the sandbox (JSON) and exit.
    #[arg(long)]
    probe: bool,
}

/// Previews (and the probe) get tighter limits than the launcher's, whoever
/// asks: a preview decodes one photo, interactively (see the previews plan).
pub const PREVIEW_MEMORY_MB: u64 = 1536;
pub const PREVIEW_CPU_SECS: u64 = 10;
/// The server's wall clock on one preview render.
const PREVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The server's preview renderer: a `media-exec` child per render with the
/// preview limits, or in-process (`SQUIRREL_MEDIA_EXEC=in-process`).
pub fn preview_executor(
    mode: crate::worker::MediaExecMode,
) -> anyhow::Result<squirrel_sandbox::exec::Executor> {
    use anyhow::Context;
    use squirrel_sandbox::exec::{Executor, SubprocessConfig};
    Ok(match mode {
        crate::worker::MediaExecMode::InProcess => Executor::InProcess,
        crate::worker::MediaExecMode::Subprocess => Executor::Subprocess(SubprocessConfig {
            program: std::env::current_exe().context("locating the squirrel executable")?,
            max_memory_mb: PREVIEW_MEMORY_MB,
            max_cpu_secs: PREVIEW_CPU_SECS,
            timeout: PREVIEW_TIMEOUT,
        }),
    })
}

/// Largest file the child may write (its outputs are thumbnails of a few hundred KB).
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OPEN_FILES: u64 = 64;

pub fn run(args: &MediaExecArgs) -> anyhow::Result<()> {
    // Before reading anything untrusted: without its limits the child refuses to run.
    for (resource, value) in limits(args) {
        apply(resource, value)?;
    }
    no_new_privs()?;
    squirrel_media::register_decoders();
    let request = if args.probe {
        ExecRequest::Probe
    } else {
        let mut raw = Vec::new();
        std::io::stdin()
            .take(MAX_REQUEST_BYTES)
            .read_to_end(&mut raw)?;
        serde_json::from_slice(&raw)?
    };
    // Lowering a limit never needs privileges: still before any decoding.
    for (resource, value) in operation_limits(&request) {
        apply(resource, value)?;
    }
    let response = squirrel_media::exec::execute(&request);
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &response)?;
    out.flush()?;
    Ok(())
}

fn limits(args: &MediaExecArgs) -> [(Resource, u64); 5] {
    [
        (Resource::As, args.max_memory_mb.saturating_mul(1024 * 1024)),
        (Resource::Cpu, args.max_cpu_secs),
        (Resource::Nofile, MAX_OPEN_FILES),
        (Resource::Fsize, MAX_FILE_BYTES),
        // A crashing decoder must not leave a core dump of what it parsed.
        (Resource::Core, 0),
    ]
}

/// The limits `request` lowers further ([`apply`] never raises one).
fn operation_limits(request: &ExecRequest) -> Vec<(Resource, u64)> {
    match request {
        // A hash decodes one stored thumbnail: the preview limits are ample.
        ExecRequest::Preview { .. } | ExecRequest::Probe | ExecRequest::VisualHash { .. } => vec![
            (Resource::As, PREVIEW_MEMORY_MB * 1024 * 1024),
            (Resource::Cpu, PREVIEW_CPU_SECS),
        ],
        ExecRequest::Image { .. } | ExecRequest::Pdf { .. } => Vec::new(),
    }
}

/// Sets soft and hard limits to `value`, or to the current hard limit when
/// that is lower already (lowering never needs privileges).
/// Linux: no privilege gain through exec (setuid binaries, file capabilities)
/// for the sandboxed child and anything it starts. Elsewhere a no-op.
pub(crate) fn no_new_privs() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    rustix::thread::set_no_new_privs(true)
        .map_err(|err| anyhow::anyhow!("could not set no_new_privs: {err}"))?;
    Ok(())
}

pub(crate) fn apply(resource: Resource, value: u64) -> anyhow::Result<()> {
    let value = clamp(value, getrlimit(resource).maximum);
    let set = setrlimit(
        resource,
        Rlimit {
            current: Some(value),
            maximum: Some(value),
        },
    );
    match set {
        Ok(()) => Ok(()),
        // macOS does not support RLIMIT_AS (EINVAL): the CPU limit and the
        // parent's wall clock still apply. Every other failure is fatal.
        Err(err) if cfg!(target_os = "macos") && resource == Resource::As => {
            eprintln!("address-space limit unsupported here: {err}");
            Ok(())
        }
        Err(err) => anyhow::bail!("could not set the {resource:?} limit: {err}"),
    }
}

fn clamp(value: u64, hard: Option<u64>) -> u64 {
    hard.map_or(value, |hard| value.min(hard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_never_exceed_the_hard_limit_already_set() {
        assert_eq!(clamp(64, None), 64);
        assert_eq!(clamp(64, Some(1024)), 64);
        assert_eq!(clamp(64, Some(16)), 16);
    }

    #[test]
    fn the_child_gets_no_core_dumps_and_small_files() {
        let args = MediaExecArgs {
            max_memory_mb: 2048,
            max_cpu_secs: 120,
            probe: false,
        };
        let limits = limits(&args);
        assert!(limits.contains(&(Resource::Core, 0)));
        assert!(limits.contains(&(Resource::Fsize, MAX_FILE_BYTES)));
        assert!(limits.contains(&(Resource::As, 2048 * 1024 * 1024)));
    }

    #[test]
    fn previews_get_tighter_limits_than_thumbnails() {
        assert!(operation_limits(&ExecRequest::Probe).contains(&(Resource::Cpu, 10)));
        let thumbs = ExecRequest::Image {
            input: "/a".into(),
            out_dir: "/b".into(),
            limits: squirrel_media::thumbs::DecodeLimits::default(),
        };
        assert!(operation_limits(&thumbs).is_empty());
        let hash = ExecRequest::VisualHash {
            input: "/a".into(),
            limits: squirrel_media::thumbs::DecodeLimits::default(),
        };
        assert!(operation_limits(&hash).contains(&(Resource::Cpu, PREVIEW_CPU_SECS)));
    }
}
