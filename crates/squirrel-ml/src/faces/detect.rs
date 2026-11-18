//! YuNet face detection (OpenCV Zoo, `face_detection_yunet_2026may.onnx`):
//! BGR 0–255 input padded to a multiple of 32; per stride (8, 16, 32) the
//! model gives class and objectness scores, a box and 5 landmarks per cell,
//! decoded as OpenCV's `face_detect.cpp` does, then non-maximum suppression.

use image::RgbImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;

use crate::clip::ClipError;
use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const DETECTOR_MODEL: &str = "faces/face_detection_yunet_2026may.onnx";
pub const SCORE_THRESHOLD: f32 = 0.6;
pub const NMS_IOU: f32 = 0.3;
pub const TOP_K: usize = 50;
const STRIDES: [usize; 3] = [8, 16, 32];

/// One face, in the detector input's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub score: f32,
    /// Left, top, width, height.
    pub bbox: [f32; 4],
    /// The image-left eye, the image-right eye, the nose tip, the image-left
    /// and image-right mouth corners (the ArcFace template's order).
    pub landmarks: [[f32; 2]; 5],
}

/// The raw outputs of one stride: per cell, `cls`, `obj`, 4 box values and
/// 10 landmark values.
pub struct StrideOutput<'a> {
    pub stride: usize,
    pub cls: &'a [f32],
    pub obj: &'a [f32],
    pub bbox: &'a [f32],
    pub kps: &'a [f32],
}

pub struct Detector {
    session: Session,
}

impl Detector {
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        Ok(Self {
            session: runtime::session(&models.open_verified(DETECTOR_MODEL)?, threads)?,
        })
    }

    /// The faces of `image` (any size: the caller downscales first), best first.
    pub fn detect(&mut self, image: &RgbImage) -> Result<Vec<Detection>, ClipError> {
        let (input, pad_w, pad_h) = to_input(image);
        let fail = |err: ort::Error| ClipError::Inference(err.to_string());
        let tensor = TensorRef::from_array_view(input.view()).map_err(fail)?;
        let outputs = self
            .session
            .run(ort::inputs!["input" => tensor])
            .map_err(fail)?;
        let mut found = Vec::new();
        for stride in STRIDES {
            let get = |name: &str| -> Result<&[f32], ClipError> {
                let value = outputs
                    .get(format!("{name}_{stride}"))
                    .ok_or_else(|| ClipError::Inference(format!("no output {name}_{stride}")))?;
                Ok(value.try_extract_tensor::<f32>().map_err(fail)?.1)
            };
            let output = StrideOutput {
                stride,
                cls: get("cls")?,
                obj: get("obj")?,
                bbox: get("bbox")?,
                kps: get("kps")?,
            };
            found.extend(decode(&output, pad_w, pad_h, SCORE_THRESHOLD)?);
        }
        Ok(nms(found, NMS_IOU, TOP_K))
    }
}

/// NCHW BGR 0–255, zero-padded on the right and bottom to multiples of 32.
fn to_input(image: &RgbImage) -> (Array4<f32>, usize, usize) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let pad = |v: usize| v.div_ceil(32).max(1) * 32;
    let (pad_w, pad_h) = (pad(w), pad(h));
    let input = Array4::from_shape_fn((1, 3, pad_h, pad_w), |(_, c, y, x)| {
        if x < w && y < h {
            // Channel 0 is blue.
            f32::from(image.get_pixel(x as u32, y as u32)[2 - c])
        } else {
            0.0
        }
    });
    (input, pad_w, pad_h)
}

/// The detections of one stride over a `pad_w` × `pad_h` input.
pub fn decode(
    output: &StrideOutput<'_>,
    pad_w: usize,
    pad_h: usize,
    threshold: f32,
) -> Result<Vec<Detection>, ClipError> {
    let stride = output.stride;
    let (cols, rows) = (pad_w / stride, pad_h / stride);
    let cells = cols * rows;
    if output.cls.len() < cells
        || output.obj.len() < cells
        || output.bbox.len() < cells * 4
        || output.kps.len() < cells * 10
    {
        return Err(ClipError::Inference(format!(
            "stride {stride}: outputs shorter than {cells} cells"
        )));
    }
    let s = stride as f32;
    let mut found = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let i = row * cols + col;
            let score = (output.cls[i].clamp(0.0, 1.0) * output.obj[i].clamp(0.0, 1.0)).sqrt();
            if score < threshold {
                continue;
            }
            let b = &output.bbox[i * 4..i * 4 + 4];
            let (cx, cy) = ((col as f32 + b[0]) * s, (row as f32 + b[1]) * s);
            let (w, h) = (b[2].exp() * s, b[3].exp() * s);
            let k = &output.kps[i * 10..i * 10 + 10];
            let landmarks = std::array::from_fn(|n| {
                [(k[2 * n] + col as f32) * s, (k[2 * n + 1] + row as f32) * s]
            });
            found.push(Detection {
                score,
                bbox: [cx - w / 2.0, cy - h / 2.0, w, h],
                landmarks,
            });
        }
    }
    Ok(found)
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = a[2] * a[3] + b[2] * b[3] - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Greedy non-maximum suppression: best score first, dropping boxes that
/// overlap a kept one by more than `threshold`; at most `top_k`.
pub fn nms(mut found: Vec<Detection>, threshold: f32, top_k: usize) -> Vec<Detection> {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for detection in found {
        if kept.len() == top_k {
            break;
        }
        if kept
            .iter()
            .all(|k| iou(&k.bbox, &detection.bbox) <= threshold)
        {
            kept.push(detection);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64 × 32 input at stride 32: two cells; only the second scores.
    #[test]
    fn decoding_follows_opencv() {
        let cls = [0.1, 0.81];
        let obj = [0.1, 1.0];
        let bbox = [0.0, 0.0, 0.0, 0.0, 0.5, 0.25, 0.0, 1.0f32.ln()];
        let mut kps = [0.0f32; 20];
        kps[10..20].copy_from_slice(&[0.25, 0.25, 0.75, 0.25, 0.5, 0.5, 0.25, 0.75, 0.75, 0.75]);
        let output = StrideOutput {
            stride: 32,
            cls: &cls,
            obj: &obj,
            bbox: &bbox,
            kps: &kps,
        };
        let found = decode(&output, 64, 32, 0.6).unwrap();
        assert_eq!(found.len(), 1);
        let d = &found[0];
        assert!((d.score - 0.9).abs() < 1e-6);
        // Centre ((1 + 0.5) · 32, (0 + 0.25) · 32) = (48, 8), size 32 × 32.
        assert_eq!(d.bbox, [32.0, -8.0, 32.0, 32.0]);
        assert_eq!(d.landmarks[0], [40.0, 8.0]);
        assert_eq!(d.landmarks[4], [56.0, 24.0]);
        let short = StrideOutput {
            cls: &cls[..1],
            ..output
        };
        assert!(decode(&short, 64, 32, 0.6).is_err());
    }

    fn det(score: f32, x: f32) -> Detection {
        Detection {
            score,
            bbox: [x, 0.0, 10.0, 10.0],
            landmarks: [[0.0; 2]; 5],
        }
    }

    #[test]
    fn nms_keeps_the_better_of_overlapping_boxes() {
        let kept = nms(vec![det(0.7, 1.0), det(0.9, 0.0), det(0.8, 50.0)], 0.3, 50);
        let scores: Vec<f32> = kept.iter().map(|d| d.score).collect();
        assert_eq!(scores, [0.9, 0.8]);
        assert_eq!(nms(vec![det(0.9, 0.0), det(0.8, 50.0)], 0.3, 1).len(), 1);
    }

    #[test]
    fn inputs_are_bgr_and_padded_to_32() {
        let image = RgbImage::from_pixel(33, 10, image::Rgb([1, 2, 3]));
        let (input, w, h) = to_input(&image);
        assert_eq!((w, h), (64, 32));
        assert_eq!(input.shape(), [1, 3, 32, 64]);
        assert_eq!((input[[0, 0, 0, 0]], input[[0, 2, 0, 0]]), (3.0, 1.0));
        assert_eq!(input[[0, 0, 20, 40]], 0.0);
    }
}
