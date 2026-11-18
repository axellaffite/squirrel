//! OpenAI CLIP ViT-B/32 (`openai-vit-b32-224`): image and text towers, both
//! producing L2-normalised 512-d vectors in the same space.

pub mod image;
pub mod labels;
pub mod preprocess;
pub mod templates;
pub mod text;

#[cfg(test)]
mod golden_tests;

pub use image::ClipImage;
pub use text::ClipText;

/// Embedding dimensions of the pinned model.
pub const EMBEDDING_DIM: usize = 512;
/// Photo search keeps photos within this cosine distance of the query (text
/// to image similarities are small: ≈ 0.2–0.35 for a match). PROVISIONAL:
/// to be calibrated on real photos together with the tag thresholds.
pub const SEARCH_MAX_DISTANCE: f64 = 0.78;

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

/// What a stored embedding must be: [`EMBEDDING_DIM`] finite values of unit
/// length (± 1e-2, half precision included). Anything else is a broken model
/// or child, never stored.
pub fn check_embedding(values: &[f32]) -> Result<(), String> {
    check_unit_vector(values, EMBEDDING_DIM)
}

/// `dim` finite values of unit length (± 1e-2).
pub fn check_unit_vector(values: &[f32], dim: usize) -> Result<(), String> {
    if values.len() != dim {
        return Err(format!("{} values instead of {dim}", values.len()));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("non-finite values".into());
    }
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if (norm - 1.0).abs() > 1e-2 {
        return Err(format!("norm {norm} instead of 1"));
    }
    Ok(())
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

    #[test]
    fn only_unit_512_d_finite_vectors_pass() {
        let mut unit = vec![0.0; EMBEDDING_DIM];
        unit[0] = 1.0;
        assert!(check_embedding(&unit).is_ok());
        assert!(check_embedding(&unit[..511]).is_err());
        let mut nan = unit.clone();
        nan[1] = f32::NAN;
        assert!(check_embedding(&nan).is_err());
        assert!(
            check_embedding(&vec![0.5; EMBEDDING_DIM]).is_err(),
            "norm 11"
        );
        let mut almost = unit.clone();
        almost[0] = 1.005;
        assert!(check_embedding(&almost).is_ok());
    }
}
