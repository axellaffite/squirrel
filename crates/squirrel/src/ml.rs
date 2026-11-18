//! Worker start-up for machine learning: which model groups are installed,
//! verified against their pins, and the supervised `ml-exec` child that loads them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::ValueEnum;
use squirrel_ml::models::{ModelDir, ModelGroup};
use squirrel_ml::serve::{Engine, EngineConfig};
use squirrel_worker::context::MlContext;
use squirrel_worker::ml_client::{MlBackend, MlChildConfig, MlClient};

/// Hashing the models (a few hundred MB) happens before `ready`.
pub(crate) const READY_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const MAX_REQUESTS: u32 = 1000;
/// The worker's child frees its memory after this long without a job.
pub const WORKER_IDLE: Duration = Duration::from_secs(10 * 60);
/// The text tower's child only runs `label_bank` jobs: stopped soon after.
pub const TEXT_IDLE: Duration = Duration::from_secs(60);

/// Where model inference runs.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlExecMode {
    /// A long-lived, resource-limited `squirrel ml-exec` child.
    Subprocess,
    /// Inside the worker (tests; an escape hatch, without isolation).
    InProcess,
}

/// What the worker was told about ML.
#[derive(Debug, Clone)]
pub struct MlSettings {
    pub models_dir: PathBuf,
    pub ort_dylib: Option<PathBuf>,
    pub threads: usize,
    pub mode: MlExecMode,
}

/// The groups the worker's child loads (the text tower lives in the server).
const WORKER_GROUPS: [ModelGroup; 2] = [ModelGroup::ClipImage, ModelGroup::Faces];

/// Why the worker runs without ML: a fixed, generic phrase for the UI (the
/// details, with paths, go to the log only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    NotInstalled,
    NoRuntime,
    Mismatch,
    ChildFailed,
}

impl Unavailable {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotInstalled => "models not installed",
            Self::NoRuntime => "ONNX Runtime not configured",
            Self::Mismatch => "model files do not match their pins",
            Self::ChildFailed => "the inference process failed to start",
        }
    }
}

/// A start-up failure: what to tell users, and what to log.
#[derive(Debug, thiserror::Error)]
#[error("{}: {source:#}", why.reason())]
pub struct StartError {
    pub why: Unavailable,
    pub source: anyhow::Error,
}

pub(crate) fn fail(why: Unavailable) -> impl FnOnce(anyhow::Error) -> StartError {
    move |source| StartError { why, source }
}

/// `None` when no model group is installed (logged once). Every installed
/// group is verified first: a file that does not match its pin is refused
/// (an operator error, not a silent downgrade).
pub fn worker_context(settings: &MlSettings) -> Result<Option<MlContext>, StartError> {
    let models = ModelDir::new(&settings.models_dir);
    let groups: Vec<ModelGroup> = WORKER_GROUPS
        .into_iter()
        .filter(|group| models.installed(*group))
        .collect();
    if groups.is_empty() {
        tracing::info!(
            models_dir = %settings.models_dir.display(),
            "CLIP and faces are disabled: run scripts/fetch-models.sh"
        );
        return Ok(None);
    }
    let dylib = ort_dylib(settings.ort_dylib.as_deref()).map_err(fail(Unavailable::NoRuntime))?;
    for group in &groups {
        models
            .verify(*group)
            .with_context(|| format!("verifying the {} models", group.name()))
            .map_err(fail(Unavailable::Mismatch))?;
    }
    let main = backend(settings, &groups, &models, &dylib, WORKER_IDLE)?;
    let mut ml = MlContext::new(
        MlClient::new(main),
        groups.contains(&ModelGroup::ClipImage),
        groups.contains(&ModelGroup::Faces),
    );
    // The text tower only computes the label bank: optional, and its failure
    // only costs the tags.
    if ml.clip && models.installed(ModelGroup::ClipText) {
        let text = models
            .verify(ModelGroup::ClipText)
            .context("verifying the clip-text models")
            .map_err(fail(Unavailable::Mismatch))
            .and_then(|()| {
                backend(
                    settings,
                    &[ModelGroup::ClipText],
                    &models,
                    &dylib,
                    TEXT_IDLE,
                )
            });
        match text {
            Ok(text) => ml = ml.with_text(MlClient::new(text)),
            Err(err) => {
                tracing::warn!(error = %format!("{err:#}"), "no label bank: photos get no tags")
            }
        }
    }
    Ok(Some(ml))
}

/// How a child for `groups` runs: a supervised `ml-exec`, or in this process.
fn backend(
    settings: &MlSettings,
    groups: &[ModelGroup],
    models: &ModelDir,
    dylib: &Path,
    idle: Duration,
) -> Result<MlBackend, StartError> {
    Ok(match settings.mode {
        MlExecMode::Subprocess => MlBackend::Subprocess(MlChildConfig {
            program: std::env::current_exe()
                .context("locating the squirrel executable")
                .map_err(fail(Unavailable::ChildFailed))?,
            args: child_args(groups, models, dylib, settings.threads),
            ready_timeout: READY_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            max_requests: MAX_REQUESTS,
            idle,
            restart_backoff: squirrel_worker::ml_client::RESTART_BACKOFF,
            max_restart_backoff: squirrel_worker::ml_client::MAX_RESTART_BACKOFF,
        }),
        MlExecMode::InProcess => {
            let engine = Engine::load(&EngineConfig {
                groups: groups.to_vec(),
                models: models.clone(),
                ort_dylib: Some(dylib.to_owned()),
                threads: settings.threads,
            })
            .context("loading the models in-process")
            .map_err(fail(Unavailable::ChildFailed))?;
            MlBackend::InProcess(Arc::new(std::sync::Mutex::new(engine)))
        }
    })
}

/// The ONNX Runtime library as an absolute path, required once a model group
/// is installed.
pub(crate) fn ort_dylib(configured: Option<&Path>) -> anyhow::Result<PathBuf> {
    let path = configured.context(
        "models are installed but SQUIRREL_ORT_DYLIB is not set (run scripts/fetch-onnxruntime.sh)",
    )?;
    std::fs::canonicalize(path).with_context(|| {
        format!(
            "SQUIRREL_ORT_DYLIB {} (run scripts/fetch-onnxruntime.sh)",
            path.display()
        )
    })
}

pub(crate) fn child_args(
    groups: &[ModelGroup],
    models: &ModelDir,
    dylib: &Path,
    threads: usize,
) -> Vec<OsString> {
    let groups: Vec<&str> = groups.iter().map(|group| group.name()).collect();
    vec![
        "--groups".into(),
        groups.join(",").into(),
        "--models-dir".into(),
        models.path().into(),
        "--ort-dylib".into(),
        dylib.into(),
        "--threads".into(),
        threads.max(1).to_string().into(),
        "--max-memory-mb".into(),
        crate::ml_exec::DEFAULT_MEMORY_MB.to_string().into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_ml::models::PINNED;

    fn settings(dir: &Path, ort: Option<PathBuf>) -> MlSettings {
        MlSettings {
            models_dir: dir.to_owned(),
            ort_dylib: ort,
            threads: 1,
            mode: MlExecMode::Subprocess,
        }
    }

    /// Files of the pinned sizes but the wrong bytes: "installed", never verified.
    fn fake_group(dir: &Path, group: ModelGroup) {
        for file in PINNED.iter().filter(|f| f.groups.contains(&group)) {
            let path = ModelDir::new(dir).file(file.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let handle = std::fs::File::create(&path).unwrap();
            handle.set_len(file.size).unwrap();
        }
    }

    #[test]
    fn without_models_ml_is_off() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            worker_context(&settings(dir.path(), None))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn installed_models_need_the_runtime() {
        let dir = tempfile::tempdir().unwrap();
        fake_group(dir.path(), ModelGroup::Faces);
        let err = worker_context(&settings(dir.path(), None)).err().unwrap();
        assert_eq!(err.why, Unavailable::NoRuntime);
        assert!(format!("{err:#}").contains("fetch-onnxruntime"), "{err:#}");
    }

    #[test]
    fn a_model_that_fails_verification_stops_the_worker_with_its_name() {
        let dir = tempfile::tempdir().unwrap();
        fake_group(dir.path(), ModelGroup::Faces);
        let ort = dir.path().join("libonnxruntime.so");
        std::fs::write(&ort, b"").unwrap();
        let err = worker_context(&settings(dir.path(), Some(ort)))
            .err()
            .unwrap();
        assert_eq!(err.why, Unavailable::Mismatch);
        assert!(!err.why.reason().contains('/'), "no path for users");
        let message = format!("{err:#}");
        assert!(message.contains("faces"), "{message}");
        assert!(
            message.contains(".onnx") || message.contains("LICENSE"),
            "{message}"
        );
        assert!(message.contains("SHA-256"), "{message}");
    }

    /// With the real models: only the installed groups are loaded.
    #[test]
    fn only_the_installed_groups_are_enabled() {
        let Some(real) = squirrel_ml::testutil::test_models_for(&[ModelGroup::Faces]) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        for file in PINNED
            .iter()
            .filter(|f| f.groups.contains(&ModelGroup::Faces))
        {
            let target = ModelDir::new(dir.path()).file(file.path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(real.file(file.path), target).unwrap();
        }
        let ort = dir.path().join("libonnxruntime.so");
        std::fs::write(&ort, b"").unwrap();
        let ml = worker_context(&settings(dir.path(), Some(ort)))
            .unwrap()
            .unwrap();
        assert!(ml.faces && !ml.clip);
        assert_eq!(ml.kinds(), ["faces", "cluster_faces", "face_backfill"]);
    }

    #[test]
    fn the_child_gets_its_groups_and_paths() {
        let args = child_args(
            &[ModelGroup::ClipImage, ModelGroup::Faces],
            &ModelDir::new("/m"),
            Path::new("/o/lib.so"),
            0,
        );
        let args: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(&args[..2], ["--groups", "clip-image,faces"]);
        assert!(args.windows(2).any(|w| w == ["--threads", "1"]));
        assert!(args.windows(2).any(|w| w == ["--ort-dylib", "/o/lib.so"]));
    }
}
