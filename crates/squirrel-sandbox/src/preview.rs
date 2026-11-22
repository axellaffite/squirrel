//! Previews through the sandbox: one `media-exec` child per render, with
//! libvips when the sandbox's probe found it usable (the built-in pipeline
//! otherwise).

use std::path::PathBuf;

use squirrel_media::exec::{ExecRequest, ExecResponse};
use squirrel_media::preview::{PreviewEncoding, ProbeReport};
use squirrel_media::thumbs::DecodeLimits;
use tokio::sync::OnceCell;

use crate::exec::{ExecError, Executor};

/// One render: the blob, where to write, and how big.
#[derive(Debug, Clone)]
pub struct PreviewJob {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub bucket: u32,
    pub encoding: PreviewEncoding,
    /// The blob is HEIC/HEIF/AVIF (libvips needs its HEIF loader for it).
    pub heif: bool,
}

#[derive(Debug)]
pub struct PreviewRenderer {
    executor: Executor,
    limits: DecodeLimits,
    probe: OnceCell<ProbeReport>,
}

impl PreviewRenderer {
    pub fn new(executor: Executor, limits: DecodeLimits) -> Self {
        Self {
            executor,
            limits,
            probe: OnceCell::new(),
        }
    }

    /// What the sandbox reported about libvips, asked once per process. A
    /// child that cannot answer means libvips is not used.
    pub async fn probe(&self) -> &ProbeReport {
        self.probe
            .get_or_init(|| async {
                let report = match self.executor.run(ExecRequest::Probe).await {
                    Ok(ExecResponse::Probe { report }) => report,
                    Ok(other) => broken(format!("unexpected probe answer {other:?}")),
                    Err(err) => broken(err.to_string()),
                };
                log(&report);
                report
            })
            .await
    }

    pub async fn render(&self, job: PreviewJob) -> Result<ExecResponse, ExecError> {
        let engine = self.probe().await.engine_for(job.heif);
        self.executor
            .run(ExecRequest::Preview {
                input: job.input,
                out_dir: job.out_dir,
                bucket: job.bucket,
                encoding: job.encoding,
                engine,
                limits: self.limits,
            })
            .await
    }
}

fn broken(error: String) -> ProbeReport {
    ProbeReport {
        compiled: squirrel_media::VIPS_COMPILED,
        error: Some(error),
        ..ProbeReport::default()
    }
}

fn log(report: &ProbeReport) {
    match (report.compiled, report.usable()) {
        (false, _) => tracing::info!("previews use the built-in pipeline (built without libvips)"),
        (true, true) => tracing::info!(
            version = report.version.as_deref().unwrap_or("?"),
            loaders = %report.loaders.join(","),
            "previews use libvips"
        ),
        (true, false) => tracing::warn!(
            error = report.error.as_deref().unwrap_or("missing loaders"),
            loaders = %report.loaders.join(","),
            "libvips is unusable: previews use the built-in pipeline (2-3x slower)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_media::preview::PreviewEngine;
    use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};

    #[tokio::test]
    async fn renders_in_process_with_the_probed_engine() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(300, 200, &ExifSpec::default())).unwrap();
        let renderer = PreviewRenderer::new(Executor::InProcess, DecodeLimits::default());
        assert_eq!(
            renderer.probe().await.compiled,
            squirrel_media::VIPS_COMPILED
        );
        let response = renderer
            .render(PreviewJob {
                input,
                out_dir: dir.path().into(),
                bucket: 2160,
                encoding: PreviewEncoding::Display,
                heif: false,
            })
            .await
            .unwrap();
        let ExecResponse::Preview { preview } = response else {
            panic!("unexpected {response:?}");
        };
        assert_eq!((preview.width, preview.height), (300, 200));
        let engine = if squirrel_media::VIPS_COMPILED {
            PreviewEngine::Vips
        } else {
            PreviewEngine::Builtin
        };
        assert_eq!(preview.engine, engine);
    }
}
