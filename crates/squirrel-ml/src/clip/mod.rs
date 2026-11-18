//! SigLIP2 ViT-B/16 (`siglip2-vit-b16-224`): image and text towers, both
//! producing L2-normalised 768-d vectors in the same space. The module keeps
//! the generic "clip" name (contrastive image–text pre-training).

pub mod image;
pub mod labels;
pub mod preprocess;
pub mod templates;
pub mod text;

#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod golden_tests;

pub use image::ClipImage;
pub use text::ClipText;

/// Embedding dimensions of the pinned model.
pub const EMBEDDING_DIM: usize = 768;
// Photo search. Calibrated on the committed fixtures (Milestone I, Phase 5
// plan): matching query–photo pairs scored logits −6.9 … +3.5 (bare words
// such as "dog" ≈ −3.6, descriptions such as "a red circle" ≈ +3.5), unrelated
// pairs −8.7 or lower. SigLIP gives bare words low absolute probabilities
// (≈ 0.01–0.1 on a clear match): the floor is set between the two, not at 0.5.

/// Photo search keeps photos whose match probability with the query reaches
/// this (logit ≈ −7.6).
pub const SEARCH_MIN_P: f32 = 0.0005;
/// Photo search drops results more than this many logits behind the best hit
/// (a weak tail far behind a clear match is noise).
pub const SEARCH_RELATIVE_CUTOFF: f32 = 6.0;
/// A query whose text embedding is at least this close (cosine) to a grouped
/// label of the bank is taken for that label: the label's siblings become the
/// query's rivals (see [`labels::search_rivals`]). Queries naming a label in
/// any language ("dog", "chien", "red car") scored ≥ 0.85; queries for
/// nothing in the bank ("a blue square", "red circle") ≤ 0.82.
pub const QUERY_LABEL_MATCH: f32 = 0.85;
/// A photo is kept only if the query's logit beats every rival's logit on
/// that photo by this much: "dog" drops a photo where "cat" wins.
pub const SEARCH_CONTRAST: f32 = 1.0;

/// The cosine whose match probability is `p` (0 < p < 1).
pub fn cosine_for_probability(p: f32) -> f32 {
    ((p / (1.0 - p)).ln() - LOGIT_BIAS) / LOGIT_SCALE
}

/// [`SEARCH_MIN_P`] as a cosine.
pub fn search_min_cosine() -> f32 {
    cosine_for_probability(SEARCH_MIN_P)
}

/// A difference in logits as a difference in cosine (logits are linear in it).
pub fn logits_as_cosine(logits: f32) -> f32 {
    logits / LOGIT_SCALE
}

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

/// SigLIP's learned temperature, `exp(logit_scale)`: `logit_scale` is
/// 4.724453449249268 (F32) in `open_clip_model.safetensors` of
/// `timm/ViT-B-16-SigLIP2` @ eee10eff6dd8cabae2d7f379d4e8cfcd352030aa, the
/// checkpoint the pinned ONNX export was made from (the export drops it).
pub const LOGIT_SCALE: f32 = 112.668_9;
/// SigLIP's learned bias, `logit_bias` in the same file: −16.771724700927734.
pub const LOGIT_BIAS: f32 = -16.771_725;

/// The probability SigLIP was trained to give that an image and a text
/// match: `σ(scale · cos + bias)`, 0.5 at cos ≈ 0.1489. Independent per pair
/// (a sigmoid, not a softmax over the candidates).
pub fn match_probability(cosine: f32) -> f32 {
    1.0 / (1.0 + (-(LOGIT_SCALE * cosine + LOGIT_BIAS)).exp())
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
    fn the_match_probability_uses_the_learned_scale_and_bias() {
        assert!((LOGIT_SCALE - 4.724_453_4_f32.exp()).abs() < 1e-3);
        let even = -LOGIT_BIAS / LOGIT_SCALE;
        assert!((even - 0.148_86).abs() < 1e-4, "{even}");
        assert!((match_probability(even) - 0.5).abs() < 1e-5);
        assert!(match_probability(0.05) < 1e-4);
        assert!(match_probability(0.25) > 0.9999);
        assert!(match_probability(0.14) < match_probability(0.15));
        for p in [0.0005, 0.01, 0.5, 0.9] {
            let back = match_probability(cosine_for_probability(p));
            assert!((back - p).abs() < p * 1e-3, "{p} {back}");
        }
        let floor = LOGIT_SCALE * search_min_cosine() + LOGIT_BIAS;
        assert!((floor + 7.6).abs() < 0.01, "{floor}");
        assert!((logits_as_cosine(LOGIT_SCALE) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn only_unit_768_d_finite_vectors_pass() {
        let mut unit = vec![0.0; EMBEDDING_DIM];
        unit[0] = 1.0;
        assert!(check_embedding(&unit).is_ok());
        assert!(check_embedding(&unit[..767]).is_err());
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
