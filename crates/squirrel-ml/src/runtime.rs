//! The pinned ONNX Runtime, loaded at run time (`scripts/fetch-onnxruntime.sh`),
//! and sessions built on it. CPU only.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

/// The ONNX Runtime minor version the fetch script pins (1.31.x).
pub const ORT_MINOR: u32 = 31;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("ONNX Runtime library {path} not found (run scripts/fetch-onnxruntime.sh)")]
    Missing { path: PathBuf },
    #[error(
        "{path} is not ONNX Runtime 1.{ORT_MINOR}.x (found {found}); run scripts/fetch-onnxruntime.sh"
    )]
    Version { path: PathBuf, found: String },
    #[error("ONNX Runtime is already loaded from {loaded}, not {path}")]
    OtherLibrary { loaded: PathBuf, path: PathBuf },
    #[error("loading ONNX Runtime from {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("loading model {path}: {message}")]
    Session { path: PathBuf, message: String },
}

struct Loaded {
    path: PathBuf,
    version: String,
    environment: Environment,
}

static LOADED: OnceLock<Loaded> = OnceLock::new();
/// Serialises the first load (`OnceLock::get_or_try_init` is unstable).
static LOADING: Mutex<()> = Mutex::new(());

/// Loads ONNX Runtime from `dylib` once per process and returns its version.
/// Calling it again with the same library is fine; another library is an error.
pub fn init(dylib: &Path) -> Result<&'static str, RuntimeError> {
    let path = std::fs::canonicalize(dylib).map_err(|_| RuntimeError::Missing {
        path: dylib.to_owned(),
    })?;
    let _guard = LOADING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(loaded) = LOADED.get() {
        return if loaded.path == path {
            Ok(version())
        } else {
            Err(RuntimeError::OtherLibrary {
                loaded: loaded.path.clone(),
                path,
            })
        };
    }
    match ort::util::probe(&path) {
        Some(ORT_MINOR) => {}
        Some(minor) => {
            return Err(RuntimeError::Version {
                path,
                found: format!("1.{minor}"),
            });
        }
        None => {
            return Err(RuntimeError::Version {
                path,
                found: "an unreadable version".into(),
            });
        }
    }
    let load = |message: String| RuntimeError::Load {
        path: path.clone(),
        message,
    };
    // No global thread pool: each session gets its own, sized by the caller.
    let environment = ort::init_from(&path)
        .map_err(|err| load(err.to_string()))?
        .with_name("squirrel")
        .with_telemetry(false)
        .build()
        .map_err(|err| load(err.to_string()))?;
    let full = full_version(ort::info(), ORT_MINOR);
    let _ = LOADED.set(Loaded {
        path,
        version: full,
        environment,
    });
    Ok(version())
}

/// The loaded ONNX Runtime's version, e.g. "1.31.1".
fn version() -> &'static str {
    LOADED
        .get()
        .map_or("unknown", |loaded| loaded.version.as_str())
}

/// "1.31.1" from the build info ("… git-branch=rel-1.31.1, …"), else "1.<minor>".
fn full_version(build_info: &str, minor: u32) -> String {
    build_info
        .split(|c: char| c == ',' || c.is_whitespace())
        .find_map(|part| part.strip_prefix("git-branch=rel-"))
        .filter(|v| v.starts_with(&format!("1.{minor}")))
        .map_or_else(|| format!("1.{minor}"), str::to_owned)
}

/// A CPU session on the loaded runtime: `intra_threads` threads for one
/// inference, one inter-op thread, full graph optimisation.
pub fn session(model: &Path, intra_threads: usize) -> Result<Session, RuntimeError> {
    let fail = |message: String| RuntimeError::Session {
        path: model.to_owned(),
        message,
    };
    let loaded = LOADED
        .get()
        .ok_or_else(|| fail("ONNX Runtime is not loaded".into()))?;
    Session::builder(&loaded.environment)
        .map_err(|err| fail(err.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|err| fail(err.to_string()))?
        .with_intra_threads(intra_threads.max(1))
        .map_err(|err| fail(err.to_string()))?
        .with_inter_threads(1)
        .map_err(|err| fail(err.to_string()))?
        .commit_from_file(model)
        .map_err(|err| fail(err.to_string()))
}

/// Default threads for one inference: half the cores, at least one.
pub fn default_threads() -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (cores / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_library_names_the_path_and_the_script() {
        let err = init(Path::new("/nonexistent/libonnxruntime.so")).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("/nonexistent/libonnxruntime.so"),
            "{message}"
        );
        assert!(
            message.contains("scripts/fetch-onnxruntime.sh"),
            "{message}"
        );
    }

    #[test]
    fn a_file_that_is_not_onnx_runtime_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("libonnxruntime.so");
        std::fs::write(&fake, b"not a library").unwrap();
        let err = init(&fake).unwrap_err();
        assert!(
            matches!(
                err,
                RuntimeError::Version { .. } | RuntimeError::OtherLibrary { .. }
            ),
            "{err}"
        );
    }

    #[test]
    fn the_pinned_runtime_loads_once_and_reports_its_version() {
        let Some(dylib) = crate::testutil::test_ort() else {
            return;
        };
        let version = init(&dylib).unwrap();
        assert!(version.starts_with("1.31."), "{version}");
        assert_eq!(init(&dylib).unwrap(), version, "a second init is fine");
    }

    #[test]
    fn the_full_version_comes_from_the_build_info() {
        let info = "ORT Build Info: git-branch=rel-1.31.1, git-commit-id=abc, build type=Release";
        assert_eq!(full_version(info, 31), "1.31.1");
        assert_eq!(full_version("ORT Build Info: git-branch=main", 31), "1.31");
    }

    #[test]
    fn default_threads_is_at_least_one() {
        assert!(default_threads() >= 1);
    }
}
