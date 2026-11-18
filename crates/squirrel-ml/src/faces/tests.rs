//! With the real models: the committed public-domain portraits.

use std::path::PathBuf;

use image::RgbImage;

use super::Detector;
use crate::models::ModelGroup;
use crate::testutil::test_ml_for;

pub(crate) const PORTRAITS: [&str; 4] = ["koch-a", "koch-b", "glover-a", "glover-b"];

pub(crate) fn portrait(name: &str) -> RgbImage {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests/fixtures/faces",
        &format!("{name}.jpg"),
    ]
    .iter()
    .collect();
    image::open(path).unwrap().to_rgb8()
}

fn detector() -> Option<Detector> {
    let (dylib, models) = test_ml_for(&[ModelGroup::Faces])?;
    crate::runtime::init(&dylib).unwrap();
    Some(Detector::load(&models, 2).unwrap())
}

#[test]
fn each_portrait_has_one_face_with_its_eyes_inside() {
    let Some(mut detector) = detector() else {
        return;
    };
    for name in PORTRAITS {
        let faces = detector.detect(&portrait(name)).unwrap();
        assert_eq!(faces.len(), 1, "{name}: {faces:?}");
        let [x, y, w, h] = faces[0].bbox;
        for [ex, ey] in &faces[0].landmarks[..2] {
            assert!(
                (x..x + w).contains(ex) && (y..y + h).contains(ey),
                "{name}: eye outside"
            );
        }
        assert!(
            faces[0].landmarks[0][0] < faces[0].landmarks[1][0],
            "{name}: image-left eye first"
        );
    }
}

#[test]
fn a_landscape_has_no_face() {
    let Some(mut detector) = detector() else {
        return;
    };
    let landscape = RgbImage::from_fn(640, 420, |x, y| {
        if y < 200 {
            image::Rgb([120, 170, 230])
        } else {
            image::Rgb([60, (120 + x % 40) as u8, 50])
        }
    });
    assert!(detector.detect(&landscape).unwrap().is_empty());
}

fn analyzer() -> Option<super::FaceAnalyzer> {
    let (dylib, models) = test_ml_for(&[ModelGroup::Faces])?;
    crate::runtime::init(&dylib).unwrap();
    Some(super::FaceAnalyzer::load(&models, 2).unwrap())
}

#[test]
fn the_same_person_matches_and_two_people_do_not() {
    let Some(mut analyzer) = analyzer() else {
        return;
    };
    let embed = |analyzer: &mut super::FaceAnalyzer, name: &str| {
        let faces = analyzer.analyze(&portrait(name)).unwrap();
        assert_eq!(faces.len(), 1, "{name}");
        faces[0].embedding.clone()
    };
    let k1 = embed(&mut analyzer, "koch-a");
    let k2 = embed(&mut analyzer, "koch-b");
    let g1 = embed(&mut analyzer, "glover-a");
    let g2 = embed(&mut analyzer, "glover-b");
    let cos = crate::clip::cosine;
    for (same, a, b) in [("koch", &k1, &k2), ("glover", &g1, &g2)] {
        assert!(cos(a, b) >= super::SAME_PERSON, "{same}: {}", cos(a, b));
    }
    for (a, b) in [(&k1, &g1), (&k1, &g2), (&k2, &g1), (&k2, &g2)] {
        assert!(
            cos(a, b) < super::SAME_PERSON,
            "different people: {}",
            cos(a, b)
        );
    }
}

#[test]
fn faces_are_relative_to_the_preview() {
    let Some(mut analyzer) = analyzer() else {
        return;
    };
    let faces = analyzer.analyze(&portrait("koch-b")).unwrap();
    let [x, y, w, h] = faces[0].bbox;
    assert!(
        x > 0.0 && y > 0.0 && x + w < 1.0 && y + h < 1.0,
        "{:?}",
        faces[0].bbox
    );
    assert!(faces[0].landmarks.iter().all(|v| (0.0..=1.0).contains(v)));
    // A large preview gives the same relative box as the original.
    let big = image::imageops::resize(
        &portrait("koch-b"),
        1000,
        1250,
        image::imageops::FilterType::CatmullRom,
    );
    let on_big = analyzer.analyze(&big).unwrap();
    for (a, b) in faces[0].bbox.iter().zip(on_big[0].bbox) {
        assert!(
            (a - b).abs() < 0.02,
            "{:?} vs {:?}",
            faces[0].bbox,
            on_big[0].bbox
        );
    }
}

#[derive(serde::Deserialize)]
struct Golden {
    faces: std::collections::HashMap<String, Vec<f32>>,
}

/// Golden embeddings from `examples/make_golden.rs`: ≥ 0.999 cosine.
#[test]
fn face_embeddings_match_the_golden_vectors() {
    let Some(mut analyzer) = analyzer() else {
        return;
    };
    let golden: Golden = serde_json::from_str(include_str!(
        "../../tests/fixtures/faces/golden-yunet-sface-2021dec.json"
    ))
    .unwrap();
    for name in PORTRAITS {
        let faces = analyzer.analyze(&portrait(name)).unwrap();
        let similarity = crate::clip::cosine(&faces[0].embedding, &golden.faces[name]);
        assert!(similarity >= 0.999, "{name}: {similarity}");
    }
}
