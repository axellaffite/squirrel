//! `squirrel media-exec`: decode one untrusted input under resource limits.
//! Reads one JSON `ExecRequest` on stdin, writes one JSON `ExecResponse` on
//! stdout. Logs go to stderr (the parent keeps the tail for error messages).

use std::io::{Read, Write};

use rustix::process::{Resource, Rlimit, setrlimit};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(clap::Args)]
pub struct MediaExecArgs {
    #[arg(long, default_value_t = 2048)]
    max_memory_mb: u64,
    #[arg(long, default_value_t = 120)]
    max_cpu_secs: u64,
}

pub fn run(args: &MediaExecArgs) -> anyhow::Result<()> {
    // Before reading anything untrusted. Best effort on macOS (RLIMIT_AS is not enforced there).
    limit(Resource::As, args.max_memory_mb * 1024 * 1024);
    limit(Resource::Cpu, args.max_cpu_secs);
    limit(Resource::Nofile, 64);
    #[cfg(feature = "heic")]
    squirrel_media::heic::register();
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

fn limit(resource: Resource, value: u64) {
    if let Err(err) = setrlimit(
        resource,
        Rlimit {
            current: Some(value),
            maximum: Some(value),
        },
    ) {
        eprintln!("media-exec: could not set {resource:?} limit: {err}");
    }
}
