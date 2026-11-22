//! One unit of untrusted decoding, described as data so it can run in a child
//! process (`squirrel media-exec`) or in-process (tests). Outputs go to files in
//! `out_dir`, which the caller created and owns; names are fixed here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pdf::{self, PdfError, PdfWork};
use crate::preview::{
    PreviewEncoding, PreviewEngine, ProbeReport, RenderedPreview, render_builtin,
};
use crate::render::{Rendered, render_image, render_variants};
use crate::thumbs::{DecodeLimits, ThumbError};

/// Inputs larger than this are not decoded at all.
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
/// Where a `Pdf` request asked for text writes it, in `out_dir`.
pub const TEXT_FILE: &str = "text.txt";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ExecRequest {
    Image {
        input: PathBuf,
        out_dir: PathBuf,
        limits: DecodeLimits,
    },
    /// The first page's thumbnails (`render`) and/or the text (`text`, into
    /// [`TEXT_FILE`]) of a PDF, through the pdfium library in `pdfium_dir`.
    Pdf {
        input: PathBuf,
        out_dir: PathBuf,
        pdfium_dir: PathBuf,
        render: bool,
        text: bool,
    },
    /// A preview fitting `bucket` (long side), written to `out_dir` as
    /// [`crate::preview::PreviewFormat::file_name`]. `engine: Vips` still
    /// falls back to the built-in pipeline for inputs libvips has no allowed
    /// loader for.
    Preview {
        input: PathBuf,
        out_dir: PathBuf,
        bucket: u32,
        encoding: PreviewEncoding,
        engine: PreviewEngine,
        limits: DecodeLimits,
    },
    /// Whether libvips works in the sandbox, and which loaders it has.
    Probe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExecResponse {
    Image {
        rendered: Rendered,
    },
    Pdf {
        rendered: Option<Rendered>,
        page_count: u32,
    },
    Preview {
        preview: RenderedPreview,
    },
    Probe {
        report: ProbeReport,
    },
    /// Nothing to do for this input (format not compiled in, too large, encrypted).
    Unsupported {
        reason: String,
    },
    /// `retryable: false` — the input is broken; `true` — an I/O problem.
    Failed {
        reason: String,
        retryable: bool,
    },
}

pub fn execute(request: &ExecRequest) -> ExecResponse {
    match request {
        ExecRequest::Image {
            input,
            out_dir,
            limits,
        } => {
            if let Some(refused) = check_size(input) {
                return refused;
            }
            match render_image(input, out_dir, *limits) {
                Ok(rendered) => ExecResponse::Image { rendered },
                Err(err) => classify(err),
            }
        }
        ExecRequest::Pdf {
            input,
            out_dir,
            pdfium_dir,
            render,
            text,
        } => {
            if let Some(refused) = check_size(input) {
                return refused;
            }
            let work = PdfWork {
                render: *render,
                text: *text,
            };
            execute_pdf(input, out_dir, pdfium_dir, work)
        }
        ExecRequest::Preview {
            input,
            out_dir,
            bucket,
            encoding,
            engine,
            limits,
        } => {
            if let Some(refused) = check_size(input) {
                return refused;
            }
            let job = PreviewJob {
                input,
                out_dir,
                bucket: *bucket,
                encoding: *encoding,
                limits: *limits,
            };
            execute_preview(&job, *engine)
        }
        ExecRequest::Probe => ExecResponse::Probe { report: probe() },
    }
}

struct PreviewJob<'a> {
    input: &'a Path,
    out_dir: &'a Path,
    bucket: u32,
    encoding: PreviewEncoding,
    limits: DecodeLimits,
}

fn execute_preview(job: &PreviewJob<'_>, engine: PreviewEngine) -> ExecResponse {
    if engine == PreviewEngine::Vips
        && let Some(response) = execute_vips(job)
    {
        return response;
    }
    match render_builtin(job.input, job.out_dir, job.bucket, job.encoding, job.limits) {
        Ok(preview) => ExecResponse::Preview { preview },
        Err(err) => classify(err),
    }
}

/// libvips' answer, or `None` to fall back to the built-in pipeline (no
/// allowed loader for this input, libvips unusable, or not compiled in).
#[cfg(feature = "vips")]
fn execute_vips(job: &PreviewJob<'_>) -> Option<ExecResponse> {
    use crate::vips::{self, VipsError, VipsOutcome};
    match vips::render(job.input, job.out_dir, job.bucket, job.encoding, job.limits) {
        Ok(VipsOutcome::Rendered(preview)) => Some(ExecResponse::Preview { preview }),
        Ok(VipsOutcome::NoLoader) | Err(VipsError::Unavailable(_)) => None,
        Err(err @ VipsError::TooLarge(..)) => Some(ExecResponse::Unsupported {
            reason: err.to_string(),
        }),
        Err(err @ VipsError::Failed(_)) => Some(ExecResponse::Failed {
            reason: err.to_string(),
            retryable: false,
        }),
        Err(err @ VipsError::Io(_)) => Some(ExecResponse::Failed {
            reason: err.to_string(),
            retryable: true,
        }),
    }
}

#[cfg(not(feature = "vips"))]
fn execute_vips(_job: &PreviewJob<'_>) -> Option<ExecResponse> {
    None
}

/// What libvips looks like from inside the sandbox.
pub fn probe() -> ProbeReport {
    #[cfg(feature = "vips")]
    {
        match crate::vips::probe() {
            Ok((version, loaders)) => ProbeReport {
                compiled: true,
                version: Some(version),
                loaders,
                error: None,
            },
            Err(err) => ProbeReport {
                compiled: true,
                error: Some(err.to_string()),
                ..ProbeReport::default()
            },
        }
    }
    #[cfg(not(feature = "vips"))]
    ProbeReport::default()
}

/// `Some(response)` when the input cannot be decoded for its size (or is gone).
fn check_size(input: &Path) -> Option<ExecResponse> {
    match std::fs::metadata(input) {
        Ok(meta) if meta.len() > MAX_INPUT_BYTES => Some(ExecResponse::Unsupported {
            reason: format!("{} bytes is too large to decode", meta.len()),
        }),
        Ok(_) => None,
        Err(err) => Some(ExecResponse::Failed {
            reason: err.to_string(),
            retryable: true,
        }),
    }
}

fn execute_pdf(input: &Path, out_dir: &Path, pdfium_dir: &Path, work: PdfWork) -> ExecResponse {
    let output = match pdf::bind(pdfium_dir).and_then(|pdfium| pdf::process(pdfium, input, work)) {
        Ok(output) => output,
        Err(err @ (PdfError::Unavailable(_) | PdfError::Encrypted)) => {
            return ExecResponse::Unsupported {
                reason: err.to_string(),
            };
        }
        Err(err @ PdfError::Unreadable(_)) => {
            return ExecResponse::Failed {
                reason: err.to_string(),
                retryable: false,
            };
        }
    };
    let rendered = match output
        .first_page
        .as_ref()
        .map(|page| render_variants(page, out_dir))
    {
        None => None,
        Some(Ok(rendered)) => Some(rendered),
        Some(Err(err)) => return classify(err),
    };
    if let Some(text) = &output.text
        && let Err(err) = std::fs::write(out_dir.join(TEXT_FILE), text)
    {
        return ExecResponse::Failed {
            reason: err.to_string(),
            retryable: true,
        };
    }
    ExecResponse::Pdf {
        rendered,
        page_count: output.page_count,
    }
}

/// Unsupported: a format or feature this build cannot decode, or past the
/// limits. Failed for good: bytes that are no image at all, or corrupt.
fn classify(err: ThumbError) -> ExecResponse {
    use image::ImageError;
    use image::error::{ImageFormatHint, UnsupportedErrorKind};
    let reason = err.to_string();
    match err {
        ThumbError::TooLarge(..) => ExecResponse::Unsupported { reason },
        ThumbError::Decode(ImageError::Unsupported(ref unsupported))
            if matches!(
                unsupported.kind(),
                UnsupportedErrorKind::Format(ImageFormatHint::Unknown)
            ) =>
        {
            ExecResponse::Failed {
                reason,
                retryable: false,
            }
        }
        ThumbError::Decode(ImageError::Unsupported(_) | ImageError::Limits(_)) => {
            ExecResponse::Unsupported { reason }
        }
        ThumbError::Decode(ImageError::IoError(_)) | ThumbError::Io(_) => ExecResponse::Failed {
            reason,
            retryable: true,
        },
        _ => ExecResponse::Failed {
            reason,
            retryable: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{PREVIEW_FILE, THUMB_FILE};
    use crate::testutil::{
        ExifSpec, encrypted_pdf, jpeg_sized_with_exif, pdf_with_pages, pdf_with_text,
    };

    fn image_request(input: PathBuf, out_dir: PathBuf, limits: DecodeLimits) -> ExecRequest {
        ExecRequest::Image {
            input,
            out_dir,
            limits,
        }
    }

    #[test]
    fn renders_an_image_into_the_output_directory() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(400, 300, &ExifSpec::default())).unwrap();
        let out = tempfile::tempdir().unwrap();
        let request = image_request(input, out.path().into(), DecodeLimits::default());
        match execute(&request) {
            ExecResponse::Image { rendered } => {
                assert_eq!((rendered.width, rendered.height), (400, 300));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(out.path().join(THUMB_FILE).is_file());
        assert!(!out.path().join(PREVIEW_FILE).exists());
    }

    #[test]
    fn garbage_fails_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, b"not an image").unwrap();
        let request = image_request(input, dir.path().into(), DecodeLimits::default());
        assert!(matches!(
            execute(&request),
            ExecResponse::Failed {
                retryable: false,
                ..
            }
        ));
    }

    #[test]
    fn images_over_the_limits_are_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(32, 24, &ExifSpec::default())).unwrap();
        let limits = DecodeLimits {
            max_pixels: 10,
            ..DecodeLimits::default()
        };
        match execute(&image_request(input, dir.path().into(), limits)) {
            ExecResponse::Unsupported { reason } => assert!(reason.contains("exceeds"), "{reason}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_missing_input_is_worth_retrying() {
        let dir = tempfile::tempdir().unwrap();
        let request = image_request(
            dir.path().join("gone.jpg"),
            dir.path().into(),
            DecodeLimits::default(),
        );
        assert!(matches!(
            execute(&request),
            ExecResponse::Failed {
                retryable: true,
                ..
            }
        ));
    }

    fn pdf_request(dir: &Path, bytes: &[u8], pdfium_dir: PathBuf) -> ExecRequest {
        let input = dir.join("in.pdf");
        std::fs::write(&input, bytes).unwrap();
        ExecRequest::Pdf {
            input,
            out_dir: dir.into(),
            pdfium_dir,
            render: true,
            text: true,
        }
    }

    #[test]
    fn pdfs_without_pdfium_are_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let request = pdf_request(dir.path(), &pdf_with_text("Hi"), dir.path().join("none"));
        match execute(&request) {
            ExecResponse::Unsupported { reason } => assert!(reason.contains("pdfium"), "{reason}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    fn pdfium_dir() -> Option<PathBuf> {
        crate::testutil::test_pdfium_dir()
    }

    #[test]
    fn pdfs_render_their_first_page_and_write_their_text() {
        let Some(pdfium) = pdfium_dir() else { return };
        let dir = tempfile::tempdir().unwrap();
        let pdf = pdf_with_pages(&["First page", "Second page"], "");
        match execute(&pdf_request(dir.path(), &pdf, pdfium)) {
            ExecResponse::Pdf {
                rendered: Some(rendered),
                page_count: 2,
            } => assert_eq!((rendered.width, rendered.height), (1018, 1440)),
            other => panic!("unexpected {other:?}"),
        }
        assert!(dir.path().join(THUMB_FILE).is_file());
        let text = std::fs::read_to_string(dir.path().join(TEXT_FILE)).unwrap();
        assert!(
            text.contains("First page") && text.contains("Second page"),
            "{text}"
        );
    }

    #[test]
    fn encrypted_pdfs_are_unsupported_and_garbage_fails() {
        let Some(pdfium) = pdfium_dir() else { return };
        let dir = tempfile::tempdir().unwrap();
        match execute(&pdf_request(dir.path(), &encrypted_pdf(), pdfium.clone())) {
            ExecResponse::Unsupported { reason } => assert_eq!(reason, "encrypted PDF"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            execute(&pdf_request(dir.path(), b"%PDF-1.4 nothing", pdfium)),
            ExecResponse::Failed {
                retryable: false,
                ..
            }
        ));
    }

    fn preview_request(
        input: PathBuf,
        out_dir: PathBuf,
        engine: PreviewEngine,
        limits: DecodeLimits,
    ) -> ExecRequest {
        ExecRequest::Preview {
            input,
            out_dir,
            bucket: 1440,
            encoding: PreviewEncoding::Display,
            engine,
            limits,
        }
    }

    const ENGINES: [PreviewEngine; 2] = [PreviewEngine::Builtin, PreviewEngine::Vips];

    /// Renders `bytes` (saved as `name`) with `engine`; the decoded output.
    fn preview_of(
        name: &str,
        bytes: &[u8],
        engine: PreviewEngine,
    ) -> (RenderedPreview, image::RgbImage) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join(name);
        std::fs::write(&input, bytes).unwrap();
        let request = preview_request(input, dir.path().into(), engine, DecodeLimits::default());
        let ExecResponse::Preview { preview } = execute(&request) else {
            panic!("no preview for {name}");
        };
        let written = image::open(dir.path().join(preview.format.file_name()))
            .unwrap()
            .to_rgb8();
        assert_eq!(written.dimensions(), (preview.width, preview.height));
        (preview, written)
    }

    fn mean_difference(a: &image::RgbImage, b: &image::RgbImage) -> u64 {
        assert_eq!(a.dimensions(), b.dimensions());
        let diff: u64 = a
            .as_raw()
            .iter()
            .zip(b.as_raw())
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum();
        diff / a.as_raw().len() as u64
    }

    #[test]
    fn every_orientation_matches_the_builtin_pipeline() {
        for orientation in 1..=8 {
            let spec = ExifSpec {
                orientation: Some(orientation),
                ..ExifSpec::default()
            };
            let jpeg = jpeg_sized_with_exif(300, 200, &spec);
            let (builtin, reference) = preview_of("in.jpg", &jpeg, PreviewEngine::Builtin);
            let expected = if orientation >= 5 {
                (200, 300)
            } else {
                (300, 200)
            };
            assert_eq!((builtin.width, builtin.height), expected, "{orientation}");
            let (vips, rendered) = preview_of("in.jpg", &jpeg, PreviewEngine::Vips);
            let engine = if crate::VIPS_COMPILED {
                PreviewEngine::Vips
            } else {
                PreviewEngine::Builtin
            };
            assert_eq!(vips.engine, engine);
            assert_eq!(vips.format, crate::preview::PreviewFormat::Jpeg);
            let mean = mean_difference(&reference, &rendered);
            assert!(
                mean <= 6,
                "orientation {orientation}: mean difference {mean}"
            );
        }
    }

    #[test]
    fn previews_over_the_pixel_limit_are_refused_by_both_engines() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(64, 64, &ExifSpec::default())).unwrap();
        let limits = DecodeLimits {
            max_pixels: 64 * 63,
            ..DecodeLimits::default()
        };
        for engine in ENGINES {
            let request = preview_request(input.clone(), dir.path().into(), engine, limits);
            match execute(&request) {
                ExecResponse::Unsupported { reason } => {
                    assert!(reason.contains("exceeds"), "{reason}");
                }
                other => panic!("{engine:?}: unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn garbage_previews_fail_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, b"\xFF\xD8\xFF not really a jpeg").unwrap();
        for engine in ENGINES {
            let request = preview_request(
                input.clone(),
                dir.path().into(),
                engine,
                DecodeLimits::default(),
            );
            assert!(
                matches!(
                    execute(&request),
                    ExecResponse::Failed {
                        retryable: false,
                        ..
                    }
                ),
                "{engine:?}"
            );
        }
    }

    /// BMP has no allowed libvips loader: the built-in pipeline renders it.
    #[test]
    fn inputs_without_an_allowed_loader_fall_back_to_the_builtin_pipeline() {
        let image = image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([x as u8, y as u8, 9]));
        let mut bmp = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageFormat::Bmp)
            .unwrap();
        let (preview, _) = preview_of("in.bmp", &bmp, PreviewEngine::Vips);
        assert_eq!(preview.engine, PreviewEngine::Builtin);
        assert_eq!((preview.width, preview.height), (40, 30));
    }

    #[test]
    fn transparent_previews_are_webp_and_models_get_png() {
        let image = image::RgbaImage::from_fn(40, 30, |x, _| image::Rgba([200, 10, 10, x as u8]));
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        for engine in ENGINES {
            let (preview, _) = preview_of("in.png", &png, engine);
            assert_eq!(
                preview.format,
                crate::preview::PreviewFormat::Webp,
                "{engine:?}"
            );
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("in.png");
            std::fs::write(&input, &png).unwrap();
            let request = ExecRequest::Preview {
                input,
                out_dir: dir.path().into(),
                bucket: 1440,
                encoding: PreviewEncoding::Png,
                engine,
                limits: DecodeLimits::default(),
            };
            let ExecResponse::Preview { preview } = execute(&request) else {
                panic!("{engine:?}: no png");
            };
            assert_eq!(preview.format, crate::preview::PreviewFormat::Png);
            assert!(dir.path().join("preview.png").is_file());
        }
    }

    #[test]
    fn the_probe_reports_what_this_build_has() {
        let ExecResponse::Probe { report } = execute(&ExecRequest::Probe) else {
            panic!("no probe report");
        };
        assert_eq!(report.compiled, crate::VIPS_COMPILED);
        if crate::VIPS_COMPILED {
            assert!(report.usable(), "{report:?}");
        } else {
            assert_eq!(report, ProbeReport::default());
        }
    }

    /// libvips applies HEIF's own rotation once (see `thumbs::heif_rotation_is_applied_once`).
    #[cfg(feature = "vips")]
    #[test]
    fn libvips_heif_previews_are_upright() {
        let Some(bytes) = crate::testutil::ci_fixture("tests/fixtures/rotated.heic") else {
            return;
        };
        let (preview, _) = preview_of("in.heic", &bytes, PreviewEngine::Vips);
        assert_eq!(preview.engine, PreviewEngine::Vips);
        assert_eq!((preview.width, preview.height), (32, 64));
    }

    #[test]
    fn requests_and_responses_round_trip_as_json() {
        let request = image_request("/a".into(), "/b".into(), DecodeLimits::default());
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"op\":\"image\""), "{json}");
        let response = ExecResponse::Failed {
            reason: "x".into(),
            retryable: true,
        };
        let back: ExecResponse =
            serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
        assert_eq!(back, response);
    }
}
