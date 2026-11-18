//! SFace face embeddings: the aligned 112 × 112 crop (RGB 0–255, no
//! normalisation) → 128 values, L2-normalised. Two faces of one person have
//! cosine similarity ≥ 0.363 (OpenCV's threshold).

use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;

use super::align::SIZE;
use crate::clip::{ClipError, normalize};
use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const EMBEDDER_MODEL: &str = "faces/face_recognition_sface_2021dec.onnx";
pub const FACE_DIM: usize = 128;
/// Cosine similarity at or above which two faces are the same person.
pub const SAME_PERSON: f32 = 0.363;

pub struct Embedder {
    session: Session,
}

impl Embedder {
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            session: runtime::session(&models.open_verified(EMBEDDER_MODEL)?, threads)?,
        })
    }

    pub fn embed(&mut self, aligned: &RgbImage) -> Result<Vec<f32>, ClipError> {
        if aligned.dimensions() != (SIZE, SIZE) {
            return Err(ClipError::Inference(
                "the face crop is not 112 × 112".into(),
            ));
        }
        let side = SIZE as usize;
        let input = Array4::from_shape_fn((1, 3, side, side), |(_, c, y, x)| {
            f32::from(aligned.get_pixel(x as u32, y as u32)[c])
        });
        let fail = |err: ort::Error| ClipError::Inference(err.to_string());
        let tensor = TensorRef::from_array_view(input.view()).map_err(fail)?;
        let outputs = self.session.run(ort::inputs![tensor]).map_err(fail)?;
        let (_, values) = outputs[0].try_extract_tensor::<f32>().map_err(fail)?;
        if values.len() != FACE_DIM || values.iter().any(|v| !v.is_finite()) {
            return Err(ClipError::Inference(format!(
                "expected {FACE_DIM} finite values, got {}",
                values.len()
            )));
        }
        Ok(normalize(values))
    }
}
