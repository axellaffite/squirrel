//! Writes the golden CLIP image embeddings of the generated shapes, once, by
//! a human with the pinned runtime and models:
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… \
//!       cargo run -p squirrel-ml --features test-util --example make_golden
//!
//! Output: `tests/fixtures/clip/golden-openai-vit-b32-224.json` and
//! `tests/fixtures/faces/golden-yunet-sface-2021dec.json`.

use std::collections::BTreeMap;

use squirrel_ml::clip::ClipImage;
use squirrel_ml::models::{CLIP_MODEL_ID, ModelGroup};
use squirrel_ml::testutil::{Shape, shape_image, test_ml_for};

fn main() -> anyhow::Result<()> {
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipImage, ModelGroup::Faces])
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    models.verify(ModelGroup::ClipImage)?;
    squirrel_ml::runtime::init(&dylib)?;
    let mut clip = ClipImage::load(&models, 1)?;
    let mut shapes = BTreeMap::new();
    for shape in Shape::ALL {
        let embedding = clip.embed(&shape_image(shape, 512))?;
        let rounded: Vec<f64> = embedding
            .iter()
            .map(|v| (f64::from(*v) * 1e6).round() / 1e6)
            .collect();
        shapes.insert(shape.name(), rounded);
    }
    let golden = serde_json::json!({ "model": CLIP_MODEL_ID, "size": 512, "shapes": shapes });
    let path = format!(
        "{}/tests/fixtures/clip/golden-{CLIP_MODEL_ID}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::write(&path, serde_json::to_string(&golden)? + "\n")?;
    println!("wrote {path}");
    faces(&models)
}

/// The SFace embedding of each committed portrait's single face.
fn faces(models: &squirrel_ml::ModelDir) -> anyhow::Result<()> {
    let mut analyzer = squirrel_ml::faces::FaceAnalyzer::load(models, 1)?;
    let mut faces = BTreeMap::new();
    for name in ["biden-2021", "biden-2013", "harris-2021", "harris-2017"] {
        let path = format!(
            "{}/tests/fixtures/faces/{name}.jpg",
            env!("CARGO_MANIFEST_DIR")
        );
        let found = analyzer.analyze(&image::open(path)?.to_rgb8())?;
        anyhow::ensure!(found.len() == 1, "{name}: {} faces", found.len());
        let rounded: Vec<f64> = found[0]
            .embedding
            .iter()
            .map(|v| (f64::from(*v) * 1e6).round() / 1e6)
            .collect();
        faces.insert(name, rounded);
    }
    let golden = serde_json::json!({ "model": squirrel_ml::FACE_MODEL_ID, "faces": faces });
    let path = format!(
        "{}/tests/fixtures/faces/golden-{}.json",
        env!("CARGO_MANIFEST_DIR"),
        squirrel_ml::FACE_MODEL_ID
    );
    std::fs::write(&path, serde_json::to_string(&golden)? + "\n")?;
    println!("wrote {path}");
    Ok(())
}
