//! The worker's machine learning at start: verify, start the child, store the
//! label bank, and report the outcome (`ml_worker_status`) for the UI. A
//! failure never stops the worker unless `--kinds` asked for ML explicitly.

use std::sync::Arc;

use anyhow::Context;
use sqlx::PgPool;
use squirrel_db::ml_status::{self, MlState};
use squirrel_worker::context::MlContext;

use crate::ml::{MlSettings, StartError, Unavailable, fail, worker_context};

/// What this worker does about ML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlWanted {
    /// Its kinds exclude every ML kind: nothing to start or report.
    No,
    /// The default: ML when it can, otherwise logged and reported.
    IfAvailable,
    /// `--kinds` names ML kinds: no ML is a start-up error.
    Required,
}

/// `None` when ML is off (not wanted, not installed, or failed to start).
pub async fn start(
    settings: MlSettings,
    pool: &PgPool,
    host: &str,
    wanted: MlWanted,
) -> anyhow::Result<Option<Arc<MlContext>>> {
    if wanted == MlWanted::No {
        return Ok(None);
    }
    let outcome = try_start(&settings, pool).await;
    let (state, why) = match &outcome {
        Ok(Some(_)) => (MlState::Ready, None),
        Ok(None) => (MlState::Unavailable, Some(Unavailable::NotInstalled)),
        Err(err) => (MlState::Unavailable, Some(err.why)),
    };
    if let Err(err) = ml_status::report(pool, host, state, why.map(Unavailable::reason)).await {
        tracing::warn!(error = %err, "could not record the ML status");
    }
    match outcome {
        Ok(Some(ml)) => Ok(Some(ml)),
        Ok(None) if wanted == MlWanted::Required => {
            anyhow::bail!("--kinds names ML job kinds but no models are installed")
        }
        Ok(None) => Ok(None),
        Err(err) if wanted == MlWanted::Required => {
            Err(anyhow::Error::new(err).context("--kinds names ML job kinds"))
        }
        Err(err) => {
            tracing::error!(
                error = %format!("{err:#}"),
                "machine learning is off: its jobs wait for a worker that can run them"
            );
            Ok(None)
        }
    }
}

async fn try_start(
    settings: &MlSettings,
    pool: &PgPool,
) -> Result<Option<Arc<MlContext>>, StartError> {
    let worker_settings = settings.clone();
    let Some(ml) = tokio::task::spawn_blocking(move || worker_context(&worker_settings))
        .await
        .context("verifying the models")
        .map_err(fail(Unavailable::Mismatch))??
    else {
        return Ok(None);
    };
    let ready = ml
        .client
        .health()
        .await
        .context("starting the ML child")
        .map_err(fail(Unavailable::ChildFailed))?;
    // Checked, not kept: the first job starts it again (its memory is only
    // spent while there is work).
    ml.client.stop().await;
    tracing::info!(
        groups = ?ready.groups,
        kinds = ?ml.kinds(),
        "ML: {} ready (ONNX Runtime {})",
        ready.groups.join(", "),
        ready.onnxruntime.as_deref().unwrap_or("not loaded")
    );
    if let Err(err) = queue_label_bank(pool, &ml).await {
        tracing::warn!(error = %format!("{err:#}"), "could not queue the label bank");
    }
    Ok(Some(Arc::new(ml)))
}

/// Queues a `label_bank` job when the bank is missing or stale and this
/// worker can compute it; never waits for it (tags follow once it is done).
async fn queue_label_bank(pool: &PgPool, ml: &MlContext) -> anyhow::Result<()> {
    if ml.text.is_none() {
        tracing::info!("the CLIP text model is not installed: photos get no tags");
        return Ok(());
    }
    if squirrel_worker::label_bank::missing(pool, squirrel_ml::CLIP_MODEL_ID).await? {
        let mut conn = pool.acquire().await?;
        squirrel_jobs::enqueue(
            &mut conn,
            &squirrel_worker::jobs::label_bank(squirrel_ml::CLIP_MODEL_ID),
        )
        .await?;
        tracing::info!("label bank queued: tags follow once it is computed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::ml::MlExecMode;
    use squirrel_ml::models::{ModelDir, ModelGroup, PINNED};

    fn settings(dir: &Path, ort: Option<std::path::PathBuf>) -> MlSettings {
        MlSettings {
            models_dir: dir.to_owned(),
            ort_dylib: ort,
            threads: 1,
            mode: MlExecMode::Subprocess,
        }
    }

    /// The faces group with the pinned sizes but the wrong bytes.
    fn broken_faces(dir: &Path) {
        for file in PINNED
            .iter()
            .filter(|f| f.groups.contains(&ModelGroup::Faces))
        {
            let path = ModelDir::new(dir).file(file.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path)
                .unwrap()
                .set_len(file.size)
                .unwrap();
        }
    }

    async fn status(pool: &PgPool) -> ml_status::MlStatus {
        ml_status::current(pool).await.unwrap()
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn a_broken_install_turns_ml_off_but_keeps_the_worker(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        broken_faces(dir.path());
        let ort = dir.path().join("libonnxruntime.so");
        std::fs::write(&ort, b"").unwrap();
        let ml = start(
            settings(dir.path(), Some(ort.clone())),
            &pool,
            "box",
            MlWanted::IfAvailable,
        )
        .await
        .unwrap();
        assert!(ml.is_none());
        let status = status(&pool).await;
        assert_eq!(status.state, MlState::Unavailable);
        assert_eq!(
            status.reason.as_deref(),
            Some(Unavailable::Mismatch.reason())
        );
        let required = start(
            settings(dir.path(), Some(ort)),
            &pool,
            "box",
            MlWanted::Required,
        )
        .await;
        let Err(err) = required else {
            panic!("required ML must fail");
        };
        assert!(format!("{err:#}").contains("--kinds"), "{err:#}");
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn no_models_is_reported_and_only_fatal_when_required(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let ml = start(
            settings(dir.path(), None),
            &pool,
            "box",
            MlWanted::IfAvailable,
        )
        .await
        .unwrap();
        assert!(ml.is_none());
        assert_eq!(
            status(&pool).await.reason.as_deref(),
            Some(Unavailable::NotInstalled.reason())
        );
        assert!(
            start(settings(dir.path(), None), &pool, "box", MlWanted::Required)
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn a_worker_without_ml_kinds_reports_nothing(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let ml = start(settings(dir.path(), None), &pool, "box", MlWanted::No)
            .await
            .unwrap();
        assert!(ml.is_none());
        assert_eq!(
            status(&pool).await.reason.as_deref(),
            Some(ml_status::NO_REPORT)
        );
    }
}
