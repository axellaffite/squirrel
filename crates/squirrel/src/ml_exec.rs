//! `squirrel ml-exec`: a long-lived inference child. Applies its resource
//! limits and lowers its priority before loading anything, verifies and loads
//! the requested model groups, writes a `ready` line, then answers one JSON
//! line per request (`squirrel_ml::protocol`) until stdin closes. Logs go to
//! stderr (stdout carries the protocol).

use std::path::PathBuf;

use anyhow::Context;
use rustix::process::Resource;
use squirrel_ml::ModelGroup;
use squirrel_ml::models::ModelDir;
use squirrel_ml::serve::{Engine, EngineConfig, serve};

use crate::media_exec::apply;

/// Address space for the image tower and the face models plus ONNX Runtime's
/// arenas and thread stacks (which reserve far more than they touch). Linux only.
pub const DEFAULT_MEMORY_MB: u64 = 4096;
/// `nice` value: the web server and thumbnails win CPU contention.
const NICE: i32 = 10;
const MAX_OPEN_FILES: u64 = 64;

#[derive(clap::Args, Debug)]
pub struct MlExecArgs {
    /// Model groups to load, comma-separated (`clip-image,clip-text,faces`); may be empty.
    #[arg(long, value_delimiter = ',')]
    pub groups: Vec<String>,
    #[arg(long)]
    pub models_dir: PathBuf,
    /// The pinned ONNX Runtime library (needed when a group is loaded).
    #[arg(long)]
    pub ort_dylib: Option<PathBuf>,
    /// Intra-op threads for one inference.
    #[arg(long, default_value_t = 1)]
    pub threads: usize,
    #[arg(long, default_value_t = DEFAULT_MEMORY_MB)]
    pub max_memory_mb: u64,
}

pub fn run(args: &MlExecArgs) -> anyhow::Result<()> {
    // Before loading anything: without its limits the child refuses to run.
    for (resource, value) in limits(args) {
        apply(resource, value)?;
    }
    rustix::process::setpriority_process(None, NICE).context("lowering the priority")?;
    let groups = parse_groups(&args.groups)?;
    let config = EngineConfig {
        groups,
        models: ModelDir::new(&args.models_dir),
        ort_dylib: args.ort_dylib.clone(),
        threads: args.threads.max(1),
    };
    let mut engine = Engine::load(&config).context("loading the models")?;
    serve(
        &mut engine,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )?;
    Ok(())
}

fn parse_groups(names: &[String]) -> anyhow::Result<Vec<ModelGroup>> {
    names
        .iter()
        .filter(|name| !name.is_empty())
        .map(|name| ModelGroup::parse(name).with_context(|| format!("unknown model group {name}")))
        .collect()
}

fn limits(args: &MlExecArgs) -> [(Resource, u64); 4] {
    [
        (Resource::As, args.max_memory_mb.saturating_mul(1024 * 1024)),
        (Resource::Nofile, MAX_OPEN_FILES),
        // It writes no file at all.
        (Resource::Fsize, 0),
        (Resource::Core, 0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_parse_and_an_empty_list_is_fine() {
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(parse_groups(&names(&[""])).unwrap(), []);
        assert_eq!(
            parse_groups(&names(&["clip-image", "faces"])).unwrap(),
            [ModelGroup::ClipImage, ModelGroup::Faces]
        );
        assert!(parse_groups(&names(&["clip"])).is_err());
    }

    #[test]
    fn the_child_writes_no_file_and_dumps_no_core() {
        let args = MlExecArgs {
            groups: Vec::new(),
            models_dir: "/m".into(),
            ort_dylib: None,
            threads: 1,
            max_memory_mb: 4096,
        };
        let limits = limits(&args);
        assert!(limits.contains(&(Resource::Fsize, 0)));
        assert!(limits.contains(&(Resource::Core, 0)));
        assert!(limits.contains(&(Resource::As, 4096 * 1024 * 1024)));
    }
}
