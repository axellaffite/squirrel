//! The stored variants: the grid thumbnail of every image and PDF, and the
//! 1440 px preview of a PDF's first page. Photo previews are rendered on
//! request instead (`crate::preview`).

use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::thumbs::{
    DecodeLimits, Decoded, ThumbError, blurhash_of, decode, encode_webp, fit, resize, swaps_axes,
};

/// Bump to regenerate every thumbnail (backfill compares `media.thumbs_version`).
pub const THUMBS_VERSION: i16 = 1;
pub const THUMB_HEIGHT: u32 = 360;
pub const THUMB_MAX_WIDTH: u32 = 1080;
pub const PREVIEW_SIDE: u32 = 1440;
const THUMB_QUALITY: f32 = 72.0;
const PREVIEW_QUALITY: f32 = 80.0;
pub const THUMB_FILE: &str = "thumb.webp";
pub const PREVIEW_FILE: &str = "preview.webp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rendered {
    /// The source as displayed (after orientation).
    pub width: u32,
    pub height: u32,
    pub blurhash: String,
    /// The perceptual hash of a photo ([`crate::phash`]), from its thumbnail;
    /// `None` for a PDF page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phash: Option<u64>,
}

/// Writes `thumb.webp` and `preview.webp` into `out_dir` (a PDF page).
pub fn render_variants(image: &DynamicImage, out_dir: &Path) -> Result<Rendered, ThumbError> {
    let (width, height) = (image.width(), image.height());
    let (pw, ph) = fit(width, height, PREVIEW_SIDE, PREVIEW_SIDE);
    let preview = resize(image, pw, ph)?;
    write_variants(preview, width, height, out_dir, StorePreview::Yes)
}

/// Whether the 1440 px intermediate is stored as `preview.webp` too (a PDF
/// page), or not (a photo, which gets a perceptual hash instead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorePreview {
    Yes,
    No,
}

/// The thumbnail of a photo, decoded but not yet upright: it is resized in
/// its stored orientation to 1440 px, and only that copy is rotated, then
/// reduced. Only `thumb.webp` is written.
pub fn render_decoded(decoded: Decoded, out_dir: &Path) -> Result<Rendered, ThumbError> {
    let (width, height) = (decoded.width, decoded.height);
    let (pw, ph) = fit(width, height, PREVIEW_SIDE, PREVIEW_SIDE);
    let (rw, rh) = if swaps_axes(decoded.orientation) {
        (ph, pw)
    } else {
        (pw, ph)
    };
    let mut preview = resize(&decoded.image, rw, rh)?;
    drop(decoded.image);
    preview.apply_orientation(decoded.orientation);
    write_variants(preview, width, height, out_dir, StorePreview::No)
}

/// Writes a thumbnail made from `preview` (and the preview itself when asked).
/// `width`/`height`: the source as displayed.
fn write_variants(
    preview: DynamicImage,
    width: u32,
    height: u32,
    out_dir: &Path,
    store_preview: StorePreview,
) -> Result<Rendered, ThumbError> {
    let (pw, ph) = (preview.width(), preview.height());
    // From the preview: 4–16× fewer pixels to filter, no visible difference at 360 px.
    let (tw, th) = fit(pw, ph, THUMB_MAX_WIDTH, THUMB_HEIGHT);
    let thumb = resize(&preview, tw, th)?;
    if store_preview == StorePreview::Yes {
        fs::write(
            out_dir.join(PREVIEW_FILE),
            encode_webp(&preview, PREVIEW_QUALITY),
        )?;
    }
    fs::write(out_dir.join(THUMB_FILE), encode_webp(&thumb, THUMB_QUALITY))?;
    // From the thumbnail, as the backfill hashes the stored one.
    let phash = match store_preview {
        StorePreview::No => Some(crate::phash::phash(&thumb)?),
        StorePreview::Yes => None,
    };
    Ok(Rendered {
        width,
        height,
        blurhash: blurhash_of(&thumb)?,
        phash,
    })
}

pub fn render_image(
    input: &Path,
    out_dir: &Path,
    limits: DecodeLimits,
) -> Result<Rendered, ThumbError> {
    let decoded = decode(BufReader::new(File::open(input)?), limits)?;
    render_decoded(decoded, out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};

    #[test]
    fn photos_store_only_their_thumbnail_and_report_displayed_size() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(
            &input,
            jpeg_sized_with_exif(
                2000,
                1000,
                &ExifSpec {
                    orientation: Some(6),
                    ..ExifSpec::default()
                },
            ),
        )
        .unwrap();
        let out = tempfile::tempdir().unwrap();
        let rendered = render_image(&input, out.path(), DecodeLimits::default()).unwrap();
        assert_eq!((rendered.width, rendered.height), (1000, 2000), "rotated");
        let thumb = image::open(out.path().join(THUMB_FILE)).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (180, 360));
        assert!(
            !out.path().join(PREVIEW_FILE).exists(),
            "rendered on request"
        );
        assert!(!rendered.blurhash.is_empty());
        assert!(rendered.phash.is_some(), "photos get a perceptual hash");
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(120, 80, &ExifSpec::default())).unwrap();
        let rendered = render_image(&input, dir.path(), DecodeLimits::default()).unwrap();
        assert_eq!((rendered.width, rendered.height), (120, 80));
        assert_eq!(
            image::open(dir.path().join(THUMB_FILE)).unwrap().height(),
            80
        );
    }

    #[test]
    fn resizing_before_rotating_matches_rotating_first() {
        let jpeg = jpeg_sized_with_exif(
            1800,
            900,
            &ExifSpec {
                orientation: Some(6),
                ..ExifSpec::default()
            },
        );
        let decode_it = || {
            crate::thumbs::decode(std::io::Cursor::new(jpeg.clone()), DecodeLimits::default())
                .unwrap()
        };
        let (late, early) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        render_decoded(decode_it(), late.path()).unwrap();
        render_variants(&decode_it().upright(), early.path()).unwrap();
        let a = image::open(late.path().join(THUMB_FILE)).unwrap().to_rgb8();
        let b = image::open(early.path().join(THUMB_FILE))
            .unwrap()
            .to_rgb8();
        assert_eq!(a.dimensions(), b.dimensions());
        let diff: u64 = a
            .as_raw()
            .iter()
            .zip(b.as_raw())
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum();
        let mean = diff / a.as_raw().len() as u64;
        assert!(mean <= 3, "mean channel difference {mean}");
    }
}
