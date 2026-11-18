//! The `ml-exec` child's loop: verify and load the requested model groups,
//! say `ready`, then answer one JSON line per request until stdin closes.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::clip::{ClipError, ClipImage, ClipText};
use crate::models::{ModelDir, ModelError, ModelGroup};
use crate::protocol::{
    MAX_REQUEST_LINE, MAX_TEXTS, MlBody, MlOp, MlOutput, MlRequest, MlResponse, Ready,
};
use crate::runtime::{self, RuntimeError};

/// What to load.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub groups: Vec<ModelGroup>,
    pub models: ModelDir,
    /// Required when `groups` is not empty.
    pub ort_dylib: Option<PathBuf>,
    pub threads: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(
        "model groups need ONNX Runtime: set SQUIRREL_ORT_DYLIB (scripts/fetch-onnxruntime.sh)"
    )]
    NoRuntime,
}

/// The loaded models. Text and faces arrive in later milestones: asking for
/// them answers `unsupported`.
pub struct Engine {
    groups: Vec<ModelGroup>,
    onnxruntime: Option<String>,
    clip_image: Option<ClipImage>,
    clip_text: Option<ClipText>,
}

impl Engine {
    /// Verifies every file of every group (SHA-256), then loads them.
    pub fn load(config: &EngineConfig) -> Result<Self, LoadError> {
        for group in &config.groups {
            config.models.verify(*group)?;
        }
        let onnxruntime = if config.groups.is_empty() {
            None
        } else {
            let dylib = config.ort_dylib.as_deref().ok_or(LoadError::NoRuntime)?;
            Some(runtime::init(dylib)?.to_owned())
        };
        let clip_image = if config.groups.contains(&ModelGroup::ClipImage) {
            Some(ClipImage::load(&config.models, config.threads)?)
        } else {
            None
        };
        let clip_text = if config.groups.contains(&ModelGroup::ClipText) {
            Some(ClipText::load(&config.models, config.threads)?)
        } else {
            None
        };
        Ok(Self {
            groups: config.groups.clone(),
            onnxruntime,
            clip_image,
            clip_text,
        })
    }

    pub fn ready(&self) -> Ready {
        Ready {
            ready: true,
            groups: self.groups.iter().map(|g| g.name().to_owned()).collect(),
            onnxruntime: self.onnxruntime.clone(),
        }
    }

    pub fn handle(&mut self, op: &MlOp) -> MlBody {
        match op {
            MlOp::Ping => MlBody::Ok(MlOutput::Pong {}),
            MlOp::ClipImage { input } => match &mut self.clip_image {
                None => not_loaded(ModelGroup::ClipImage),
                Some(clip) => match clip.embed_file(input) {
                    Ok(embedding) => MlBody::Ok(MlOutput::Embedding { embedding }),
                    Err(err) => failure(err),
                },
            },
            MlOp::ClipText { texts } => match &mut self.clip_text {
                None => not_loaded(ModelGroup::ClipText),
                Some(_) if texts.len() > MAX_TEXTS => MlBody::Unsupported {
                    reason: format!("at most {MAX_TEXTS} texts per request"),
                },
                Some(clip) => match texts.iter().map(|t| clip.embed(t)).collect() {
                    Ok(embeddings) => MlBody::Ok(MlOutput::Embeddings { embeddings }),
                    Err(err) => failure(err),
                },
            },
            MlOp::Faces { .. } => not_loaded(ModelGroup::Faces),
        }
    }
}

fn failure(err: ClipError) -> MlBody {
    match err {
        ClipError::Unsupported(reason) => MlBody::Unsupported { reason },
        ClipError::Io(reason) | ClipError::Inference(reason) => MlBody::Failed { reason },
    }
}

fn not_loaded(group: ModelGroup) -> MlBody {
    MlBody::Failed {
        reason: format!("the {} models are not loaded in this child", group.name()),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("reading a request: {0}")]
    Io(#[from] std::io::Error),
    #[error("a request line is longer than {MAX_REQUEST_LINE} bytes")]
    TooLong,
    #[error("malformed request: {0}")]
    Malformed(serde_json::Error),
}

/// Writes the `ready` line, then answers requests until `input` ends. A
/// malformed or oversized line ends the loop with an error: the parent
/// restarts the child rather than guess where the next request starts.
pub fn serve(
    engine: &mut Engine,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), ServeError> {
    write_line(&mut output, &engine.ready())?;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = std::io::Read::take(&mut input, MAX_REQUEST_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        if line.last() != Some(&b'\n') && line.len() > MAX_REQUEST_LINE {
            return Err(ServeError::TooLong);
        }
        let request: MlRequest = serde_json::from_slice(&line).map_err(ServeError::Malformed)?;
        let body = engine.handle(&request.op);
        write_line(
            &mut output,
            &MlResponse {
                id: request.id,
                body,
            },
        )?;
    }
}

fn write_line(output: &mut impl Write, value: &impl serde::Serialize) -> std::io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_engine() -> Engine {
        Engine::load(&EngineConfig {
            groups: Vec::new(),
            models: ModelDir::new("/nonexistent"),
            ort_dylib: None,
            threads: 1,
        })
        .unwrap()
    }

    fn lines(output: &[u8]) -> Vec<serde_json::Value> {
        output
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect()
    }

    #[test]
    fn without_groups_it_answers_ping_without_any_runtime() {
        let mut engine = empty_engine();
        let mut out = Vec::new();
        let input =
            b"{\"id\":1,\"op\":\"ping\"}\n{\"id\":2,\"op\":\"clip_image\",\"input\":\"/x\"}\n";
        serve(&mut engine, &input[..], &mut out).unwrap();
        let lines = lines(&out);
        assert_eq!(lines[0]["ready"], true);
        assert_eq!(lines[1], serde_json::json!({"id": 1, "status": "ok"}));
        assert_eq!(lines[2]["id"], 2);
        assert_eq!(lines[2]["status"], "failed");
    }

    #[test]
    fn a_malformed_line_ends_the_loop() {
        let mut engine = empty_engine();
        let mut out = Vec::new();
        let err = serve(&mut engine, &b"garbage\n"[..], &mut out).unwrap_err();
        assert!(matches!(err, ServeError::Malformed(_)), "{err}");
    }

    #[test]
    fn an_oversized_line_ends_the_loop() {
        let mut engine = empty_engine();
        let mut out = Vec::new();
        let long = vec![b'x'; MAX_REQUEST_LINE + 10];
        let err = serve(&mut engine, &long[..], &mut out).unwrap_err();
        assert!(matches!(err, ServeError::TooLong), "{err}");
    }

    #[test]
    fn groups_without_a_runtime_are_refused() {
        let Some(models) = crate::testutil::test_models() else {
            return;
        };
        let err = Engine::load(&EngineConfig {
            groups: vec![ModelGroup::Faces],
            models,
            ort_dylib: None,
            threads: 1,
        })
        .err()
        .unwrap();
        assert!(matches!(err, LoadError::NoRuntime), "{err}");
    }

    #[test]
    fn a_broken_model_file_stops_the_load() {
        let tmp = tempfile::tempdir().unwrap();
        let err = Engine::load(&EngineConfig {
            groups: vec![ModelGroup::Faces],
            models: ModelDir::new(tmp.path()),
            ort_dylib: None,
            threads: 1,
        })
        .err()
        .unwrap();
        assert!(
            matches!(err, LoadError::Model(ModelError::Missing { .. })),
            "{err}"
        );
    }

    #[test]
    fn clip_image_returns_a_512_d_unit_vector() {
        let Some((dylib, models)) = crate::testutil::test_ml() else {
            return;
        };
        let mut engine = Engine::load(&EngineConfig {
            groups: vec![ModelGroup::ClipImage],
            models,
            ort_dylib: Some(dylib),
            threads: 2,
        })
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("preview.png");
        image::RgbImage::from_fn(320, 240, |x, y| image::Rgb([x as u8, y as u8, 128]))
            .save(&input)
            .unwrap();
        let MlBody::Ok(MlOutput::Embedding { embedding }) =
            engine.handle(&MlOp::ClipImage { input })
        else {
            panic!("no embedding");
        };
        assert_eq!(embedding.len(), 512);
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "{norm}");
    }
}
