//! The `ml-exec` child's protocol: one JSON object per line on stdin
//! (requests) and stdout (a `ready` line, then one response per request).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Longest request line the child reads (a path or 256 short texts).
pub const MAX_REQUEST_LINE: usize = 64 * 1024;
/// Longest response line the parent reads (256 text embeddings of 768 floats
/// take ≈ 3 MiB; a test checks the worst case fits).
pub const MAX_RESPONSE_LINE: usize = 4 * 1024 * 1024;
/// Most texts one `clip_text` request may carry.
pub const MAX_TEXTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlRequest {
    pub id: u64,
    #[serde(flatten)]
    pub op: MlOp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MlOp {
    Ping,
    /// The image embedding (SigLIP2, 768-d) of an image file (a preview WebP).
    ClipImage {
        input: PathBuf,
    },
    /// One text embedding per text.
    ClipText {
        texts: Vec<String>,
    },
    /// Faces found on an image file (Milestone D).
    Faces {
        input: PathBuf,
    },
}

impl MlOp {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::ClipImage { .. } => "clip_image",
            Self::ClipText { .. } => "clip_text",
            Self::Faces { .. } => "faces",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlResponse {
    pub id: u64,
    #[serde(flatten)]
    pub body: MlBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MlBody {
    Ok(MlOutput),
    /// The input cannot be processed (e.g. not an image): retrying cannot help.
    Unsupported {
        reason: String,
    },
    /// Processing failed (e.g. a model error).
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MlOutput {
    Embedding {
        embedding: Vec<f32>,
    },
    Embeddings {
        embeddings: Vec<Vec<f32>>,
    },
    /// `faces`: every face found, relative to the preview.
    Faces {
        faces: Vec<crate::faces::FaceFound>,
    },
    /// `ping`: no payload. Last, as it matches any object.
    Pong {},
}

/// The child's first line, once its models are verified and loaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ready {
    pub ready: bool,
    pub groups: Vec<String>,
    /// The loaded ONNX Runtime, when a model group needed one.
    pub onnxruntime: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_are_flat_objects_tagged_by_op() {
        let request = MlRequest {
            id: 7,
            op: MlOp::ClipImage {
                input: "/d/preview-v1.webp".into(),
            },
        };
        let line = serde_json::to_value(&request).unwrap();
        assert_eq!(
            line,
            json!({"id": 7, "op": "clip_image", "input": "/d/preview-v1.webp"})
        );
        let ping: MlRequest = serde_json::from_str(r#"{"id":1,"op":"ping"}"#).unwrap();
        assert_eq!(ping.op, MlOp::Ping);
        assert!(serde_json::from_str::<MlRequest>(r#"{"id":1,"op":"rm"}"#).is_err());
    }

    /// The longest floats serde writes, for the most texts one request takes.
    #[test]
    fn the_largest_text_response_fits_a_line() {
        let worst = -1.234_567_9e-11_f32;
        let response = MlResponse {
            id: u64::MAX,
            body: MlBody::Ok(MlOutput::Embeddings {
                embeddings: vec![vec![worst; crate::clip::EMBEDDING_DIM]; MAX_TEXTS],
            }),
        };
        let line = serde_json::to_string(&response).unwrap();
        assert!(line.len() < MAX_RESPONSE_LINE, "{}", line.len());
    }

    #[test]
    fn responses_round_trip() {
        for body in [
            MlBody::Ok(MlOutput::Pong {}),
            MlBody::Ok(MlOutput::Embedding {
                embedding: vec![0.5, -0.25],
            }),
            MlBody::Ok(MlOutput::Embeddings {
                embeddings: vec![vec![1.0], vec![0.0]],
            }),
            MlBody::Unsupported {
                reason: "not an image".into(),
            },
            MlBody::Failed {
                reason: "model".into(),
            },
        ] {
            let response = MlResponse { id: 3, body };
            let line = serde_json::to_string(&response).unwrap();
            assert!(!line.contains('\n'));
            assert_eq!(
                serde_json::from_str::<MlResponse>(&line).unwrap(),
                response,
                "{line}"
            );
        }
        let ok = serde_json::to_value(MlResponse {
            id: 1,
            body: MlBody::Ok(MlOutput::Embedding {
                embedding: vec![1.0],
            }),
        })
        .unwrap();
        assert_eq!(ok, json!({"id": 1, "status": "ok", "embedding": [1.0]}));
    }
}
