//! `squirrel media-exec`: decode one untrusted input under resource limits.
//! Reads one JSON `ExecRequest` on stdin, writes one JSON `ExecResponse` on
//! stdout. Logs go to stderr (the parent keeps the tail for error messages).

use std::io::{Read, Write};

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(clap::Args)]
pub struct MediaExecArgs {
    #[arg(long, default_value_t = 2048)]
    max_memory_mb: u64,
    #[arg(long, default_value_t = 120)]
    max_cpu_secs: u64,
}

/// Largest file the child may write (its outputs are thumbnails of a few hundred KB).
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OPEN_FILES: u64 = 64;

pub fn run(args: &MediaExecArgs) -> anyhow::Result<()> {
    // Before reading anything untrusted: without its limits the child refuses to run.
    for (resource, value) in limits(args) {
        apply(resource, value)?;
    }
    squirrel_media::register_decoders();
    let mut raw = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES)
        .read_to_end(&mut raw)?;
    let request: squirrel_media::exec::ExecRequest = serde_json::from_slice(&raw)?;
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

/// Sets soft and hard limits to `value`, or to the current hard limit when
/// that is lower already (lowering never needs privileges).
fn apply(resource: Resource, value: u64) -> anyhow::Result<()> {
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
            eprintln!("media-exec: address-space limit unsupported here: {err}");
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
        };
        let limits = limits(&args);
        assert!(limits.contains(&(Resource::Core, 0)));
        assert!(limits.contains(&(Resource::Fsize, MAX_FILE_BYTES)));
        assert!(limits.contains(&(Resource::As, 2048 * 1024 * 1024)));
    }
}
