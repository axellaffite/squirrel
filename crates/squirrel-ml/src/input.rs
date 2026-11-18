//! Reading what the models look at: the lightbox preview the thumbnails step
//! already made (a WebP we encoded, at most 1440 px), never the original.

use std::path::{Path, PathBuf};

use image::{ImageReader, Limits, RgbImage};

/// Previews are at most 1440 × 1440 (≈ 2 MP); refuse anything far larger.
pub const MAX_PREVIEW_PIXELS: u64 = 100_000_000;
const MAX_ALLOC: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// Missing or unreadable file: worth a retry (e.g. thumbnails being redone).
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Not a decodable image: retrying cannot help.
    #[error("cannot decode {path}: {message}")]
    Decode { path: PathBuf, message: String },
    #[error("{path} is {width} × {height}, over the {max} pixel limit")]
    TooLarge {
        path: PathBuf,
        width: u32,
        height: u32,
        max: u64,
    },
}

/// What a media item gives the models to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRef {
    /// A photo: its preview.
    Image { preview: PathBuf },
}

/// The frames to analyse: one for a photo (Phase 4 adds video keyframes).
pub fn frames_for(media: &MediaRef) -> Vec<PathBuf> {
    match media {
        MediaRef::Image { preview } => vec![preview.clone()],
    }
}

/// Decodes `path` to RGB (alpha dropped), refusing more than `max_pixels`
/// before decoding anything but the header.
pub fn load_preview(path: &Path, max_pixels: u64) -> Result<RgbImage, InputError> {
    let decode = |message: String| InputError::Decode {
        path: path.to_owned(),
        message,
    };
    let open = || {
        ImageReader::open(path)
            .map_err(|source| InputError::Io {
                path: path.to_owned(),
                source,
            })?
            .with_guessed_format()
            .map_err(|source| InputError::Io {
                path: path.to_owned(),
                source,
            })
    };
    let (width, height) = open()?
        .into_dimensions()
        .map_err(|err| decode(err.to_string()))?;
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err(InputError::TooLarge {
            path: path.to_owned(),
            width,
            height,
            max: max_pixels,
        });
    }
    let mut reader = open()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    Ok(reader
        .decode()
        .map_err(|err| decode(err.to_string()))?
        .to_rgb8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_photo_has_one_frame_its_preview() {
        let media = MediaRef::Image {
            preview: "/d/preview-v1.webp".into(),
        };
        assert_eq!(frames_for(&media), [PathBuf::from("/d/preview-v1.webp")]);
    }

    #[test]
    fn a_missing_preview_is_an_io_error() {
        let err =
            load_preview(Path::new("/nonexistent/preview.webp"), MAX_PREVIEW_PIXELS).unwrap_err();
        assert!(matches!(err, InputError::Io { .. }), "{err}");
    }

    #[test]
    fn garbage_is_a_decode_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.webp");
        std::fs::write(&path, b"nope").unwrap();
        let err = load_preview(&path, MAX_PREVIEW_PIXELS).unwrap_err();
        assert!(matches!(err, InputError::Decode { .. }), "{err}");
    }

    #[test]
    fn an_oversized_header_is_refused_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        RgbImage::new(200, 100).save(&path).unwrap();
        let err = load_preview(&path, 10_000).unwrap_err();
        assert!(
            matches!(err, InputError::TooLarge { width: 200, .. }),
            "{err}"
        );
        // A 12000 × 9000 header (108 MP) without its pixels.
        let huge = dir.path().join("huge.bmp");
        std::fs::write(&huge, bmp_header(12_000, 9_000)).unwrap();
        let err = load_preview(&huge, MAX_PREVIEW_PIXELS).unwrap_err();
        assert!(matches!(err, InputError::TooLarge { .. }), "{err}");
    }

    /// A 24-bit BMP header announcing `width` × `height`, without pixels.
    fn bmp_header(width: u32, height: u32) -> Vec<u8> {
        let mut out = b"BM".to_vec();
        out.extend(54u32.to_le_bytes()); // file size (wrong on purpose: no pixels)
        out.extend([0; 4]);
        out.extend(54u32.to_le_bytes()); // pixel data offset
        out.extend(40u32.to_le_bytes()); // BITMAPINFOHEADER
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(24u16.to_le_bytes());
        out.extend([0; 24]);
        out
    }

    #[test]
    fn previews_decode_to_rgb() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 0]))
            .save(&path)
            .unwrap();
        let rgb = load_preview(&path, MAX_PREVIEW_PIXELS).unwrap();
        assert_eq!(rgb.dimensions(), (4, 3));
        assert_eq!(rgb.get_pixel(0, 0).0, [10, 20, 30]);
    }
}
