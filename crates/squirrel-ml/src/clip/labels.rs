//! The label bank (`labels/labels-v1.tsv`) and zero-shot scoring against it.

use super::{ClipError, ClipText, cosine, normalize, templates};

/// Bumped with every change to the label file, the templates or the model
/// (blobs are then re-tagged from their stored embedding).
pub const LABELS_VERSION: i16 = 1;
pub const LABELS_TSV: &str = include_str!("../../labels/labels-v1.tsv");
/// CLIP's logit scale: probabilities are `softmax(100 · cos)` over the bank.
const LOGIT_SCALE: f32 = 100.0;
/// At most this many tags per photo.
pub const TOP_K: usize = 5;

/// When a scored label becomes a tag, per model (see [`thresholds`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum softmax probability over the whole bank.
    pub probability: f32,
    /// Minimum cosine similarity between the image and the label.
    pub cosine: f32,
}

/// PROVISIONAL for `openai-vit-b32-224`, set on generated pictures only (no
/// real photo library was available): to be calibrated on ~200 real photos
/// with `examples/tag_report.rs` so that ≥ 80 % of shown tags are right.
/// Precision first: a dominant concept usually clears 0.2 among 200 labels.
pub fn thresholds(model: &str) -> Option<Thresholds> {
    match model {
        crate::models::CLIP_MODEL_ID => Some(Thresholds {
            probability: 0.20,
            cosine: 0.24,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub slug: String,
    pub names: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LabelError {
    #[error("line {line}: expected `slug<TAB>name|name`")]
    Shape { line: usize },
    #[error("line {line}: bad slug {slug:?}")]
    Slug { line: usize, slug: String },
    #[error("line {line}: duplicate slug {slug:?}")]
    Duplicate { line: usize, slug: String },
    #[error("line {line}: empty class name")]
    EmptyName { line: usize },
}

/// Slugs as `asset_tags.tag` accepts them: `^[a-z][a-z0-9_]{0,47}$`.
pub fn is_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    (1..=48).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// Parses a label file; `#` lines and blank lines are ignored.
pub fn parse(tsv: &str) -> Result<Vec<Label>, LabelError> {
    let mut labels: Vec<Label> = Vec::new();
    for (index, raw) in tsv.lines().enumerate() {
        let line = index + 1;
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        let (slug, names) = raw.split_once('\t').ok_or(LabelError::Shape { line })?;
        if !is_slug(slug) {
            return Err(LabelError::Slug {
                line,
                slug: slug.to_owned(),
            });
        }
        if labels.iter().any(|label| label.slug == slug) {
            return Err(LabelError::Duplicate {
                line,
                slug: slug.to_owned(),
            });
        }
        let names: Vec<String> = names.split('|').map(|n| n.trim().to_owned()).collect();
        if names.iter().any(String::is_empty) {
            return Err(LabelError::EmptyName { line });
        }
        labels.push(Label {
            slug: slug.to_owned(),
            names,
        });
    }
    Ok(labels)
}

/// The committed bank (checked by a test).
pub fn bank() -> Vec<Label> {
    parse(LABELS_TSV).unwrap_or_default()
}

/// A label's text embedding: the re-normalised mean of its prompts'
/// embeddings, which must all have the same length.
pub fn mean_embedding(prompt_embeddings: &[Vec<f32>]) -> Result<Vec<f32>, ClipError> {
    let dim = prompt_embeddings.first().map_or(0, Vec::len);
    if dim == 0 || prompt_embeddings.iter().any(|e| e.len() != dim) {
        return Err(ClipError::Inference(
            "prompt embeddings of different lengths".into(),
        ));
    }
    let mut sum = vec![0.0f32; dim];
    for embedding in prompt_embeddings {
        for (total, value) in sum.iter_mut().zip(embedding) {
            *total += value;
        }
    }
    Ok(normalize(&sum))
}

/// The whole bank, encoded in this process (tests, examples; the worker goes
/// through its `ml-exec` child instead).
pub fn compute_bank(
    text: &mut ClipText,
    labels: &[Label],
) -> Result<Vec<(String, Vec<f32>)>, ClipError> {
    labels
        .iter()
        .map(|label| {
            let embeddings = templates::prompts(&label.names)
                .iter()
                .map(|prompt| text.embed(prompt))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((label.slug.clone(), mean_embedding(&embeddings)?))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub slug: String,
    pub cosine: f32,
    pub probability: f32,
}

/// Every label's cosine and softmax probability, best first.
pub fn score(image: &[f32], bank: &[(String, Vec<f32>)]) -> Vec<Score> {
    let cosines: Vec<f32> = bank.iter().map(|(_, text)| cosine(image, text)).collect();
    let max = cosines.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = cosines
        .iter()
        .map(|c| (LOGIT_SCALE * (c - max)).exp())
        .collect();
    let sum: f32 = exps.iter().sum();
    let mut scores: Vec<Score> = bank
        .iter()
        .zip(cosines.iter().zip(&exps))
        .map(|((slug, _), (cosine, exp))| Score {
            slug: slug.clone(),
            cosine: *cosine,
            probability: exp / sum,
        })
        .collect();
    scores.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    scores
}

/// The tags to keep: among the `top_k` best, those over both thresholds,
/// as (slug, probability).
pub fn select(scores: &[Score], thresholds: Thresholds, top_k: usize) -> Vec<(String, f32)> {
    scores
        .iter()
        .take(top_k)
        .filter(|s| s.probability >= thresholds.probability && s.cosine >= thresholds.cosine)
        .map(|s| (s.slug.clone(), s.probability))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_the_normalised_mean_of_its_prompts() {
        let mean = mean_embedding(&[vec![1.0, 0.0], vec![0.0, 1.0]]).unwrap();
        assert!(mean_embedding(&[vec![1.0, 0.0], vec![1.0]]).is_err());
        assert!(mean_embedding(&[]).is_err());
        assert!((mean[0] - mean[1]).abs() < 1e-6);
        assert!((cosine(&mean, &mean) - 1.0).abs() < 1e-6);
    }

    /// With the real models: the generated shapes get no unrelated tag.
    #[test]
    fn generated_shapes_get_no_unrelated_tag() {
        let Some((dylib, models)) = crate::testutil::test_ml_for(&[
            crate::models::ModelGroup::ClipImage,
            crate::models::ModelGroup::ClipText,
        ]) else {
            return;
        };
        crate::runtime::init(&dylib).unwrap();
        let mut text = ClipText::load(&models, 2).unwrap();
        let mut image = super::super::ClipImage::load(&models, 2).unwrap();
        let bank = compute_bank(&mut text, &bank()).unwrap();
        let thresholds = thresholds(crate::models::CLIP_MODEL_ID).unwrap();
        // Fair readings of flat shapes on white: a flag (a red disc is
        // Japan's), a sign, a drawing; a green triangle is a Christmas tree.
        let related = [
            "flag",
            "sign",
            "drawing",
            "painting",
            "macro",
            "text",
            "christmas",
            "tree",
        ];
        for shape in crate::testutil::Shape::ALL {
            let embedding = image
                .embed(&crate::testutil::shape_image(shape, 512))
                .unwrap();
            let tags = select(&score(&embedding, &bank), thresholds, TOP_K);
            for (tag, probability) in &tags {
                assert!(
                    related.contains(&tag.as_str()),
                    "{}: unrelated tag {tag} ({probability})",
                    shape.name()
                );
            }
        }
    }

    #[test]
    fn the_committed_bank_parses() {
        let labels = parse(LABELS_TSV).unwrap();
        assert!(labels.len() >= 190, "{}", labels.len());
        assert!(labels.iter().any(|l| l.slug == "birthday_cake"));
        assert_eq!(bank().len(), labels.len());
    }

    #[test]
    fn bad_files_are_refused() {
        assert_eq!(
            parse("dog\tdog\ndog\tpuppy"),
            Err(LabelError::Duplicate {
                line: 2,
                slug: "dog".into()
            })
        );
        assert!(matches!(parse("Dog\tdog"), Err(LabelError::Slug { .. })));
        assert!(matches!(
            parse("dog-cat\tdog"),
            Err(LabelError::Slug { .. })
        ));
        assert!(matches!(
            parse("dog\tdog||puppy"),
            Err(LabelError::EmptyName { .. })
        ));
        assert!(matches!(parse("dog\t"), Err(LabelError::EmptyName { .. })));
        assert!(matches!(parse("dog"), Err(LabelError::Shape { .. })));
        assert_eq!(parse("# comment\n\n").unwrap(), []);
    }

    fn unit(angle: f32) -> Vec<f32> {
        vec![angle.cos(), angle.sin()]
    }

    #[test]
    fn scores_are_a_softmax_over_the_bank() {
        let bank = vec![
            ("a".to_owned(), unit(0.0)),
            ("b".to_owned(), unit(0.5)),
            ("c".to_owned(), unit(1.5)),
        ];
        let scores = score(&unit(0.05), &bank);
        assert_eq!(scores[0].slug, "a");
        let total: f32 = scores.iter().map(|s| s.probability).sum();
        assert!((total - 1.0).abs() < 1e-5);
    }

    #[test]
    fn select_keeps_at_most_k_tags_over_both_thresholds_sorted() {
        let scores: Vec<Score> = (0..8)
            .map(|i| Score {
                slug: format!("t{i}"),
                cosine: if i == 1 { 0.1 } else { 0.3 },
                probability: 0.5 - i as f32 * 0.05,
            })
            .collect();
        let thresholds = Thresholds {
            probability: 0.2,
            cosine: 0.2,
        };
        let tags = select(&scores, thresholds, TOP_K);
        let slugs: Vec<&str> = tags.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(slugs, ["t0", "t2", "t3", "t4"]);
        assert!(tags.windows(2).all(|w| w[0].1 >= w[1].1));
        assert!(
            select(
                &scores,
                Thresholds {
                    probability: 0.0,
                    cosine: 0.0
                },
                TOP_K
            )
            .len()
                == 5
        );
    }
}
