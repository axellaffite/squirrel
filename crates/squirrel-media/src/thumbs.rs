//! Decoding (bounded), orientation, resizing, WebP encoding and blurhash.

use std::io::{BufRead, Seek, SeekFrom};

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

/// Decoding limits for untrusted images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DecodeLimits {
    /// Width × height above which an image is refused before decoding.
    pub max_pixels: u64,
    /// Largest single allocation the decoder may make.
    pub max_alloc: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_pixels: 100_000_000,       // 100 MP: every phone and most cameras.
            max_alloc: 1024 * 1024 * 1024, // 1 GiB.
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    #[error("image of {0}×{1} pixels exceeds the limit")]
    TooLarge(u32, u32),
    #[error("unsupported or corrupt image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("resize failed: {0}")]
    Resize(String),
    #[error("blurhash failed: {0}")]
    Blurhash(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// A decoded image, upright (EXIF orientation applied), with its stored size.
/// Peak memory of a render is about this many times the decoded buffer
/// (the buffer, a pixel-format conversion, and the resizer's working copy).
const DECODE_MEMORY_FACTOR: u64 = 3;

/// A decoded image as stored, with the orientation that makes it upright.
/// Orientation is applied after resizing ([`Decoded::upright`] for the full size).
pub struct Decoded {
    pub image: DynamicImage,
    pub orientation: Orientation,
    /// Dimensions as displayed (after orientation).
    pub width: u32,
    pub height: u32,
}

impl Decoded {
    /// The full-size image, upright.
    pub fn upright(self) -> DynamicImage {
        let mut image = self.image;
        image.apply_orientation(self.orientation);
        image
    }
}

/// Whether `orientation` turns the image a quarter turn (width and height swap).
pub fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// Decodes JPEG/PNG/GIF/WebP/TIFF/BMP (and HEIC/HEIF/AVIF with the `heic`
/// feature) with limits checked before any pixel buffer is allocated.
pub fn decode<R: BufRead + Seek>(
    mut reader: R,
    limits: DecodeLimits,
) -> Result<Decoded, ThumbError> {
    let heif = is_iso_bmff(&mut reader)?;
    let mut reader = ImageReader::new(reader).with_guessed_format()?;
    let mut image_limits = Limits::default();
    image_limits.max_alloc = Some(limits.max_alloc);
    reader.limits(image_limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > limits.max_pixels
        || decoder.total_bytes().saturating_mul(DECODE_MEMORY_FACTOR) > limits.max_alloc
    {
        return Err(ThumbError::TooLarge(w, h));
    }
    // libheif applies the container's rotation and mirroring itself.
    let orientation = if heif {
        Orientation::NoTransforms
    } else {
        decoder.orientation()?
    };
    let image = DynamicImage::from_decoder(decoder)?;
    let (width, height) = if swaps_axes(orientation) {
        (image.height(), image.width())
    } else {
        (image.width(), image.height())
    };
    Ok(Decoded {
        image,
        orientation,
        width,
        height,
    })
}

/// HEIF-family files are ISO-BMFF: bytes 4..8 are `ftyp`. Leaves the reader at the start.
fn is_iso_bmff<R: BufRead + Seek>(reader: &mut R) -> std::io::Result<bool> {
    let head = reader.fill_buf()?;
    let found = head.get(4..8) == Some(b"ftyp".as_slice());
    reader.seek(SeekFrom::Start(0))?;
    Ok(found)
}

/// Size of a variant that fits `max_w` × `max_h` keeping the aspect ratio,
/// never upscaling, never 0.
pub fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_w) / f64::from(width),
            f64::from(max_h) / f64::from(height),
        ),
    );
    let w = ((f64::from(width) * scale).round() as u32).max(1);
    let h = ((f64::from(height) * scale).round() as u32).max(1);
    (w, h)
}

/// Resized copy (Lanczos3) as 8-bit RGB or RGBA (kept only when the source has alpha).
/// An 8-bit RGB(A) source is resized as is, without a full-size copy.
pub fn resize(src: &DynamicImage, width: u32, height: u32) -> Result<DynamicImage, ThumbError> {
    let converted;
    let src = match src {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => src,
        other if other.color().has_alpha() => {
            converted = DynamicImage::ImageRgba8(other.to_rgba8());
            &converted
        }
        other => {
            converted = DynamicImage::ImageRgb8(other.to_rgb8());
            &converted
        }
    };
    let mut dst = DynamicImage::new(width, height, src.color());
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(src, &mut dst, &options)
        .map_err(|err| ThumbError::Resize(err.to_string()))?;
    Ok(dst)
}

/// Lossy WebP (libwebp) at `quality` 0–100.
pub fn encode_webp(image: &DynamicImage, quality: f32) -> Vec<u8> {
    let (w, h) = (image.width(), image.height());
    match image {
        DynamicImage::ImageRgba8(buf) => webp::Encoder::from_rgba(buf.as_raw(), w, h)
            .encode(quality)
            .to_vec(),
        other => {
            let rgb = other.to_rgb8();
            webp::Encoder::from_rgb(rgb.as_raw(), w, h)
                .encode(quality)
                .to_vec()
        }
    }
}

/// 4×3-component blurhash computed on a ≤ 32 px copy (fast, same result visually).
pub fn blurhash_of(image: &DynamicImage) -> Result<String, ThumbError> {
    let (w, h) = fit(image.width(), image.height(), 32, 32);
    let small = resize(image, w, h)?.to_rgba8();
    blurhash::encode(4, 3, w, h, small.as_raw())
        .map_err(|err| ThumbError::Blurhash(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};
    use std::io::Cursor;

    #[test]
    fn fit_keeps_the_ratio_and_never_upscales() {
        assert_eq!(fit(4000, 3000, 1440, 1440), (1440, 1080));
        assert_eq!(fit(3000, 4000, 1080, 360), (270, 360));
        assert_eq!(fit(100, 50, 1440, 1440), (100, 50));
        assert_eq!(fit(10_000, 1, 1080, 360), (1080, 1));
    }

    #[test]
    fn orientation_is_applied_when_decoding() {
        let jpeg = jpeg_sized_with_exif(
            64,
            32,
            &ExifSpec {
                orientation: Some(6),
                ..ExifSpec::default()
            },
        );
        let decoded = decode(Cursor::new(jpeg), DecodeLimits::default()).unwrap();
        assert_eq!((decoded.width, decoded.height), (32, 64));
    }

    #[test]
    fn images_over_the_pixel_limit_are_refused_before_decoding() {
        let jpeg = jpeg_sized_with_exif(64, 64, &ExifSpec::default());
        let limits = DecodeLimits {
            max_pixels: 64 * 63,
            ..DecodeLimits::default()
        };
        assert!(matches!(
            decode(Cursor::new(jpeg), limits),
            Err(ThumbError::TooLarge(64, 64))
        ));
    }

    #[test]
    fn images_whose_buffers_exceed_the_memory_budget_are_refused() {
        let jpeg = jpeg_sized_with_exif(64, 64, &ExifSpec::default());
        let limits = DecodeLimits {
            max_alloc: 64 * 64 * 3 * DECODE_MEMORY_FACTOR - 1,
            ..DecodeLimits::default()
        };
        assert!(matches!(
            decode(Cursor::new(jpeg), limits),
            Err(ThumbError::TooLarge(64, 64))
        ));
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            decode(
                Cursor::new(b"definitely not an image".to_vec()),
                DecodeLimits::default()
            ),
            Err(ThumbError::Decode(_))
        ));
    }

    /// `tests/fixtures/rotated.heic`: `heif-enc -q 50` of a 64 × 32 JPEG with
    /// EXIF orientation 6 (`jpeg_sized_with_exif`). Skipped when the fixture
    /// has not been generated on this machine.
    #[cfg(feature = "heic")]
    #[test]
    fn heif_rotation_is_applied_once() {
        let Ok(bytes) = std::fs::read("tests/fixtures/rotated.heic") else {
            eprintln!("skipping: tests/fixtures/rotated.heic is missing");
            return;
        };
        crate::heic::register();
        let decoded = decode(Cursor::new(bytes), DecodeLimits::default()).unwrap();
        assert_eq!((decoded.width, decoded.height), (32, 64));
    }

    #[test]
    fn variants_are_webp_and_blurhash_is_short() {
        let jpeg = jpeg_sized_with_exif(400, 300, &ExifSpec::default());
        let decoded = decode(Cursor::new(jpeg), DecodeLimits::default()).unwrap();
        let (w, h) = fit(decoded.width, decoded.height, 1080, 120);
        let small = resize(&decoded.image, w, h).unwrap();
        let webp = encode_webp(&small, 75.0);
        assert_eq!(&webp[..4], b"RIFF");
        assert_eq!(&webp[8..12], b"WEBP");
        let back = image::load_from_memory(&webp).unwrap();
        assert_eq!((back.width(), back.height()), (160, 120));
        let hash = blurhash_of(&decoded.image).unwrap();
        assert!((20..=40).contains(&hash.len()), "{hash}");
    }
}
