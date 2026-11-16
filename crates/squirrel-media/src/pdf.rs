//! First-page render and text of a PDF, through a pdfium library loaded at
//! runtime (`SQUIRREL_PDFIUM_DIR`). Runs inside the media sandbox.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use image::DynamicImage;
use pdfium_render::prelude::*;

/// The first page is rendered to fit this square (then thumbnailed like an image).
pub const RENDER_SIDE: i32 = 1440;
/// Text kept per document (UTF-8 bytes) and pages read for it.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
pub const MAX_TEXT_PAGES: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("pdfium is not available: {0}")]
    Unavailable(String),
    /// Opening it needs a password.
    #[error("encrypted PDF")]
    Encrypted,
    #[error("unreadable PDF: {0}")]
    Unreadable(String),
}

#[derive(Debug, Clone, Copy)]
pub struct PdfWork {
    pub render: bool,
    pub text: bool,
}

pub struct PdfOutput {
    pub page_count: u32,
    pub first_page: Option<DynamicImage>,
    pub text: Option<String>,
}

/// Where the pdfium shared library of `library_dir` must be.
pub fn library_path(library_dir: &Path) -> PathBuf {
    Pdfium::pdfium_platform_library_name_at_path(library_dir)
}

/// pdfium can be bound once per process; later calls reuse that binding.
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
static BINDING: Mutex<()> = Mutex::new(());
/// Held while a document loads (see [`process`]).
static LOADING: Mutex<()> = Mutex::new(());

/// The process's pdfium, bound from `library_dir` on first use. A directory
/// without the library is refused even once another one was bound.
pub fn bind(library_dir: &Path) -> Result<&'static Pdfium, PdfError> {
    let path = library_path(library_dir);
    if !path.is_file() {
        return Err(PdfError::Unavailable(format!(
            "no pdfium library at {}",
            path.display()
        )));
    }
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }
    let _guard = BINDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }
    let bindings =
        Pdfium::bind_to_library(&path).map_err(|e| PdfError::Unavailable(e.to_string()))?;
    Ok(PDFIUM.get_or_init(|| Pdfium::new(bindings)))
}

fn unreadable(err: PdfiumError) -> PdfError {
    match err {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            PdfError::Encrypted
        }
        other => PdfError::Unreadable(other.to_string()),
    }
}

pub fn process(pdfium: &Pdfium, input: &Path, work: PdfWork) -> Result<PdfOutput, PdfError> {
    // No password: documents that need one fail here and are reported unsupported.
    // pdfium reports why a load failed through a process-wide "last error":
    // loads in parallel threads (in-process execution, tests) would read
    // each other's. The sandbox runs one document per process.
    let document = {
        let _load = LOADING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pdfium.load_pdf_from_file(input, None).map_err(unreadable)?
    };
    let pages = document.pages();
    let page_count = u32::try_from(pages.len()).unwrap_or(0);
    let first_page = if work.render {
        let first = pages.first().map_err(unreadable)?;
        let (width, height) = fit_side(first.width().value, first.height().value);
        let config = PdfRenderConfig::new().set_target_size(width, height);
        let bitmap = first.render_with_config(&config).map_err(unreadable)?;
        Some(bitmap.as_image().map_err(unreadable)?)
    } else {
        None
    };
    let text = work
        .text
        .then(|| page_texts(pages.iter().map(|page| page.text().map(|t| t.all()).ok())));
    Ok(PdfOutput {
        page_count,
        first_page,
        text,
    })
}

/// The pages' texts in order, each followed by a line break, while they fit
/// in [`MAX_TEXT_BYTES`] (at most [`MAX_TEXT_PAGES`]). Only a first page
/// larger than the cap is cut; otherwise reading stops before the page that
/// would not fit. Unreadable pages (`None`) are skipped.
fn page_texts(pages: impl Iterator<Item = Option<String>>) -> String {
    let mut text = String::new();
    for page in pages.take(MAX_TEXT_PAGES).flatten() {
        let page = page.replace('\0', "");
        if text.len() + page.len() + 1 > MAX_TEXT_BYTES {
            if text.is_empty() {
                text.push_str(&page[..page.floor_char_boundary(MAX_TEXT_BYTES)]);
            }
            break;
        }
        text.push_str(&page);
        text.push('\n');
    }
    text
}

/// A page of `width` × `height` points scaled (up or down) so that its
/// longest side is [`RENDER_SIDE`] pixels; degenerate sizes become a square.
fn fit_side(width: f32, height: f32) -> (i32, i32) {
    let valid = |v: f32| v.is_finite() && v > 0.0;
    if !valid(width) || !valid(height) {
        return (RENDER_SIDE, RENDER_SIDE);
    }
    let side = RENDER_SIDE as f32;
    let scale = side / width.max(height);
    let px = |v: f32| ((v * scale).round() as i32).clamp(1, RENDER_SIDE);
    (px(width), px(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::pdf_with_text;

    /// `scripts/fetch-pdfium.sh` prints the value to export.
    fn pdfium() -> Option<&'static Pdfium> {
        let dir = crate::testutil::test_pdfium_dir()?;
        Some(bind(&dir).expect("pdfium loads"))
    }

    #[test]
    fn renders_the_first_page_and_extracts_text() {
        let Some(pdfium) = pdfium() else { return };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        std::fs::write(&input, pdf_with_text("Squirrel invoice 2026")).unwrap();
        let work = PdfWork {
            render: true,
            text: true,
        };
        let out = process(pdfium, &input, work).unwrap();
        assert_eq!(out.page_count, 1);
        let page = out.first_page.unwrap();
        assert_eq!(
            (page.width(), page.height()),
            (1018, 1440),
            "A4 portrait fitted"
        );
        assert!(out.text.unwrap().contains("Squirrel invoice 2026"));
    }

    #[test]
    fn text_alone_renders_nothing() {
        let Some(pdfium) = pdfium() else { return };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        std::fs::write(&input, pdf_with_text("Hello")).unwrap();
        let work = PdfWork {
            render: false,
            text: true,
        };
        let out = process(pdfium, &input, work).unwrap();
        assert!(out.first_page.is_none());
        assert!(out.text.unwrap().contains("Hello"));
    }

    #[test]
    fn garbage_is_unreadable() {
        let Some(pdfium) = pdfium() else { return };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("bad.pdf");
        std::fs::write(&input, b"%PDF-1.4 garbage").unwrap();
        let work = PdfWork {
            render: true,
            text: false,
        };
        assert!(matches!(
            process(pdfium, &input, work),
            Err(PdfError::Unreadable(_))
        ));
    }

    #[test]
    fn page_text_stops_before_the_page_that_would_not_fit() {
        let big = "x".repeat(MAX_TEXT_BYTES / 2);
        let pages = vec![Some("a".to_owned()), None, Some(big.clone()), Some(big)];
        let text = page_texts(pages.into_iter());
        assert_eq!(text.len(), 2 + MAX_TEXT_BYTES / 2 + 1);
        let huge = "é".repeat(MAX_TEXT_BYTES);
        let text = page_texts([Some(huge), Some("b".to_owned())].into_iter());
        assert_eq!(
            text.len(),
            MAX_TEXT_BYTES,
            "a first page is cut, not dropped"
        );
        assert_eq!(page_texts([Some("a\0b".to_owned())].into_iter()), "ab\n");
        let many = (0..500).map(|_| Some("p".to_owned()));
        assert_eq!(page_texts(many).len(), 2 * MAX_TEXT_PAGES);
    }

    #[test]
    fn pages_fit_the_render_square() {
        assert_eq!(fit_side(595.0, 842.0), (1018, 1440));
        assert_eq!(fit_side(842.0, 595.0), (1440, 1018));
        assert_eq!(fit_side(100_000.0, 1.0), (1440, 1));
        assert_eq!(fit_side(0.0, 10.0), (1440, 1440));
        assert_eq!(fit_side(f32::NAN, 10.0), (1440, 1440));
    }

    #[test]
    fn a_directory_without_the_library_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(bind(dir.path()), Err(PdfError::Unavailable(_))));
    }
}
