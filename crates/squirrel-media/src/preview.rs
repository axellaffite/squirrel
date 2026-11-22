//! Lightbox previews rendered on request from the original: size buckets,
//! output formats, and the built-in pipeline (used without libvips, or when
//! libvips has no loader for the input).

use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::Path;

use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use serde::{Deserialize, Serialize};

use crate::thumbs::{DecodeLimits, ThumbError, decode, encode_webp, fit, resize, swaps_axes};

/// Bump when the renderer's output changes: cached previews of another
/// version are purged at start, and clients revalidate within a week (ETag).
pub const PREVIEW_VERSION: i16 = 1;
/// Long sides a preview may have. A client's `px` is rounded up to one of
/// them and capped at the last.
pub const BUCKETS: [u32; 3] = [1440, 2160, 3200];
/// No `px` (old clients, face crops, the models): the historical 1440 px.
pub const DEFAULT_BUCKET: u32 = BUCKETS[0];
const JPEG_QUALITY: u8 = 85;
const WEBP_QUALITY: f32 = 80.0;

/// The bucket for a client's requested long side, in device pixels.
pub fn bucket_for(px: Option<u32>) -> u32 {
    let Some(px) = px else {
        return DEFAULT_BUCKET;
    };
    BUCKETS
        .into_iter()
        .find(|bucket| *bucket >= px)
        .unwrap_or(BUCKETS[BUCKETS.len() - 1])
}

/// Never upscale: a source (`width` × `height` as displayed) smaller than
/// `bucket` shares the smallest bucket that covers it, so at most one entry
/// per real size is rendered and cached.
pub fn bucket_for_source(bucket: u32, width: u32, height: u32) -> u32 {
    let long = width.max(height);
    let covering = BUCKETS
        .into_iter()
        .find(|candidate| *candidate >= long)
        .unwrap_or(u32::MAX);
    bucket.min(covering)
}

/// What the caller wants: the display formats (JPEG when opaque, WebP with
/// alpha), or lossless PNG (the models' input).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewEncoding {
    Display,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewFormat {
    Jpeg,
    Webp,
    Png,
}

impl PreviewFormat {
    pub const ALL: [PreviewFormat; 3] = [Self::Jpeg, Self::Webp, Self::Png];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Png => "png",
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.extension() == extension)
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Png => "image/png",
        }
    }

    /// The renderer's output file in its `out_dir`.
    pub fn file_name(self) -> String {
        format!("preview.{}", self.extension())
    }
}

/// Which pipeline renders: libvips (when built with `vips` and the probe
/// found it working) or the built-in decoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewEngine {
    Builtin,
    Vips,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedPreview {
    pub width: u32,
    pub height: u32,
    pub format: PreviewFormat,
    /// Which pipeline actually rendered (libvips falls back per input).
    pub engine: PreviewEngine,
}

/// What `media-exec --probe` found about libvips in this build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// Built with the `vips` feature.
    pub compiled: bool,
    /// The libvips version, when it initialised.
    pub version: Option<String>,
    /// The allowed loaders it has (`jpegload`, `heifload`, …).
    pub loaders: Vec<String>,
    /// Why libvips cannot be used, when it cannot.
    pub error: Option<String>,
}

/// The loaders previews need from libvips for every common photo.
const CORE_LOADERS: [&str; 3] = ["jpegload", "pngload", "webpload"];
const HEIF_LOADER: &str = "heifload";

impl ProbeReport {
    /// libvips is usable: it initialised and has the core loaders.
    pub fn usable(&self) -> bool {
        self.version.is_some()
            && self.error.is_none()
            && CORE_LOADERS
                .iter()
                .all(|loader| self.loaders.iter().any(|l| l == loader))
    }

    /// The engine for a blob (`heif`: of the HEIF family): libvips when
    /// usable and, for HEIF, it has the HEIF loader; else the built-in pipeline.
    pub fn engine_for(&self, heif: bool) -> PreviewEngine {
        let heif_ok = !heif || self.loaders.iter().any(|l| l == HEIF_LOADER);
        if self.usable() && heif_ok {
            PreviewEngine::Vips
        } else {
            PreviewEngine::Builtin
        }
    }
}

/// The built-in pipeline: decode (bounded), resize to fit `bucket`, orient,
/// encode into `out_dir`. Metadata is never written.
pub fn render_builtin(
    input: &Path,
    out_dir: &Path,
    bucket: u32,
    encoding: PreviewEncoding,
    limits: DecodeLimits,
) -> Result<RenderedPreview, ThumbError> {
    let decoded = decode(BufReader::new(File::open(input)?), limits)?;
    let (pw, ph) = fit(decoded.width, decoded.height, bucket, bucket);
    let (rw, rh) = if swaps_axes(decoded.orientation) {
        (ph, pw)
    } else {
        (pw, ph)
    };
    let mut preview = resize(&decoded.image, rw, rh)?;
    drop(decoded.image);
    preview.apply_orientation(decoded.orientation);
    let format = format_for(encoding, preview.color().has_alpha());
    fs::write(out_dir.join(format.file_name()), encode(&preview, format)?)?;
    Ok(RenderedPreview {
        width: preview.width(),
        height: preview.height(),
        format,
        engine: PreviewEngine::Builtin,
    })
}

/// JPEG for opaque display previews (≈ 10× faster than WebP at 3200 px),
/// WebP when there is alpha, PNG when asked.
pub fn format_for(encoding: PreviewEncoding, has_alpha: bool) -> PreviewFormat {
    match encoding {
        PreviewEncoding::Png => PreviewFormat::Png,
        PreviewEncoding::Display if has_alpha => PreviewFormat::Webp,
        PreviewEncoding::Display => PreviewFormat::Jpeg,
    }
}

fn encode(image: &DynamicImage, format: PreviewFormat) -> Result<Vec<u8>, ThumbError> {
    let mut out = Vec::new();
    match format {
        PreviewFormat::Jpeg => {
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&image.to_rgb8())?;
        }
        PreviewFormat::Webp => out = encode_webp(image, WEBP_QUALITY),
        PreviewFormat::Png => {
            image.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)?
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};

    #[test]
    fn px_is_rounded_up_to_a_bucket_and_capped() {
        assert_eq!(bucket_for(None), 1440);
        assert_eq!(bucket_for(Some(1)), 1440);
        assert_eq!(bucket_for(Some(1440)), 1440);
        assert_eq!(bucket_for(Some(1441)), 2160);
        assert_eq!(bucket_for(Some(2160)), 2160);
        assert_eq!(bucket_for(Some(2500)), 3200);
        assert_eq!(bucket_for(Some(100_000)), 3200);
    }

    #[test]
    fn small_sources_share_the_smallest_covering_bucket() {
        assert_eq!(bucket_for_source(3200, 6000, 4000), 3200);
        assert_eq!(bucket_for_source(3200, 2000, 1500), 2160);
        assert_eq!(bucket_for_source(3200, 1000, 800), 1440);
        assert_eq!(bucket_for_source(2160, 1440, 900), 1440);
        assert_eq!(bucket_for_source(1440, 8000, 6000), 1440);
        assert_eq!(bucket_for_source(2160, 0, 0), 1440);
    }

    #[test]
    fn formats_follow_alpha_and_the_request() {
        assert_eq!(
            format_for(PreviewEncoding::Display, false),
            PreviewFormat::Jpeg
        );
        assert_eq!(
            format_for(PreviewEncoding::Display, true),
            PreviewFormat::Webp
        );
        assert_eq!(format_for(PreviewEncoding::Png, false), PreviewFormat::Png);
        for format in PreviewFormat::ALL {
            assert_eq!(
                PreviewFormat::from_extension(format.extension()),
                Some(format)
            );
        }
    }

    #[test]
    fn the_probe_picks_libvips_only_when_it_has_the_loaders() {
        let mut probe = ProbeReport {
            compiled: true,
            version: Some("8.18.0".into()),
            loaders: ["jpegload", "pngload", "webpload"]
                .map(String::from)
                .to_vec(),
            error: None,
        };
        assert_eq!(probe.engine_for(false), PreviewEngine::Vips);
        assert_eq!(probe.engine_for(true), PreviewEngine::Builtin);
        probe.loaders.push("heifload".into());
        assert_eq!(probe.engine_for(true), PreviewEngine::Vips);
        probe.loaders.retain(|l| l != "pngload");
        assert_eq!(probe.engine_for(false), PreviewEngine::Builtin);
        assert_eq!(
            ProbeReport::default().engine_for(false),
            PreviewEngine::Builtin
        );
    }

    fn render(width: u32, height: u32, orientation: Option<u16>, bucket: u32) -> RenderedPreview {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        let spec = ExifSpec {
            orientation,
            ..ExifSpec::default()
        };
        std::fs::write(&input, jpeg_sized_with_exif(width, height, &spec)).unwrap();
        let rendered = render_builtin(
            &input,
            dir.path(),
            bucket,
            PreviewEncoding::Display,
            DecodeLimits::default(),
        )
        .unwrap();
        let written = image::open(dir.path().join(rendered.format.file_name())).unwrap();
        assert_eq!(
            (written.width(), written.height()),
            (rendered.width, rendered.height)
        );
        rendered
    }

    #[test]
    fn the_builtin_pipeline_fits_the_bucket_upright_without_upscaling() {
        let rotated = render(3000, 1500, Some(6), 2160);
        assert_eq!((rotated.width, rotated.height), (1080, 2160));
        assert_eq!(rotated.format, PreviewFormat::Jpeg);
        assert_eq!(rotated.engine, PreviewEngine::Builtin);
        let small = render(400, 300, None, 3200);
        assert_eq!((small.width, small.height), (400, 300));
    }
}
