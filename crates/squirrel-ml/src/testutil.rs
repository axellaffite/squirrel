//! Locating the native pieces model-dependent tests need. Unset, a test is
//! skipped (`None`), except on CI (`CI` set), where skipping would hide a
//! broken setup: it panics. Same contract as `squirrel_media::testutil::test_pdfium_dir`.

use std::path::PathBuf;

use crate::models::{ModelDir, ModelGroup};

/// The pinned ONNX Runtime library (`scripts/fetch-onnxruntime.sh`): `SQUIRREL_ORT_DYLIB`.
pub fn test_ort() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SQUIRREL_ORT_DYLIB") {
        return Some(path.into());
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI is set but SQUIRREL_ORT_DYLIB is not: run scripts/fetch-onnxruntime.sh"
    );
    eprintln!("skipped: SQUIRREL_ORT_DYLIB is not set");
    None
}

/// The pinned models (`scripts/fetch-models.sh`): `SQUIRREL_MODELS_DIR`, with
/// every group installed.
pub fn test_models() -> Option<ModelDir> {
    let Some(path) = std::env::var_os("SQUIRREL_MODELS_DIR") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI is set but SQUIRREL_MODELS_DIR is not: run scripts/fetch-models.sh"
        );
        eprintln!("skipped: SQUIRREL_MODELS_DIR is not set");
        return None;
    };
    let dir = ModelDir::new(path);
    for group in ModelGroup::ALL {
        assert!(
            dir.installed(group),
            "SQUIRREL_MODELS_DIR {} lacks the {} models: run scripts/fetch-models.sh",
            dir.path().display(),
            group.name()
        );
    }
    Some(dir)
}

/// Both the runtime and the models, or `None` (see the module docs).
pub fn test_ml() -> Option<(PathBuf, ModelDir)> {
    Some((test_ort()?, test_models()?))
}

/// Generated test pictures: no third-party images in the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    RedCircle,
    BlueSquare,
    GreenTriangle,
}

impl Shape {
    pub const ALL: [Self; 3] = [Self::RedCircle, Self::BlueSquare, Self::GreenTriangle];

    pub fn name(self) -> &'static str {
        match self {
            Self::RedCircle => "red_circle",
            Self::BlueSquare => "blue_square",
            Self::GreenTriangle => "green_triangle",
        }
    }

    /// What CLIP should read in it.
    pub fn prompt(self) -> &'static str {
        match self {
            Self::RedCircle => "a red circle",
            Self::BlueSquare => "a blue square",
            Self::GreenTriangle => "a green triangle",
        }
    }
}

/// A `size` × `size` white picture with one large coloured shape in the middle.
pub fn shape_image(shape: Shape, size: u32) -> image::RgbImage {
    let s = size as f32;
    let (cx, cy) = (s / 2.0, s / 2.0);
    let inside = |x: f32, y: f32| match shape {
        Shape::RedCircle => (x - cx).powi(2) + (y - cy).powi(2) <= (s * 0.35).powi(2),
        Shape::BlueSquare => (x - cx).abs() <= s * 0.3 && (y - cy).abs() <= s * 0.3,
        Shape::GreenTriangle => {
            // Apex at the top, base at the bottom.
            let (top, bottom) = (s * 0.15, s * 0.85);
            let half = (y - top) / (bottom - top) * s * 0.35;
            (top..=bottom).contains(&y) && (x - cx).abs() <= half
        }
    };
    let colour = match shape {
        Shape::RedCircle => image::Rgb([220, 30, 30]),
        Shape::BlueSquare => image::Rgb([30, 60, 220]),
        Shape::GreenTriangle => image::Rgb([30, 170, 60]),
    };
    image::RgbImage::from_fn(size, size, |x, y| {
        if inside(x as f32 + 0.5, y as f32 + 0.5) {
            colour
        } else {
            image::Rgb([255, 255, 255])
        }
    })
}
