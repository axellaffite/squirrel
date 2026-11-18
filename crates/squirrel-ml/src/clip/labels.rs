//! The label bank (`labels/labels-v2.tsv`) and zero-shot tagging against it.
//!
//! Each label is scored on its own (no softmax: labels do not compete): a
//! photo gets a label when its cosine with the label reaches an absolute
//! threshold (the model's default, or the label's own) **and** beats the
//! generic "background" prompts by a margin, so a concept must be more
//! present than "a photo" itself. At most [`TOP_K`] tags, best first.

use super::{ClipError, ClipText, cosine, normalize, templates};

/// Bumped with every change to the label file, the templates, the background
/// prompts or the model (blobs are then re-tagged from their stored embedding).
pub const LABELS_VERSION: i16 = 2;
pub const LABELS_TSV: &str = include_str!("../../labels/labels-v2.tsv");
/// The reserved bank entry holding the background prompts' embedding.
pub const BACKGROUND: &str = "background";
/// At most this many tags per photo.
pub const TOP_K: usize = 5;

/// When a scored label becomes a tag, per model (see [`thresholds`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum cosine between the image and a label without its own threshold.
    pub cosine: f32,
    /// Minimum `cos(label) − cos(background)`.
    pub margin: f32,
}

/// PROVISIONAL for `openai-vit-b32-224`, set on generated pictures only (no
/// real photo library was available): to be calibrated on ~200 real photos
/// with `examples/tag_report.rs` so that ≥ 80 % of shown tags are right.
pub fn thresholds(model: &str) -> Option<Thresholds> {
    match model {
        crate::models::CLIP_MODEL_ID => Some(Thresholds {
            cosine: 0.25,
            margin: 0.045,
        }),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub slug: String,
    pub names: Vec<String>,
    /// Overrides [`Thresholds::cosine`] for this label only.
    pub min_cosine: Option<f32>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LabelError {
    #[error("line {line}: expected `slug<TAB>name|name[<TAB>min_cosine]`")]
    Shape { line: usize },
    #[error("line {line}: bad slug {slug:?}")]
    Slug { line: usize, slug: String },
    #[error("line {line}: duplicate slug {slug:?}")]
    Duplicate { line: usize, slug: String },
    #[error("line {line}: empty class name")]
    EmptyName { line: usize },
    #[error("line {line}: the minimum cosine must be a number in (0, 1)")]
    Threshold { line: usize },
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
        let fields: Vec<&str> = raw.split('\t').collect();
        let (slug, names, min_cosine) = match fields.as_slice() {
            [slug, names] => (*slug, *names, None),
            [slug, names, min] => {
                let min: f32 = min
                    .trim()
                    .parse()
                    .map_err(|_| LabelError::Threshold { line })?;
                if !(min > 0.0 && min < 1.0) {
                    return Err(LabelError::Threshold { line });
                }
                (*slug, *names, Some(min))
            }
            _ => return Err(LabelError::Shape { line }),
        };
        if !is_slug(slug) || slug == BACKGROUND {
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
            min_cosine,
        });
    }
    Ok(labels)
}

/// SHA-256 (hex) of everything the bank is computed from: the label file, the
/// prompt templates and the background prompts. Stored with a bank; a stored
/// bank with another digest is computed again.
pub fn bank_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(LABELS_TSV.as_bytes());
    for part in templates::TEMPLATES
        .iter()
        .chain(&templates::BACKGROUND_PROMPTS)
    {
        hasher.update([0u8]);
        hasher.update(part.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

/// Every prompt to encode, as (bank entry, prompt): each label's names through
/// every template, then the background prompts under [`BACKGROUND`].
pub fn bank_prompts(labels: &[Label]) -> Vec<(String, String)> {
    labels
        .iter()
        .flat_map(|label| {
            templates::prompts(&label.names)
                .into_iter()
                .map(|prompt| (label.slug.clone(), prompt))
        })
        .chain(
            templates::BACKGROUND_PROMPTS
                .iter()
                .map(|prompt| (BACKGROUND.to_owned(), (*prompt).to_owned())),
        )
        .collect()
}

/// The whole bank, encoded in this process (tests, examples; the worker goes
/// through its `ml-exec` child instead).
pub fn compute_bank(
    text: &mut ClipText,
    labels: &[Label],
) -> Result<Vec<(String, Vec<f32>)>, ClipError> {
    let mut grouped: Vec<(String, Vec<Vec<f32>>)> = Vec::new();
    for (entry, prompt) in bank_prompts(labels) {
        let embedding = text.embed(&prompt)?;
        match grouped.last_mut() {
            Some((last, embeddings)) if *last == entry => embeddings.push(embedding),
            _ => grouped.push((entry, vec![embedding])),
        }
    }
    grouped
        .into_iter()
        .map(|(entry, embeddings)| Ok((entry, mean_embedding(&embeddings)?)))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub slug: String,
    pub cosine: f32,
    /// `cosine − cos(background)`.
    pub margin: f32,
}

/// Every label's cosine and margin over the background, best cosine first.
/// The background entry itself is not a label.
pub fn score(image: &[f32], bank: &[(String, Vec<f32>)]) -> Vec<Score> {
    let background = bank
        .iter()
        .find(|(slug, _)| slug == BACKGROUND)
        .map_or(0.0, |(_, text)| cosine(image, text));
    let mut scores: Vec<Score> = bank
        .iter()
        .filter(|(slug, _)| slug != BACKGROUND)
        .map(|(slug, text)| {
            let cosine = cosine(image, text);
            Score {
                slug: slug.clone(),
                cosine,
                margin: cosine - background,
            }
        })
        .collect();
    scores.sort_by(|a, b| b.cosine.total_cmp(&a.cosine));
    scores
}

/// The tags to keep, as (slug, cosine): over the label's threshold and the
/// background margin, at most `top_k`, best first.
pub fn select(
    scores: &[Score],
    labels: &[Label],
    thresholds: Thresholds,
    top_k: usize,
) -> Vec<(String, f32)> {
    let threshold = |slug: &str| {
        labels
            .iter()
            .find(|label| label.slug == slug)
            .and_then(|label| label.min_cosine)
            .unwrap_or(thresholds.cosine)
    };
    scores
        .iter()
        .filter(|s| s.cosine >= threshold(&s.slug) && s.margin >= thresholds.margin)
        .take(top_k)
        .map(|s| (s.slug.clone(), s.cosine))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_bank_parses() {
        let labels = parse(LABELS_TSV).unwrap();
        assert!(labels.len() >= 150, "{}", labels.len());
        assert!(labels.iter().any(|l| l.slug == "cake"));
        assert_eq!(bank().len(), labels.len());
    }

    /// Changing the labels, templates or background prompts must come with a
    /// new LABELS_VERSION: update both here together.
    #[test]
    fn the_bank_digest_is_pinned_to_its_labels_version() {
        assert_eq!(
            (LABELS_VERSION, bank_digest().as_str()),
            (
                2,
                "3416ccdcea95271c07241d35741887f66589019c02f4ea03d2ccacaf739aff47"
            ),
            "the bank changed: bump LABELS_VERSION and pin the new digest"
        );
    }

    #[test]
    fn near_synonyms_are_one_label() {
        let labels = bank();
        for merged in ["birthday_cake", "dessert", "sea", "night_sky", "swimming"] {
            assert!(!labels.iter().any(|l| l.slug == merged), "{merged}");
        }
        let cake = labels.iter().find(|l| l.slug == "cake").unwrap();
        assert!(cake.names.contains(&"birthday cake".to_owned()));
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
            parse("background\tthing"),
            Err(LabelError::Slug { .. })
        ));
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
        assert!(matches!(
            parse("dog\tdog\tx"),
            Err(LabelError::Threshold { .. })
        ));
        assert!(matches!(
            parse("dog\tdog\t1.5"),
            Err(LabelError::Threshold { .. })
        ));
        assert!(matches!(
            parse("dog\tdog\t0.3\tz"),
            Err(LabelError::Shape { .. })
        ));
        assert_eq!(parse("dog\tdog\t0.3").unwrap()[0].min_cosine, Some(0.3));
        assert_eq!(parse("# comment\n\n").unwrap(), []);
    }

    #[test]
    fn a_label_is_the_normalised_mean_of_its_prompts() {
        let mean = mean_embedding(&[vec![1.0, 0.0], vec![0.0, 1.0]]).unwrap();
        assert!((mean[0] - mean[1]).abs() < 1e-6);
        assert!((cosine(&mean, &mean) - 1.0).abs() < 1e-6);
        assert!(mean_embedding(&[vec![1.0, 0.0], vec![1.0]]).is_err());
        assert!(mean_embedding(&[]).is_err());
    }

    #[test]
    fn the_background_prompts_close_the_prompt_list() {
        let labels = parse("dog\tdog|puppy").unwrap();
        let prompts = bank_prompts(&labels);
        assert_eq!(prompts.len(), 14 + templates::BACKGROUND_PROMPTS.len());
        assert_eq!(prompts.last().unwrap().0, BACKGROUND);
    }

    fn unit(angle: f32) -> Vec<f32> {
        vec![angle.cos(), angle.sin()]
    }

    #[test]
    fn labels_must_beat_the_background_and_their_threshold() {
        let labels = parse("a\ta\nb\tb\t0.95\nc\tc").unwrap();
        let bank = vec![
            ("a".to_owned(), unit(0.0)),
            ("b".to_owned(), unit(0.1)),
            ("c".to_owned(), unit(0.6)),
            (BACKGROUND.to_owned(), unit(0.5)),
        ];
        let scores = score(&unit(0.05), &bank);
        assert_eq!(scores.len(), 3, "the background is not a label");
        assert_eq!(scores[0].slug, "a");
        let thresholds = Thresholds {
            cosine: 0.5,
            margin: 0.05,
        };
        let tags = select(&scores, &labels, thresholds, TOP_K);
        let slugs: Vec<&str> = tags.iter().map(|(s, _)| s.as_str()).collect();
        // b: cos 0.9988 passes 0.95; c: cos 0.90 but below the background
        // (cos 0.90 vs 0.90): no margin.
        assert_eq!(slugs, ["a", "b"]);
        let strict = parse("a\ta\t0.9999\nb\tb\t0.9999\nc\tc").unwrap();
        assert!(select(&scores, &strict, thresholds, TOP_K).is_empty());
        assert_eq!(select(&scores, &labels, thresholds, 1).len(), 1);
    }

    /// With the real models: the generated shapes get no unrelated tag.
    #[test]
    fn generated_shapes_get_no_unrelated_tag() {
        use crate::models::ModelGroup;
        let Some((dylib, models)) =
            crate::testutil::test_ml_for(&[ModelGroup::ClipImage, ModelGroup::ClipText])
        else {
            return;
        };
        crate::runtime::init(&dylib).unwrap();
        let mut text = ClipText::load(&models, 2).unwrap();
        let mut image = super::super::ClipImage::load(&models, 2).unwrap();
        let labels = bank();
        let bank = compute_bank(&mut text, &labels).unwrap();
        let thresholds = thresholds(crate::models::CLIP_MODEL_ID).unwrap();
        // Fair readings of flat shapes on white: a flag (a red disc is
        // Japan's), a sign, art; a green triangle is a Christmas tree.
        let related = [
            "flag",
            "sign",
            "art",
            "macro",
            "christmas",
            "forest",
            "document",
        ];
        for shape in crate::testutil::Shape::ALL {
            let embedding = image
                .embed(&crate::testutil::shape_image(shape, 512))
                .unwrap();
            let tags = select(&score(&embedding, &bank), &labels, thresholds, TOP_K);
            for (tag, cosine) in &tags {
                assert!(
                    related.contains(&tag.as_str()),
                    "{}: unrelated tag {tag} ({cosine})",
                    shape.name()
                );
            }
        }
    }
}
