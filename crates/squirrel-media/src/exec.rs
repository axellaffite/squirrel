//! One unit of untrusted decoding, described as data so it can run in a child
//! process (`squirrel media-exec`) or in-process (tests). Outputs go to files in
//! `out_dir`, which the caller created and owns; names are fixed here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pdf::{self, PdfError, PdfWork};
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
    }
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
        assert!(out.path().join(PREVIEW_FILE).is_file());
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
        let dir = std::env::var_os("SQUIRREL_PDFIUM_DIR");
        if dir.is_none() {
            eprintln!("skipped: SQUIRREL_PDFIUM_DIR is not set");
        }
        dir.map(PathBuf::from)
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
