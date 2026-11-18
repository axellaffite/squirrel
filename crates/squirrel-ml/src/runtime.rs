//! The pinned ONNX Runtime, loaded at run time (`scripts/fetch-onnxruntime.sh`),
//! and sessions built on it. CPU only.

use crate::models::VerifiedFile;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

/// The ONNX Runtime minor version the fetch script pins (1.31.x).
pub const ORT_MINOR: u32 = 31;
/// The exact build the library pins below identify (its build info only says
/// `git-branch=HEAD`).
pub const ORT_VERSION: &str = "1.31.1";

/// SHA-256 of `libonnxruntime` 1.31.1 per platform, as extracted by
/// `scripts/fetch-onnxruntime.sh` (a test keeps both lists equal). Checked
/// before the library is loaded: it is native code run in-process.
pub const PINNED_LIBRARIES: [&str; 3] = [
    // linux-x64
    "a96f132e5eb5ca04e23a7d2fc1078fb95aa591e8d71d0f237702ece3a1e0f4ea",
    // linux-arm64
    "7e17c55d675be73f67a218dbecb55522395f61dafb3da03ebc7ade56a4c2f813",
    // osx-arm64
    "2685dc296147c47b0dbd8e0615547b12fc0c445d158d1f374cbd0705d9626fdc",
];

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// A model file read for loading does not match its pin.
    #[error(transparent)]
    Model(#[from] crate::models::ModelError),
    #[error("ONNX Runtime library {path} not found (run scripts/fetch-onnxruntime.sh)")]
    Missing { path: PathBuf },
    #[error(
        "{path} is not ONNX Runtime 1.{ORT_MINOR}.x (found {found}); run scripts/fetch-onnxruntime.sh"
    )]
    Version { path: PathBuf, found: String },
    #[error(
        "{path} is not the pinned ONNX Runtime build (SHA-256 {actual}); run scripts/fetch-onnxruntime.sh"
    )]
    Unpinned { path: PathBuf, actual: String },
    #[error("ONNX Runtime is already loaded from {loaded}, not {path}")]
    OtherLibrary { loaded: PathBuf, path: PathBuf },
    #[error("loading ONNX Runtime from {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("loading model {path}: {message}")]
    Session { path: PathBuf, message: String },
}

struct Loaded {
    path: PathBuf,
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
    check_pinned(&path)?;
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
    let _ = LOADED.set(Loaded { path, environment });
    Ok(version())
}

/// Refuses a library whose SHA-256 is not one of [`PINNED_LIBRARIES`].
fn check_pinned(path: &Path) -> Result<(), RuntimeError> {
    let actual = std::fs::File::open(path)
        .and_then(|mut file| crate::models::sha256_hex(&mut file))
        .map_err(|err| RuntimeError::Load {
            path: path.to_owned(),
            message: err.to_string(),
        })?;
    if PINNED_LIBRARIES.contains(&actual.as_str()) {
        Ok(())
    } else {
        Err(RuntimeError::Unpinned {
            path: path.to_owned(),
            actual,
        })
    }
}

/// The loaded version: the hash pin identifies the build exactly.
fn version() -> &'static str {
    ORT_VERSION
}

/// A CPU session on the loaded runtime, loaded by ONNX Runtime from a model
/// file already verified through its open descriptor
/// (`ModelDir::open_verified`): `intra_threads` threads for one inference, one
/// inter-op thread, full graph optimisation.
pub fn session(model: &VerifiedFile, intra_threads: usize) -> Result<Session, RuntimeError> {
    let fail = |message: String| RuntimeError::Session {
        path: model.path().to_owned(),
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
        .commit_from_file(model.load_path())
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
                RuntimeError::Unpinned { .. } | RuntimeError::OtherLibrary { .. }
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
    fn the_fetch_script_pins_the_same_libraries() {
        let script = include_str!("../../../scripts/fetch-onnxruntime.sh");
        let mut listed: Vec<&str> = script
            .split("LIB_SHA=")
            .skip(1)
            .filter_map(|rest| rest.get(..64))
            .filter(|sha| sha.bytes().all(|b| b.is_ascii_hexdigit()))
            .collect();
        let mut pinned = PINNED_LIBRARIES.to_vec();
        listed.sort_unstable();
        pinned.sort_unstable();
        assert_eq!(listed, pinned);
    }

    #[test]
    fn default_threads_is_at_least_one() {
        assert!(default_threads() >= 1);
    }
}
