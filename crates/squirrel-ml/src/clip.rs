//! CLIP ViT-B/32 image encoder: preview → 224 × 224 tensor → 512-d unit vector.
//! Preprocessing follows `clip/visual/preprocess_cfg.json`: shortest side to
//! 224 (bicubic), centre crop, CLIP mean/std. (Task 8 pins it with a golden test.)

use std::path::Path;

use image::imageops::FilterType;
use image::{ImageReader, Limits, RgbImage};
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;

use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const IMAGE_MODEL: &str = "clip/visual/model.onnx";
pub const EMBEDDING_DIM: usize = 512;
const SIZE: u32 = 224;
// Copied digit for digit from preprocess_cfg.json, beyond f32 precision.
#[allow(clippy::excessive_precision)]
const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
#[allow(clippy::excessive_precision)]
const STD: [f32; 3] = [0.268_629_54, 0.261_302_58, 0.275_777_11];
/// Previews are at most 1440 px; anything far larger is not ours.
const MAX_SIDE: u32 = 8192;
const MAX_ALLOC: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    /// The input cannot be decoded: not worth a retry.
    #[error("cannot decode {0}")]
    Decode(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

pub struct ClipImage {
    session: Session,
}

impl ClipImage {
    /// Loads the image tower (the models must be verified first).
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            session: runtime::session(&models.file(IMAGE_MODEL), threads)?,
        })
    }

    /// The L2-normalised embedding of the image at `path`.
    pub fn embed_file(&mut self, path: &Path) -> Result<Vec<f32>, ClipError> {
        let image = decode(path)?;
        self.embed(&image)
    }

    pub fn embed(&mut self, image: &RgbImage) -> Result<Vec<f32>, ClipError> {
        let input = preprocess(image);
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

fn decode(path: &Path) -> Result<RgbImage, ClipError> {
    let fail =
        |err: &dyn std::fmt::Display| ClipError::Decode(format!("{}: {err}", path.display()));
    let mut reader = ImageReader::open(path)
        .map_err(|err| fail(&err))?
        .with_guessed_format()
        .map_err(|err| fail(&err))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    Ok(reader.decode().map_err(|err| fail(&err))?.to_rgb8())
}

/// Shortest side to 224 (bicubic), centre crop 224 × 224, normalised NCHW.
pub fn preprocess(image: &RgbImage) -> Array4<f32> {
    let (width, height) = image.dimensions();
    let scale = f64::from(SIZE) / f64::from(width.min(height).max(1));
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(SIZE);
    let (new_w, new_h) = (scaled(width), scaled(height));
    let resized = image::imageops::resize(image, new_w, new_h, FilterType::CatmullRom);
    let (left, top) = ((new_w - SIZE) / 2, (new_h - SIZE) / 2);
    let crop = image::imageops::crop_imm(&resized, left, top, SIZE, SIZE).to_image();
    Array4::from_shape_fn((1, 3, SIZE as usize, SIZE as usize), |(_, c, y, x)| {
        let value = f32::from(crop.get_pixel(x as u32, y as u32)[c]) / 255.0;
        (value - MEAN[c]) / STD[c]
    })
}

fn normalize(values: &[f32]) -> Vec<f32> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return values.to_vec();
    }
    values.iter().map(|v| v / norm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocessing_crops_the_centre_to_224() {
        let image = RgbImage::from_fn(448, 300, |x, _| {
            if x < 40 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let tensor = preprocess(&image);
        assert_eq!(tensor.shape(), [1, 3, 224, 224]);
        // The red band (x < 40 of 448) is cut off by the centre crop.
        let blue = (1.0 - MEAN[2]) / STD[2];
        assert!((tensor[[0, 2, 112, 0]] - blue).abs() < 1e-3);
    }

    #[test]
    fn embeddings_are_unit_vectors() {
        let unit = normalize(&[3.0, 4.0]);
        assert_eq!(unit, [0.6, 0.8]);
        assert_eq!(normalize(&[0.0, 0.0]), [0.0, 0.0]);
    }

    #[test]
    fn an_undecodable_file_is_a_decode_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.webp");
        std::fs::write(&path, b"nope").unwrap();
        assert!(matches!(decode(&path), Err(ClipError::Decode(_))));
    }
}
