//! OpenAI CLIP ViT-B/32 (`openai-vit-b32-224`): image and text towers, both
//! producing L2-normalised 512-d vectors in the same space.

pub mod image;
pub mod preprocess;
pub mod text;

pub use image::ClipImage;
pub use text::ClipText;

/// Embedding dimensions of the pinned model.
pub const EMBEDDING_DIM: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    /// The input cannot be used (undecodable, too large): not worth a retry.
    #[error("{0}")]
    Unsupported(String),
    /// Reading the input failed (e.g. a preview being replaced): retry.
    #[error("{0}")]
    Io(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

/// Unit length (cosine similarity is then a dot product).
pub fn normalize(values: &[f32]) -> Vec<f32> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return values.to_vec();
    }
    values.iter().map(|v| v / norm).collect()
}

/// Dot product of two unit vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddings_are_unit_vectors() {
        assert_eq!(normalize(&[3.0, 4.0]), [0.6, 0.8]);
        assert_eq!(normalize(&[0.0, 0.0]), [0.0, 0.0]);
        assert!((cosine(&[0.6, 0.8], &[0.6, 0.8]) - 1.0).abs() < 1e-6);
    }
}
