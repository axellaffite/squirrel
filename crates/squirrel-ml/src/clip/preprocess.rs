//! Image preprocessing, as the model's `visual/preprocess_cfg.json` describes
//! it (OpenCLIP): either squash to the input size (SigLIP2) or shortest side to
//! the input size and centre crop (OpenAI CLIP), both bicubic; then
//! `(x / 255 − mean) / std` per channel, NCHW.

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, RgbImage};
use ndarray::Array4;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum PreprocessError {
    #[error("preprocess_cfg.json: {0}")]
    Config(String),
    #[error("resizing: {0}")]
    Resize(String),
}

/// `visual/preprocess_cfg.json`, checked: only what this code implements.
#[derive(Debug, Clone, PartialEq)]
pub struct PreprocessCfg {
    pub size: u32,
    pub resize: ResizeMode,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

/// OpenCLIP's `resize_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    /// Both sides to the input size: the aspect ratio is not kept.
    Squash,
    /// Shortest side to the input size, then a centre crop.
    Shortest,
}

#[derive(Deserialize)]
struct RawCfg {
    size: [u32; 2],
    mode: String,
    mean: [f32; 3],
    std: [f32; 3],
    interpolation: String,
    resize_mode: String,
}

impl PreprocessCfg {
    pub fn parse(json: &[u8]) -> Result<Self, PreprocessError> {
        let raw: RawCfg =
            serde_json::from_slice(json).map_err(|err| PreprocessError::Config(err.to_string()))?;
        let unsupported = |what: &str, value: &str| {
            Err(PreprocessError::Config(format!(
                "unsupported {what} {value:?}"
            )))
        };
        if raw.size[0] != raw.size[1] || raw.size[0] == 0 {
            return unsupported("size", &format!("{:?}", raw.size));
        }
        if raw.mode != "RGB" {
            return unsupported("mode", &raw.mode);
        }
        if raw.interpolation != "bicubic" {
            return unsupported("interpolation", &raw.interpolation);
        }
        let resize = match raw.resize_mode.as_str() {
            "squash" => ResizeMode::Squash,
            "shortest" => ResizeMode::Shortest,
            other => return unsupported("resize_mode", other),
        };
        if raw.std.iter().any(|s| *s <= 0.0) {
            return unsupported("std", &format!("{:?}", raw.std));
        }
        Ok(Self {
            size: raw.size[0],
            resize,
            mean: raw.mean,
            std: raw.std,
        })
    }
}

/// Resize as `cfg.resize` says (squash: torchvision's `Resize((s, s))`;
/// shortest: `Resize(int)`, the long side truncated, then a centre crop with
/// torchvision's rounding), normalise.
pub fn preprocess(image: &RgbImage, cfg: &PreprocessCfg) -> Result<Array4<f32>, PreprocessError> {
    let size = cfg.size;
    let (new_w, new_h) = match cfg.resize {
        ResizeMode::Squash => (size, size),
        ResizeMode::Shortest => shortest_side_to(image.dimensions(), size)?,
    };
    let resized = if (new_w, new_h) == image.dimensions() {
        image.clone()
    } else {
        let src = DynamicImage::ImageRgb8(image.clone());
        let mut dst = DynamicImage::new_rgb8(new_w, new_h);
        let options =
            ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
        Resizer::new()
            .resize(&src, &mut dst, &options)
            .map_err(|err| PreprocessError::Resize(err.to_string()))?;
        dst.into_rgb8()
    };
    let left = crop_offset(new_w, size);
    let top = crop_offset(new_h, size);
    let side = size as usize;
    Ok(Array4::from_shape_fn((1, 3, side, side), |(_, c, y, x)| {
        let pixel = resized.get_pixel(left + x as u32, top + y as u32);
        (f32::from(pixel[c]) / 255.0 - cfg.mean[c]) / cfg.std[c]
    }))
}

fn shortest_side_to((width, height): (u32, u32), size: u32) -> Result<(u32, u32), PreprocessError> {
    let (short, long) = (width.min(height).max(1), width.max(height));
    let long_scaled = ((u64::from(size) * u64::from(long)) / u64::from(short)).max(u64::from(size));
    let long_scaled =
        u32::try_from(long_scaled).map_err(|_| PreprocessError::Resize("too wide".into()))?;
    Ok(if width <= height {
        (size, long_scaled)
    } else {
        (long_scaled, size)
    })
}

/// torchvision's `CenterCrop`: `int(round((length - size) / 2))`.
fn crop_offset(length: u32, size: u32) -> u32 {
    (f64::from(length - size) / 2.0).round_ties_even() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAI: &str = r#"{"size": [224, 224], "mode": "RGB", "mean": [0.48145466, 0.4578275, 0.40821073], "std": [0.26862954, 0.26130258, 0.27577711], "interpolation": "bicubic", "resize_mode": "shortest", "fill_color": 0}"#;

    /// The pinned SigLIP2 export's `visual/preprocess_cfg.json`.
    const SIGLIP2: &str = r#"{"size": [224, 224], "mode": "RGB", "mean": [0.5, 0.5, 0.5], "std": [0.5, 0.5, 0.5], "interpolation": "bicubic", "resize_mode": "squash", "fill_color": 0}"#;

    fn cfg() -> PreprocessCfg {
        PreprocessCfg::parse(OPENAI.as_bytes()).unwrap()
    }

    #[test]
    fn the_models_config_parses() {
        let cfg = PreprocessCfg::parse(SIGLIP2.as_bytes()).unwrap();
        assert_eq!(
            (cfg.size, cfg.resize, cfg.mean, cfg.std),
            (224, ResizeMode::Squash, [0.5; 3], [0.5; 3])
        );
        let cfg = self::cfg();
        assert_eq!(cfg.resize, ResizeMode::Shortest);
        assert!((cfg.mean[0] - 0.481_454_66).abs() < 1e-7);
    }

    #[test]
    fn squash_keeps_the_whole_image() {
        // 448 × 224: red left half, blue right half; both survive the squash.
        let image = RgbImage::from_fn(448, 224, |x, _| {
            if x < 224 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        let cfg = PreprocessCfg::parse(SIGLIP2.as_bytes()).unwrap();
        let tensor = preprocess(&image, &cfg).unwrap();
        assert_eq!(tensor.shape(), [1, 3, 224, 224]);
        // (1 − 0.5) / 0.5 = 1, (0 − 0.5) / 0.5 = −1.
        assert!((tensor[[0, 0, 100, 10]] - 1.0).abs() < 1e-5);
        assert!((tensor[[0, 2, 100, 10]] + 1.0).abs() < 1e-5);
        assert!((tensor[[0, 0, 100, 213]] + 1.0).abs() < 1e-5);
        assert!((tensor[[0, 2, 100, 213]] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn unknown_modes_are_refused() {
        for (from, to) in [
            ("\"shortest\"", "\"longest\""),
            ("\"bicubic\"", "\"bilinear\""),
            ("\"RGB\"", "\"BGR\""),
        ] {
            let json = OPENAI.replace(from, to);
            let err = PreprocessCfg::parse(json.as_bytes()).unwrap_err();
            assert!(err.to_string().contains(to.trim_matches('"')), "{err}");
        }
    }

    #[test]
    fn a_wide_image_keeps_only_its_central_square() {
        // 448 × 224: red quarters on both sides, blue in the middle.
        let image = RgbImage::from_fn(448, 224, |x, _| {
            if (112..336).contains(&x) {
                image::Rgb([0, 0, 255])
            } else {
                image::Rgb([255, 0, 0])
            }
        });
        let cfg = cfg();
        let tensor = preprocess(&image, &cfg).unwrap();
        assert_eq!(tensor.shape(), [1, 3, 224, 224]);
        let red_of_blue = -cfg.mean[0] / cfg.std[0];
        let blue = (1.0 - cfg.mean[2]) / cfg.std[2];
        for x in [0, 100, 223] {
            assert!((tensor[[0, 0, 50, x]] - red_of_blue).abs() < 1e-5, "x={x}");
            assert!((tensor[[0, 2, 50, x]] - blue).abs() < 1e-5, "x={x}");
        }
    }

    #[test]
    fn white_and_black_map_to_the_normalised_extremes() {
        let cfg = cfg();
        let white =
            preprocess(&RgbImage::from_pixel(300, 300, image::Rgb([255; 3])), &cfg).unwrap();
        let black = preprocess(&RgbImage::from_pixel(224, 224, image::Rgb([0; 3])), &cfg).unwrap();
        let expected_white = [1.9303, 2.0749, 2.1459];
        for c in 0..3 {
            assert!(
                (white[[0, c, 10, 10]] - expected_white[c]).abs() < 1e-3,
                "{c}"
            );
            let expected_black = -cfg.mean[c] / cfg.std[c];
            assert!((black[[0, c, 10, 10]] - expected_black).abs() < 1e-5, "{c}");
        }
    }

    #[test]
    fn tall_images_are_cropped_vertically() {
        let image = RgbImage::from_fn(224, 448, |_, y| {
            if y < 100 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 255, 0])
            }
        });
        let tensor = preprocess(&image, &cfg()).unwrap();
        let green = (1.0 - cfg().mean[1]) / cfg().std[1];
        assert!((tensor[[0, 1, 0, 0]] - green).abs() < 1e-5);
    }

    #[test]
    fn crop_offsets_round_like_torchvision() {
        assert_eq!(crop_offset(224, 224), 0);
        assert_eq!(crop_offset(448, 224), 112);
        assert_eq!(crop_offset(225, 224), 0);
        assert_eq!(crop_offset(227, 224), 2);
    }
}
