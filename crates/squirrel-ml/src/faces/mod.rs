//! Faces: YuNet detection, 5-point alignment and SFace embeddings (OpenCV
//! Zoo models, `yunet-sface-2021dec`).

pub mod align;
pub mod detect;
pub mod embed;

use std::path::Path;

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, RgbImage};
use serde::{Deserialize, Serialize};

pub use detect::{Detection, Detector};
pub use embed::{Embedder, FACE_DIM, SAME_PERSON};

use crate::clip::ClipError;
use crate::input::{InputError, MAX_PREVIEW_PIXELS, load_preview};
use crate::models::ModelDir;
use crate::runtime::RuntimeError;

/// YuNet sees faces of ~10–300 px: previews are downscaled to this long side.
pub const DETECT_SIDE: u32 = 640;
/// Faces smaller than this (short side, in preview pixels) are too small to
/// recognise reliably: dropped.
pub const MIN_FACE_PX: f32 = 40.0;
/// At most this many faces per photo.
pub const MAX_FACES: usize = 50;

/// One face of a photo, relative to the preview (0..1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceFound {
    /// Left, top, width, height.
    pub bbox: [f32; 4],
    /// Five (x, y) points: eyes, nose tip, mouth corners (image left first).
    pub landmarks: [f32; 10],
    pub score: f32,
    /// SFace, unit length.
    pub embedding: Vec<f32>,
}

pub struct FaceAnalyzer {
    detector: Detector,
    embedder: Embedder,
}

impl FaceAnalyzer {
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            detector: Detector::load(models, threads)?,
            embedder: Embedder::load(models, threads)?,
        })
    }

    pub fn analyze_file(&mut self, path: &Path) -> Result<Vec<FaceFound>, ClipError> {
        let preview = load_preview(path, MAX_PREVIEW_PIXELS).map_err(|err| match err {
            InputError::Io { .. } => ClipError::Io(err.to_string()),
            InputError::Decode { .. } | InputError::TooLarge { .. } => {
                ClipError::Unsupported(err.to_string())
            }
        })?;
        self.analyze(&preview)
    }

    /// Detects on a ≤ 640 px copy, then aligns and embeds each face from the
    /// full preview.
    pub fn analyze(&mut self, preview: &RgbImage) -> Result<Vec<FaceFound>, ClipError> {
        let (width, height) = preview.dimensions();
        if width == 0 || height == 0 {
            return Err(ClipError::Unsupported("empty preview".into()));
        }
        let scale = (f64::from(DETECT_SIDE) / f64::from(width.max(height))).min(1.0);
        let small = downscale(preview, scale)?;
        let back = (1.0 / scale) as f32;
        let mut found = Vec::new();
        for detection in self.detector.detect(&small)?.into_iter().take(MAX_FACES) {
            let [x, y, w, h] = detection.bbox.map(|v| v * back);
            if w.min(h) < MIN_FACE_PX {
                continue;
            }
            let landmarks = detection.landmarks.map(|[lx, ly]| [lx * back, ly * back]);
            let Some(aligned) = align::align(preview, &landmarks) else {
                continue;
            };
            let embedding = self.embedder.embed(&aligned)?;
            let (fw, fh) = (width as f32, height as f32);
            let left = x.clamp(0.0, fw);
            let top = y.clamp(0.0, fh);
            found.push(FaceFound {
                bbox: [
                    left / fw,
                    top / fh,
                    ((x + w).clamp(0.0, fw) - left) / fw,
                    ((y + h).clamp(0.0, fh) - top) / fh,
                ],
                landmarks: std::array::from_fn(|i| {
                    let point = landmarks[i / 2];
                    if i % 2 == 0 {
                        point[0] / fw
                    } else {
                        point[1] / fh
                    }
                }),
                score: detection.score,
                embedding,
            });
        }
        Ok(found)
    }
}

fn downscale(image: &RgbImage, scale: f64) -> Result<RgbImage, ClipError> {
    if scale >= 1.0 {
        return Ok(image.clone());
    }
    let w = ((f64::from(image.width()) * scale).round() as u32).max(1);
    let h = ((f64::from(image.height()) * scale).round() as u32).max(1);
    let src = DynamicImage::ImageRgb8(image.clone());
    let mut dst = DynamicImage::new_rgb8(w, h);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|err| ClipError::Inference(err.to_string()))?;
    Ok(dst.into_rgb8())
}

#[cfg(test)]
mod tests;
