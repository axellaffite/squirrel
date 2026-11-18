//! The real models on the committed fixtures: public-domain pet photos
//! (`tests/fixtures/pets`), the face portraits and the generated shapes. These
//! are the calibration's regression tests (Milestone I).

use super::labels::{self, TOP_K};
use super::{
    ClipImage, ClipText, LOGIT_BIAS, LOGIT_SCALE, QUERY_LABEL_MATCH, SEARCH_CONTRAST, cosine,
    search_min_cosine,
};
use std::sync::{LazyLock, Mutex, MutexGuard};

use crate::models::{CLIP_MODEL_ID, ModelGroup};
use crate::testutil::{Shape, shape_image, test_ml_for};

struct Fixtures {
    text: ClipText,
    bank: Vec<(String, Vec<f32>)>,
    labels: Vec<labels::Label>,
    /// (name, image embedding).
    photos: Vec<(String, Vec<f32>)>,
}

fn fixture(dir: &str, name: &str) -> image::RgbImage {
    let path = format!(
        "{}/tests/fixtures/{dir}/{name}.jpg",
        env!("CARGO_MANIFEST_DIR")
    );
    image::open(path).unwrap().to_rgb8()
}

/// Computed once for every test here (the bank is ≈ 1 350 prompts).
static FIXTURES: LazyLock<Option<Mutex<Fixtures>>> = LazyLock::new(|| compute().map(Mutex::new));

fn load() -> Option<MutexGuard<'static, Fixtures>> {
    FIXTURES.as_ref().map(|fixtures| {
        fixtures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    })
}

fn compute() -> Option<Fixtures> {
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipImage, ModelGroup::ClipText])?;
    crate::runtime::init(&dylib).unwrap();
    let mut text = ClipText::load(&models, 4).unwrap();
    let mut image = ClipImage::load(&models, 4).unwrap();
    let labels = labels::bank();
    let bank = labels::compute_bank(&mut text, &labels).unwrap();
    let mut photos = Vec::new();
    for name in ["dog-a", "dog-b", "cat-a", "cat-b"] {
        photos.push((
            name.to_owned(),
            image.embed(&fixture("pets", name)).unwrap(),
        ));
    }
    for name in ["koch-a", "glover-a"] {
        photos.push((
            name.to_owned(),
            image.embed(&fixture("faces", name)).unwrap(),
        ));
    }
    for shape in Shape::ALL {
        let embedding = image.embed(&shape_image(shape, 512)).unwrap();
        photos.push((shape.name().to_owned(), embedding));
    }
    Some(Fixtures {
        text,
        bank,
        labels,
        photos,
    })
}

/// What the search query in `squirrel-db` keeps, by name (best first): over
/// the floor, and ahead of every rival by the contrast (in logits).
fn search(fixtures: &mut Fixtures, query: &str) -> Vec<String> {
    let embedding = fixtures.text.embed(query).unwrap();
    let rivals = labels::search_rivals(
        &embedding,
        &fixtures.bank,
        &fixtures.labels,
        QUERY_LABEL_MATCH,
    );
    let mut kept: Vec<(f32, String)> = fixtures
        .photos
        .iter()
        .filter_map(|(name, photo)| {
            let own = cosine(photo, &embedding);
            let beats_rivals = rivals
                .iter()
                .all(|(_, rival)| LOGIT_SCALE * (own - cosine(photo, rival)) >= SEARCH_CONTRAST);
            (own >= search_min_cosine() && beats_rivals).then(|| (own, name.clone()))
        })
        .collect();
    kept.sort_by(|a, b| b.0.total_cmp(&a.0));
    kept.into_iter().map(|(_, name)| name).collect()
}

#[test]
fn pets_get_their_own_tag_and_not_the_other() {
    let Some(fixtures) = load() else {
        return;
    };
    let thresholds = labels::thresholds(CLIP_MODEL_ID).unwrap();
    let tags = |name: &str| -> Vec<String> {
        let (_, photo) = fixtures.photos.iter().find(|(n, _)| n == name).unwrap();
        labels::select(
            &labels::score(photo, &fixtures.bank),
            &fixtures.labels,
            thresholds,
            TOP_K,
        )
        .into_iter()
        .map(|(slug, _)| slug)
        .collect()
    };
    for (photos, own, other) in [
        (["dog-a", "dog-b"], "dog", "cat"),
        (["cat-a", "cat-b"], "cat", "dog"),
    ] {
        for name in photos {
            let tags = tags(name);
            assert!(tags.iter().any(|t| t == own), "{name}: {tags:?}");
            assert!(!tags.iter().any(|t| t == other), "{name}: {tags:?}");
        }
    }
    // Fair readings of flat shapes and studio portraits; nothing else.
    let related = ["flag", "sign", "art", "portrait"];
    for name in [
        "koch-a",
        "glover-a",
        "red_circle",
        "blue_square",
        "green_triangle",
    ] {
        for tag in tags(name) {
            assert!(
                related.contains(&tag.as_str()),
                "{name}: unrelated tag {tag}"
            );
        }
    }
}

#[test]
fn searches_find_their_photos_in_any_language() {
    let Some(mut fixtures) = load() else {
        return;
    };
    for (query, expected) in [
        ("dog", &["dog-a", "dog-b"][..]),
        ("chien", &["dog-a", "dog-b"]),
        ("cat", &["cat-a", "cat-b"]),
        ("chat", &["cat-a", "cat-b"]),
        ("red circle", &["red_circle"]),
        ("cercle rouge", &["red_circle"]),
        ("a green triangle", &["green_triangle"]),
    ] {
        let mut found = search(&mut fixtures, query);
        found.sort();
        assert_eq!(found, expected, "{query}");
    }
}

/// "dog" is close to the `dog` label, so `dog`'s siblings compete: a photo
/// where `cat` scores better than the query is dropped even when the query
/// alone passes the floor.
#[test]
fn a_search_beaten_by_a_rival_label_drops_the_photo() {
    let Some(mut fixtures) = load() else {
        return;
    };
    let query = fixtures.text.embed("dog").unwrap();
    let rivals = labels::search_rivals(&query, &fixtures.bank, &fixtures.labels, QUERY_LABEL_MATCH);
    let names: Vec<&str> = rivals.iter().map(|(slug, _)| *slug).collect();
    assert!(
        names.contains(&"cat") && !names.contains(&"dog"),
        "{names:?}"
    );
    let (_, cat) = fixtures.photos.iter().find(|(n, _)| n == "cat-a").unwrap();
    let logit = |text: &[f32]| LOGIT_SCALE * cosine(cat, text) + LOGIT_BIAS;
    let best_rival = rivals
        .iter()
        .map(|(_, r)| logit(r))
        .fold(f32::MIN, f32::max);
    assert!(
        best_rival - logit(&query) > SEARCH_CONTRAST,
        "cat wins on the cat photo"
    );
}
