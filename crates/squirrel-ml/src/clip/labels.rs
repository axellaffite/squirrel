//! The label bank (`labels/labels-v3.tsv`) and zero-shot tagging against it.
//!
//! Each label is scored on its own with SigLIP's match probability (a sigmoid:
//! labels do not share one probability mass). A photo gets a label when its
//! probability reaches [`Thresholds::min_p`]. Labels that are easily mistaken
//! for each other (dog and cat, car and truck) share a **group**: only the
//! best label of a group can become a tag, and only when its logit beats the
//! second best sibling's by [`Thresholds::contrast`]. At most [`TOP_K`] tags,
//! best first.

use super::{ClipError, ClipText, LOGIT_BIAS, LOGIT_SCALE, cosine, normalize, templates};

/// Bumped with every change to the label file, the templates or the model
/// (blobs are then re-tagged from their stored embedding).
pub const LABELS_VERSION: i16 = 3;
pub const LABELS_TSV: &str = include_str!("../../labels/labels-v3.tsv");
/// At most this many tags per photo.
pub const TOP_K: usize = 5;

/// When a scored label becomes a tag, per model (see [`thresholds`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum match probability of a label.
    pub min_p: f32,
    /// Minimum logit lead of a grouped label over its best sibling.
    pub contrast: f32,
}

/// For `siglip2-vit-b16-224`, calibrated on the committed fixtures (pets,
/// portraits, generated shapes): see the Milestone I calibration in the
/// Phase 5 plan. To be re-checked on real photos with `examples/tag_report.rs`.
pub fn thresholds(model: &str) -> Option<Thresholds> {
    match model {
        crate::models::CLIP_MODEL_ID => Some(Thresholds {
            min_p: TAG_MIN_P,
            contrast: TAG_CONTRAST,
        }),
        _ => None,
    }
}

/// See [`Thresholds::min_p`] (logit ≈ −6.0). On the fixtures, a dog or cat
/// photo scores its label at logit −3.5 … −4.2, and no label of an unrelated
/// picture (portraits, flat shapes) reached −6.4. SigLIP's probabilities for
/// the templated labels are small even on a clear match (≈ 0.015–0.03):
/// the floor sits between the two, not at 0.5.
pub const TAG_MIN_P: f32 = 0.0025;
/// See [`Thresholds::contrast`] (logits: e^1 ≈ 2.7 times the odds). On the
/// fixtures the right pet led the other by > 5 logits.
pub const TAG_CONTRAST: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub slug: String,
    pub names: Vec<String>,
    /// Labels a photo is easily mistaken between share a group.
    pub group: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LabelError {
    #[error("line {line}: expected `slug<TAB>name|name[<TAB>group]`")]
    Shape { line: usize },
    #[error("line {line}: bad slug {slug:?}")]
    Slug { line: usize, slug: String },
    #[error("line {line}: duplicate slug {slug:?}")]
    Duplicate { line: usize, slug: String },
    #[error("line {line}: empty class name")]
    EmptyName { line: usize },
    #[error("line {line}: bad group {group:?}")]
    Group { line: usize, group: String },
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
        let (slug, names, group) = match fields.as_slice() {
            [slug, names] => (*slug, *names, None),
            [slug, names, group] => {
                if !is_slug(group) {
                    return Err(LabelError::Group {
                        line,
                        group: (*group).to_owned(),
                    });
                }
                (*slug, *names, Some((*group).to_owned()))
            }
            _ => return Err(LabelError::Shape { line }),
        };
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
            group,
        });
    }
    Ok(labels)
}

/// SHA-256 (hex) of everything the bank is computed from: the label file and
/// the prompt templates. Stored with a bank; a stored bank with another
/// digest is computed again.
pub fn bank_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(LABELS_TSV.as_bytes());
    for part in templates::TEMPLATES {
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

/// Every prompt to encode, as (label slug, prompt): each label's names through
/// every template.
pub fn bank_prompts(labels: &[Label]) -> Vec<(String, String)> {
    labels
        .iter()
        .flat_map(|label| {
            templates::prompts(&label.names)
                .into_iter()
                .map(|prompt| (label.slug.clone(), prompt))
        })
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
    /// `scale · cosine + bias`: SigLIP's logit.
    pub logit: f32,
    /// `σ(logit)`.
    pub probability: f32,
}

/// Every label's cosine, logit and probability, best first.
pub fn score(image: &[f32], bank: &[(String, Vec<f32>)]) -> Vec<Score> {
    let mut scores: Vec<Score> = bank
        .iter()
        .map(|(slug, text)| {
            let cosine = cosine(image, text);
            Score {
                slug: slug.clone(),
                cosine,
                logit: LOGIT_SCALE * cosine + LOGIT_BIAS,
                probability: super::match_probability(cosine),
            }
        })
        .collect();
    scores.sort_by(|a, b| b.logit.total_cmp(&a.logit));
    scores
}

/// The group of `slug` in `labels`, if any.
fn group_of<'a>(labels: &'a [Label], slug: &str) -> Option<&'a str> {
    labels
        .iter()
        .find(|label| label.slug == slug)
        .and_then(|label| label.group.as_deref())
}

/// The tags to keep, as (slug, probability): probable enough, and for a
/// grouped label, the best of its group by `contrast` logits; at most `top_k`,
/// best first. `scores` must hold every label (as [`score`] returns them).
pub fn select(
    scores: &[Score],
    labels: &[Label],
    thresholds: Thresholds,
    top_k: usize,
) -> Vec<(String, f32)> {
    let leads_its_group = |s: &Score| {
        let Some(group) = group_of(labels, &s.slug) else {
            return true;
        };
        scores
            .iter()
            .filter(|other| other.slug != s.slug && group_of(labels, &other.slug) == Some(group))
            .all(|sibling| s.logit - sibling.logit >= thresholds.contrast)
    };
    scores
        .iter()
        .filter(|s| s.probability >= thresholds.min_p && leads_its_group(s))
        .take(top_k)
        .map(|s| (s.slug.clone(), s.probability))
        .collect()
}

/// The labels a search for `query` must beat on each photo: when the query's
/// text embedding is within `min_cosine` of a grouped label (the closest one),
/// that label's siblings; otherwise none. `bank` holds the label embeddings.
pub fn search_rivals<'a>(
    query: &[f32],
    bank: &'a [(String, Vec<f32>)],
    labels: &[Label],
    min_cosine: f32,
) -> Vec<(&'a str, &'a [f32])> {
    let closest = bank
        .iter()
        .map(|(slug, text)| (slug.as_str(), cosine(query, text)))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    let Some((matched, similarity)) = closest else {
        return Vec::new();
    };
    let Some(group) = group_of(labels, matched).filter(|_| similarity >= min_cosine) else {
        return Vec::new();
    };
    bank.iter()
        .filter(|(slug, _)| slug != matched && group_of(labels, slug) == Some(group))
        .map(|(slug, text)| (slug.as_str(), text.as_slice()))
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

    /// Changing the labels or templates must come with a new LABELS_VERSION:
    /// update both here together.
    #[test]
    fn the_bank_digest_is_pinned_to_its_labels_version() {
        assert_eq!(
            (LABELS_VERSION, bank_digest().as_str()),
            (
                3,
                "a8fcc0bc71550ca3c53bd4c384f7d119b75f7e5b7dd269da1058626476599b40"
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
    fn confusable_labels_share_a_group() {
        let labels = bank();
        for (a, b) in [
            ("dog", "cat"),
            ("car", "truck"),
            ("beach", "lake"),
            ("baby", "child"),
        ] {
            let (ga, gb) = (group_of(&labels, a), group_of(&labels, b));
            assert!(ga.is_some() && ga == gb, "{a} {b}");
        }
        assert_eq!(group_of(&labels, "sunset"), None);
        // A group of one would never compete: every group has siblings.
        for label in &labels {
            if let Some(group) = &label.group {
                let size = labels
                    .iter()
                    .filter(|l| l.group.as_ref() == Some(group))
                    .count();
                assert!(size >= 2, "{group}");
            }
        }
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
        assert!(matches!(
            parse("dog\tdog\tPets"),
            Err(LabelError::Group { .. })
        ));
        assert!(matches!(
            parse("dog\tdog\tpet\tz"),
            Err(LabelError::Shape { .. })
        ));
        assert_eq!(
            parse("dog\tdog\tpet").unwrap()[0].group.as_deref(),
            Some("pet")
        );
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
    fn every_prompt_belongs_to_a_label() {
        let labels = parse("dog\tdog|puppy").unwrap();
        let prompts = bank_prompts(&labels);
        assert_eq!(prompts.len(), 2 * templates::TEMPLATES.len());
        assert!(prompts.iter().all(|(slug, _)| slug == "dog"));
    }

    /// A unit vector whose cosine with `axis()` is `cos`.
    fn at(cos: f32) -> Vec<f32> {
        vec![cos, (1.0 - cos * cos).sqrt()]
    }

    fn axis() -> Vec<f32> {
        vec![1.0, 0.0]
    }

    /// Scores straight from cosines: labels as unit vectors, image on the axis.
    fn scores(cosines: &[(&str, f32)]) -> Vec<Score> {
        let bank: Vec<(String, Vec<f32>)> = cosines
            .iter()
            .map(|(slug, cos)| ((*slug).to_owned(), at(*cos)))
            .collect();
        score(&axis(), &bank)
    }

    const RULE: Thresholds = Thresholds {
        min_p: 0.5,
        contrast: 1.0,
    };

    #[test]
    fn scores_are_siglip_probabilities_best_first() {
        let scored = scores(&[("a", 0.10), ("b", 0.20)]);
        assert_eq!(scored[0].slug, "b");
        assert!((scored[0].logit - (LOGIT_SCALE * 0.20 + LOGIT_BIAS)).abs() < 1e-3);
        assert!(scored[0].probability > 0.99 && scored[1].probability < 0.01);
    }

    #[test]
    fn a_tag_needs_its_probability() {
        let labels = parse("a\ta\nb\tb").unwrap();
        // 0.1489 is p = 0.5: a just passes, b does not.
        let tags = select(&scores(&[("a", 0.150), ("b", 0.147)]), &labels, RULE, TOP_K);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0, "a");
        let tags = select(&scores(&[("a", 0.2), ("b", 0.2)]), &labels, RULE, 1);
        assert_eq!(tags.len(), 1, "top k");
    }

    #[test]
    fn a_grouped_tag_must_beat_its_siblings_by_the_contrast() {
        let labels = parse("dog\tdog\tpet\ncat\tcat\tpet\nsofa\tsofa").unwrap();
        // 1 logit ≈ 0.0089 of cosine.
        let close = scores(&[("dog", 0.200), ("cat", 0.195), ("sofa", 0.180)]);
        let slugs = |tags: Vec<(String, f32)>| tags.into_iter().map(|t| t.0).collect::<Vec<_>>();
        assert_eq!(
            slugs(select(&close, &labels, RULE, TOP_K)),
            ["sofa"],
            "dog and cat too close: neither; the ungrouped sofa stays"
        );
        let clear = scores(&[("dog", 0.200), ("cat", 0.180), ("sofa", 0.180)]);
        assert_eq!(slugs(select(&clear, &labels, RULE, TOP_K)), ["dog", "sofa"]);
        // A sibling below the probability floor still counts as a rival.
        let low = scores(&[("dog", 0.150), ("cat", 0.146)]);
        assert!(select(&low, &labels, RULE, TOP_K).is_empty());
    }

    #[test]
    fn a_search_close_to_a_grouped_label_meets_its_siblings() {
        let labels =
            parse("dog\tdog\tpet\ncat\tcat\tpet\nrabbit\trabbit\tpet\nsofa\tsofa").unwrap();
        let bank = vec![
            ("cat".to_owned(), vec![0.0, 1.0, 0.0]),
            ("dog".to_owned(), vec![1.0, 0.0, 0.0]),
            ("rabbit".to_owned(), vec![0.0, 0.0, 1.0]),
            ("sofa".to_owned(), vec![0.6, 0.0, 0.8]),
        ];
        let query = normalize(&[0.9, 0.1, 0.0]);
        let rivals: Vec<&str> = search_rivals(&query, &bank, &labels, 0.8)
            .iter()
            .map(|(slug, _)| *slug)
            .collect();
        assert_eq!(rivals, ["cat", "rabbit"]);
        assert!(
            search_rivals(&query, &bank, &labels, 0.999).is_empty(),
            "not close enough"
        );
        let sofa = normalize(&[0.6, 0.0, 0.8]);
        assert!(
            search_rivals(&sofa, &bank, &labels, 0.8).is_empty(),
            "no group"
        );
        assert!(search_rivals(&query, &[], &labels, 0.8).is_empty());
    }
}
