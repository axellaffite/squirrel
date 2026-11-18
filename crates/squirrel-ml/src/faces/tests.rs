//! With the real models: the committed public-domain portraits.

use std::path::PathBuf;

use image::RgbImage;

use super::Detector;
use crate::models::ModelGroup;
use crate::testutil::test_ml_for;

pub(crate) const PORTRAITS: [&str; 4] = ["biden-2021", "biden-2013", "harris-2021", "harris-2017"];

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
