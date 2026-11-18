//! Machine learning on photos: pinned models, ONNX Runtime and the inference
//! code run by the `squirrel ml-exec` child. Pure inference: no database, no
//! network.

pub mod models;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;

pub use models::{CLIP_MODEL_ID, FACE_MODEL_ID, ModelDir, ModelError, ModelGroup};
