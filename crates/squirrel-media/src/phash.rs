//! A 64-bit perceptual hash (DCT pHash) of an image, for finding photos that
//! look the same although their bytes differ (resized, re-compressed, lightly
//! edited copies).
//!
//! The image (already upright) is reduced to a 32 × 32 grey copy, a 2-D
//! DCT-II is taken, and the 8 × 8 lowest frequencies are compared with their
//! median: each bit says whether a coefficient is above it. Two hashes are
//! compared by Hamming distance ([`distance`]).

use image::DynamicImage;

use crate::thumbs::{ThumbError, resize};

/// Side of the grey copy the DCT runs on.
const SIDE: usize = 32;
/// Side of the block of low frequencies kept (8 × 8 = 64 bits).
const LOW: usize = 8;

/// The perceptual hash of `image` (upright).
pub fn phash(image: &DynamicImage) -> Result<u64, ThumbError> {
    let small = resize(image, SIDE as u32, SIDE as u32)?;
    Ok(phash_of_grey(&grey(&small)))
}

/// Number of bits that differ between two hashes (0 = same, 64 = opposite).
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Luma (BT.601) of a 32 × 32 copy, alpha composited on black.
fn grey(small: &DynamicImage) -> Vec<f64> {
    let rgba = small.to_rgba8();
    rgba.pixels()
        .map(|p| {
            let [r, g, b, a] = p.0.map(f64::from);
            (0.299 * r + 0.587 * g + 0.114 * b) * a / 255.0
        })
        .collect()
}

/// The hash of a `SIDE × SIDE` grey image, row-major.
fn phash_of_grey(pixels: &[f64]) -> u64 {
    debug_assert_eq!(pixels.len(), SIDE * SIDE);
    let basis = dct_basis();
    // Rows first, keeping only the low frequencies of each row.
    let mut rows = vec![0.0; SIDE * LOW];
    for y in 0..SIDE {
        for u in 0..LOW {
            rows[y * LOW + u] = (0..SIDE).map(|x| pixels[y * SIDE + x] * basis[u][x]).sum();
        }
    }
    let mut low = [0.0; LOW * LOW];
    for v in 0..LOW {
        for u in 0..LOW {
            low[v * LOW + u] = (0..SIDE).map(|y| rows[y * LOW + u] * basis[v][y]).sum();
        }
    }
    let median = median(&low);
    low.iter()
        .enumerate()
        .filter(|(_, c)| **c > median)
        .fold(0u64, |hash, (bit, _)| hash | (1 << bit))
}

/// `basis[k][n]` = cos(π (2n + 1) k / 2N), the unnormalised DCT-II basis (a
/// constant factor per frequency changes no comparison with the median).
fn dct_basis() -> [[f64; SIDE]; LOW] {
    let mut basis = [[0.0; SIDE]; LOW];
    for (k, row) in basis.iter_mut().enumerate() {
        for (n, cell) in row.iter_mut().enumerate() {
            *cell =
                (std::f64::consts::PI * (2 * n + 1) as f64 * k as f64 / (2 * SIDE) as f64).cos();
        }
    }
    basis
}

fn median(values: &[f64; LOW * LOW]) -> f64 {
    let mut sorted = *values;
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    (sorted[mid - 1] + sorted[mid]) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scene;
    use image::{ImageFormat, RgbImage};

    fn jpeg_round_trip(image: &DynamicImage, quality: u8) -> DynamicImage {
        let mut bytes = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
        image.to_rgb8().write_with_encoder(encoder).unwrap();
        image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).unwrap()
    }

    fn brighter(image: &DynamicImage, by: i16) -> DynamicImage {
        let mut rgb = image.to_rgb8();
        for p in rgb.pixels_mut() {
            p.0 = p.0.map(|v| (i16::from(v) + by).clamp(0, 255) as u8);
        }
        DynamicImage::ImageRgb8(rgb)
    }

    #[test]
    fn distance_counts_differing_bits() {
        assert_eq!(distance(0, 0), 0);
        assert_eq!(distance(0b1011, 0b0001), 2);
        assert_eq!(distance(0, u64::MAX), 64);
    }

    #[test]
    fn copies_of_one_photo_stay_close() {
        for seed in 0..12 {
            let original = scene(seed, 1600, 1200);
            let reference = phash(&original).unwrap();
            let copies = [
                (
                    "resized",
                    original.resize_exact(800, 600, image::imageops::Triangle),
                ),
                (
                    "thumbnail",
                    original.resize_exact(480, 360, image::imageops::Lanczos3),
                ),
                ("re-encoded", jpeg_round_trip(&original, 55)),
                ("brighter", brighter(&original, 12)),
                (
                    "small re-encoded",
                    jpeg_round_trip(
                        &original.resize_exact(640, 480, image::imageops::Triangle),
                        70,
                    ),
                ),
            ];
            for (name, copy) in copies {
                let d = distance(reference, phash(&copy).unwrap());
                assert!(d <= 6, "seed {seed}, {name}: distance {d}");
            }
        }
    }

    #[test]
    fn different_photos_are_far_apart() {
        let hashes: Vec<u64> = (100..124)
            .map(|seed| phash(&scene(seed, 640, 480)).unwrap())
            .collect();
        for (i, a) in hashes.iter().enumerate() {
            for b in &hashes[i + 1..] {
                let d = distance(*a, *b);
                assert!(d > 12, "distance {d} between two different scenes");
            }
        }
    }

    #[test]
    fn rotating_a_photo_changes_its_hash() {
        let original = scene(7, 800, 600);
        let turned = original.rotate90();
        assert!(distance(phash(&original).unwrap(), phash(&turned).unwrap()) > 12);
    }

    #[test]
    fn a_flat_image_hashes_without_panicking() {
        let flat = DynamicImage::ImageRgb8(RgbImage::from_pixel(50, 40, image::Rgb([90, 90, 90])));
        phash(&flat).unwrap();
    }
}
