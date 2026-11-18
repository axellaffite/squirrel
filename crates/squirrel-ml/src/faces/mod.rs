//! Faces: YuNet detection, 5-point alignment and SFace embeddings (OpenCV
//! Zoo models, `yunet-sface-2021dec`).

pub mod detect;

pub use detect::{Detection, Detector};

#[cfg(test)]
mod tests;
