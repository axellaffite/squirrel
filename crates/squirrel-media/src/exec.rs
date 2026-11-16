//! One unit of untrusted decoding, described as data so it can run in a child
//! process (`squirrel media-exec`) or in-process (tests). Outputs go to files in
//! `out_dir`, which the caller created and owns; names are fixed here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::render::{Rendered, render_image};
use crate::thumbs::{DecodeLimits, ThumbError};

/// Inputs larger than this are not decoded at all.
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ExecRequest {
    Image {
        input: PathBuf,
        out_dir: PathBuf,
        limits: DecodeLimits,
    },
    // `Pdf` is added in Task 28.
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExecResponse {
    Image {
        rendered: Rendered,
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
            match std::fs::metadata(input) {
                Ok(meta) if meta.len() > MAX_INPUT_BYTES => {
                    return ExecResponse::Unsupported {
                        reason: format!("{} bytes is too large to decode", meta.len()),
                    };
                }
                Ok(_) => {}
                Err(err) => {
                    return ExecResponse::Failed {
                        reason: err.to_string(),
                        retryable: true,
                    };
                }
            }
            match render_image(input, out_dir, *limits) {
                Ok(rendered) => ExecResponse::Image { rendered },
                Err(err) => classify(err),
            }
        }
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
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};

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
