//! The CLIP image tower: preview → preprocessed tensor → 512-d unit vector.

use std::path::Path;

use ::image::RgbImage;
use ort::session::Session;
use ort::value::TensorRef;

use super::preprocess::{PreprocessCfg, preprocess};
use super::{ClipError, EMBEDDING_DIM, normalize};
use crate::input::{InputError, MAX_PREVIEW_PIXELS, load_preview};
use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const IMAGE_MODEL: &str = "clip/visual/model.onnx";
pub const PREPROCESS_CFG: &str = "clip/visual/preprocess_cfg.json";

pub struct ClipImage {
    session: Session,
    cfg: PreprocessCfg,
}

impl ClipImage {
    /// Loads the image tower (the models must be verified first).
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        let cfg_path = models.file(PREPROCESS_CFG);
        let invalid = |message: String| RuntimeError::Session {
            path: cfg_path.clone(),
            message,
        };
        let raw = std::fs::read(&cfg_path).map_err(|err| invalid(err.to_string()))?;
        let cfg = PreprocessCfg::parse(&raw).map_err(|err| invalid(err.to_string()))?;
        Ok(Self {
            session: runtime::session(&models.file(IMAGE_MODEL), threads)?,
            cfg,
        })
    }

    /// The embedding of the preview at `path`.
    pub fn embed_file(&mut self, path: &Path) -> Result<Vec<f32>, ClipError> {
        let image = load_preview(path, MAX_PREVIEW_PIXELS).map_err(|err| match err {
            InputError::Io { .. } => ClipError::Io(err.to_string()),
            InputError::Decode { .. } | InputError::TooLarge { .. } => {
                ClipError::Unsupported(err.to_string())
            }
        })?;
        self.embed(&image)
    }

    pub fn embed(&mut self, image: &RgbImage) -> Result<Vec<f32>, ClipError> {
        let input =
            preprocess(image, &self.cfg).map_err(|err| ClipError::Inference(err.to_string()))?;
        let fail = |err: ort::Error| ClipError::Inference(err.to_string());
        let tensor = TensorRef::from_array_view(input.view()).map_err(fail)?;
        let outputs = self.session.run(ort::inputs![tensor]).map_err(fail)?;
        let (_, values) = outputs[0].try_extract_tensor::<f32>().map_err(fail)?;
        if values.len() != EMBEDDING_DIM {
            return Err(ClipError::Inference(format!(
                "expected {EMBEDDING_DIM} values, got {}",
                values.len()
            )));
        }
        Ok(normalize(values))
    }
}
