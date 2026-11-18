//! Worker start-up for machine learning: which model groups are installed,
//! verified against their pins, and the supervised `ml-exec` child that loads them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::ValueEnum;
use sqlx::PgPool;
use squirrel_ml::models::{ModelDir, ModelGroup};
use squirrel_ml::serve::{Engine, EngineConfig};
use squirrel_worker::context::MlContext;
use squirrel_worker::ml_client::{MlBackend, MlChildConfig, MlClient};

/// Hashing the models (a few hundred MB) happens before `ready`.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REQUESTS: u32 = 1000;
/// The worker's child frees its memory after this long without a job.
pub const WORKER_IDLE: Duration = Duration::from_secs(10 * 60);

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

/// The worker's ML at start: `None` without models. Verifies the installed
/// groups, pings the child, then makes sure the label bank of the current
/// labels version is stored (computed once by a short-lived `clip-text`
/// child; without the text tower photos get embeddings but no tags).
pub async fn start(settings: MlSettings, pool: &PgPool) -> anyhow::Result<Option<Arc<MlContext>>> {
    let worker_settings = settings.clone();
    let Some(ml) = tokio::task::spawn_blocking(move || worker_context(&worker_settings))
        .await
        .context("verifying the models")??
    else {
        return Ok(None);
    };
    let ready = ml.client.health().await.context("starting the ML child")?;
    tracing::info!(
        groups = ?ready.groups,
        kinds = ?ml.kinds(),
        "ML: {} ready (ONNX Runtime {})",
        ready.groups.join(", "),
        ready.onnxruntime.as_deref().unwrap_or("not loaded")
    );
    if ml.clip
        && let Err(err) = ensure_label_bank(&settings, pool).await
    {
        tracing::warn!(error = %format!("{err:#}"), "no label bank: photos get no tags");
    }
    Ok(Some(Arc::new(ml)))
}

/// Computes the label bank once (through its own `clip-text` child, stopped
/// afterwards), when the text tower is installed and the bank is missing.
async fn ensure_label_bank(settings: &MlSettings, pool: &PgPool) -> anyhow::Result<()> {
    let models = ModelDir::new(&settings.models_dir);
    if !models.installed(ModelGroup::ClipText) {
        tracing::info!("the CLIP text model is not installed: photos get no tags");
        return Ok(());
    }
    let labels = squirrel_ml::clip::labels::bank();
    let stored = squirrel_db::clip::label_embeddings(
        pool,
        squirrel_ml::CLIP_MODEL_ID,
        squirrel_ml::clip::labels::LABELS_VERSION,
    )
    .await?;
    if stored.len() == labels.len() {
        return Ok(());
    }
    let text_models = models.clone();
    tokio::task::spawn_blocking(move || text_models.verify(ModelGroup::ClipText))
        .await
        .context("verifying the text model")??;
    let dylib = ort_dylib(settings.ort_dylib.as_deref())?;
    let groups = [ModelGroup::ClipText];
    let backend = match settings.mode {
        MlExecMode::Subprocess => MlBackend::Subprocess(MlChildConfig {
            program: std::env::current_exe().context("locating the squirrel executable")?,
            args: child_args(&groups, &models, &dylib, settings.threads),
            ready_timeout: READY_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            max_requests: MAX_REQUESTS,
            idle: WORKER_IDLE,
        }),
        MlExecMode::InProcess => {
            let engine = Engine::load(&EngineConfig {
                groups: groups.to_vec(),
                models,
                ort_dylib: Some(dylib),
                threads: settings.threads,
            })?;
            MlBackend::InProcess(Arc::new(std::sync::Mutex::new(engine)))
        }
    };
    let client = MlClient::new(backend);
    let result =
        squirrel_worker::label_bank::ensure(pool, &client, squirrel_ml::CLIP_MODEL_ID).await;
    client.stop().await;
    result.map(drop)
}

/// The groups the worker's child loads (the text tower lives in the server).
const WORKER_GROUPS: [ModelGroup; 2] = [ModelGroup::ClipImage, ModelGroup::Faces];

/// `None` when no model group is installed (logged once). Every installed
/// group is verified first: a file that does not match its pin stops the
/// worker (an operator error, not a silent downgrade).
pub fn worker_context(settings: &MlSettings) -> anyhow::Result<Option<MlContext>> {
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
    let dylib = ort_dylib(settings.ort_dylib.as_deref())?;
    for group in &groups {
        models
            .verify(*group)
            .with_context(|| format!("verifying the {} models", group.name()))?;
    }
    let backend = match settings.mode {
        MlExecMode::Subprocess => MlBackend::Subprocess(MlChildConfig {
            program: std::env::current_exe().context("locating the squirrel executable")?,
            args: child_args(&groups, &models, &dylib, settings.threads),
            ready_timeout: READY_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            max_requests: MAX_REQUESTS,
            idle: WORKER_IDLE,
        }),
        MlExecMode::InProcess => {
            let engine = Engine::load(&EngineConfig {
                groups: groups.clone(),
                models,
                ort_dylib: Some(dylib),
                threads: settings.threads,
            })
            .context("loading the models in-process")?;
            MlBackend::InProcess(Arc::new(std::sync::Mutex::new(engine)))
        }
    };
    Ok(Some(MlContext::new(
        MlClient::new(backend),
        groups.contains(&ModelGroup::ClipImage),
        groups.contains(&ModelGroup::Faces),
    )))
}

/// The ONNX Runtime library as an absolute path, required once a model group
/// is installed.
fn ort_dylib(configured: Option<&Path>) -> anyhow::Result<PathBuf> {
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

fn child_args(
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
        let Some(real) = squirrel_ml::testutil::test_models() else {
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
