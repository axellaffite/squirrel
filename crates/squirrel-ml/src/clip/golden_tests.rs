//! Pins the image tower's output on generated pictures (golden vectors from
//! `examples/make_golden.rs`), and checks that the text tower reads them.

use std::collections::HashMap;

use super::{ClipImage, ClipText, cosine};
use crate::models::{CLIP_MODEL_ID, ModelGroup};
use crate::testutil::{Shape, shape_image, test_ml_for};

#[derive(serde::Deserialize)]
struct Golden {
    model: String,
    size: u32,
    shapes: HashMap<String, Vec<f32>>,
}

fn golden() -> Golden {
    let raw = include_str!("../../tests/fixtures/clip/golden-siglip2-vit-b16-224.json");
    serde_json::from_str(raw).unwrap()
}

#[test]
fn the_golden_file_is_for_the_pinned_model() {
    let golden = golden();
    assert_eq!(golden.model, CLIP_MODEL_ID);
    for shape in Shape::ALL {
        let vector = &golden.shapes[shape.name()];
        assert_eq!(vector.len(), super::EMBEDDING_DIM);
        let norm = cosine(vector, vector).sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "{norm}");
    }
}

/// CPU kernels differ slightly across x86/ARM and versions: cosine ≥ 0.999.
#[test]
fn image_embeddings_match_the_golden_vectors() {
    let Some((dylib, models)) = test_ml_for(&[ModelGroup::ClipImage]) else {
        return;
    };
    models.verify(ModelGroup::ClipImage).unwrap();
    crate::runtime::init(&dylib).unwrap();
    let mut clip = ClipImage::load(&models, 2).unwrap();
    let golden = golden();
    for shape in Shape::ALL {
        let embedding = clip.embed(&shape_image(shape, golden.size)).unwrap();
        let norm = cosine(&embedding, &embedding).sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "{norm}");
        let similarity = cosine(&embedding, &golden.shapes[shape.name()]);
        assert!(similarity >= 0.999, "{}: {similarity}", shape.name());
    }
}

/// Each picture is closest to its own description among the three.
#[test]
fn the_text_tower_reads_the_shapes() {
    let Some((dylib, models)) = test_ml_for(&[ModelGroup::ClipText]) else {
        return;
    };
    crate::runtime::init(&dylib).unwrap();
    let mut text = ClipText::load(&models, 2).unwrap();
    let prompts: Vec<Vec<f32>> = Shape::ALL
        .iter()
        .map(|shape| text.embed(shape.prompt()).unwrap())
        .collect();
    let golden = golden();
    for (index, shape) in Shape::ALL.iter().enumerate() {
        let image = &golden.shapes[shape.name()];
        let scores: Vec<f32> = prompts.iter().map(|p| cosine(image, p)).collect();
        let best = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(best, index, "{}: {scores:?}", shape.name());
    }
}
