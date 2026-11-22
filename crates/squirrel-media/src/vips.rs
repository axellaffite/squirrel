//! Previews through the system libvips (`vips` feature). Called only where
//! untrusted bytes are decoded: the `media-exec` child, or the in-process
//! executor in tests.
//!
//! Hardening, applied once per process before any load: no operation cache,
//! two worker threads, operations flagged untrusted blocked, and every loader
//! blocked except JPEG, PNG, WebP, TIFF, GIF and HEIF (so ImageMagick, PDF,
//! SVG, RAW… never parse anything).

use std::ffi::CString;
use std::path::Path;
use std::sync::OnceLock;

use libvips::bindings::{
    vips_block_untrusted_set, vips_foreign_find_load, vips_operation_block_set, vips_type_find,
};
use libvips::ops::{self, FailOn, ForeignKeep, Interpretation, Size, ThumbnailOptions};
use libvips::{VipsApp, VipsImage};

use crate::preview::{PreviewEncoding, PreviewEngine, PreviewFormat, RenderedPreview, format_for};
use crate::thumbs::DecodeLimits;

/// Loader classes allowed (unblocked), with the nickname the probe looks for.
const LOADERS: [(&str, &str); 6] = [
    ("VipsForeignLoadJpeg", "jpegload"),
    ("VipsForeignLoadPng", "pngload"),
    ("VipsForeignLoadWebp", "webpload"),
    ("VipsForeignLoadTiff", "tiffload"),
    ("VipsForeignLoadNsgif", "gifload"),
    ("VipsForeignLoadHeif", "heifload"),
];
const CONCURRENCY: i32 = 2;
const JPEG_QUALITY: i32 = 85;
const WEBP_QUALITY: i32 = 80;
/// Fast, still lossless: the models' input is read once and deleted.
const PNG_COMPRESSION: i32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum VipsError {
    #[error("libvips is unavailable: {0}")]
    Unavailable(String),
    #[error("image of {0}×{1} pixels exceeds the limit")]
    TooLarge(u32, u32),
    #[error("libvips could not render: {0}")]
    Failed(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// libvips' outcome: a preview, or no allowed loader for this input (the
/// caller falls back to the built-in pipeline).
#[derive(Debug)]
pub enum VipsOutcome {
    Rendered(RenderedPreview),
    NoLoader,
}

static APP: OnceLock<Result<VipsApp, String>> = OnceLock::new();

fn app() -> Result<&'static VipsApp, VipsError> {
    APP.get_or_init(start)
        .as_ref()
        .map_err(|err| VipsError::Unavailable(err.clone()))
}

fn start() -> Result<VipsApp, String> {
    let app = VipsApp::new("squirrel", false).map_err(|err| err.to_string())?;
    app.cache_set_max(0);
    app.cache_set_max_mem(0);
    app.cache_set_max_files(0);
    app.concurrency_set(CONCURRENCY);
    let all_loaders = CString::new("VipsForeignLoad").map_err(|err| err.to_string())?;
    // SAFETY: plain setters on an initialised libvips, with NUL-terminated
    // class names that outlive the calls.
    unsafe {
        vips_block_untrusted_set(1);
        vips_operation_block_set(all_loaders.as_ptr(), 1);
    }
    for (class, _) in LOADERS {
        let class = CString::new(class).map_err(|err| err.to_string())?;
        // SAFETY: as above.
        unsafe { vips_operation_block_set(class.as_ptr(), 0) };
    }
    Ok(app)
}

/// The libvips version and the allowed loaders it has.
pub fn probe() -> Result<(String, Vec<String>), VipsError> {
    let app = app()?;
    let version = app
        .version_string()
        .map_err(|err| VipsError::Unavailable(err.to_string()))?
        .to_owned();
    let base = CString::new("VipsOperation").map_err(|err| VipsError::Failed(err.to_string()))?;
    let mut loaders = Vec::new();
    for (_, nickname) in LOADERS {
        let name = CString::new(nickname).map_err(|err| VipsError::Failed(err.to_string()))?;
        // SAFETY: reads libvips' type table; both strings are NUL-terminated.
        if unsafe { vips_type_find(base.as_ptr(), name.as_ptr()) } != 0 {
            loaders.push(nickname.to_owned());
        }
    }
    Ok((version, loaders))
}

/// Renders `input` to fit `bucket` (shrink-on-load, orientation applied,
/// sRGB, metadata stripped) into `out_dir`.
pub fn render(
    input: &Path,
    out_dir: &Path,
    bucket: u32,
    encoding: PreviewEncoding,
    limits: DecodeLimits,
) -> Result<VipsOutcome, VipsError> {
    let app = app()?;
    let Some(path) = input.to_str() else {
        return Ok(VipsOutcome::NoLoader);
    };
    if !has_loader(app, path)? {
        return Ok(VipsOutcome::NoLoader);
    }
    match render_with(path, out_dir, bucket, encoding, limits) {
        Ok(rendered) => Ok(VipsOutcome::Rendered(rendered)),
        Err(VipsError::Failed(what)) => Err(VipsError::Failed(details(app, &what))),
        Err(other) => {
            app.error_clear();
            Err(other)
        }
    }
}

fn render_with(
    path: &str,
    out_dir: &Path,
    bucket: u32,
    encoding: PreviewEncoding,
    limits: DecodeLimits,
) -> Result<RenderedPreview, VipsError> {
    // The header alone (libvips loads lazily): refuse huge images before decoding.
    let header = VipsImage::new_from_file(path).map_err(failed)?;
    let (width, height) = (
        dimension(header.get_width()),
        dimension(header.get_height()),
    );
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(VipsError::TooLarge(width, height));
    }
    drop(header);
    let side = i32::try_from(bucket).map_err(|err| VipsError::Failed(err.to_string()))?;
    let options = ThumbnailOptions {
        height: side,
        size: Size::Down,
        fail_on: FailOn::Error,
        ..ThumbnailOptions::default()
    };
    let mut image = ops::thumbnail_with_opts(path, side, &options).map_err(failed)?;
    if !matches!(image.get_interpretation(), Ok(Interpretation::Srgb)) {
        image = ops::colourspace(&image, Interpretation::Srgb).map_err(failed)?;
    }
    let format = format_for(encoding, image.image_hasalpha());
    let bytes = encode(&image, format)?;
    std::fs::write(out_dir.join(format.file_name()), bytes)?;
    Ok(RenderedPreview {
        width: dimension(image.get_width()),
        height: dimension(image.get_height()),
        format,
        engine: PreviewEngine::Vips,
    })
}

fn encode(image: &VipsImage, format: PreviewFormat) -> Result<Vec<u8>, VipsError> {
    let saved = match format {
        PreviewFormat::Jpeg => ops::jpegsave_buffer_with_opts(
            image,
            &ops::JpegsaveBufferOptions {
                q: JPEG_QUALITY,
                keep: ForeignKeep::None,
                ..ops::JpegsaveBufferOptions::default()
            },
        ),
        PreviewFormat::Webp => ops::webpsave_buffer_with_opts(
            image,
            &ops::WebpsaveBufferOptions {
                q: WEBP_QUALITY,
                keep: ForeignKeep::None,
                ..ops::WebpsaveBufferOptions::default()
            },
        ),
        PreviewFormat::Png => ops::pngsave_buffer_with_opts(
            image,
            &ops::PngsaveBufferOptions {
                compression: PNG_COMPRESSION,
                keep: ForeignKeep::None,
                ..ops::PngsaveBufferOptions::default()
            },
        ),
    };
    saved.map_err(failed)
}

/// Whether an allowed loader recognises the file (blocked loaders do not count).
fn has_loader(app: &VipsApp, path: &str) -> Result<bool, VipsError> {
    let path = CString::new(path).map_err(|err| VipsError::Failed(err.to_string()))?;
    // SAFETY: returns a static loader name or NULL; `path` is NUL-terminated.
    let found = !unsafe { vips_foreign_find_load(path.as_ptr()) }.is_null();
    if !found {
        app.error_clear();
    }
    Ok(found)
}

fn failed(err: libvips::error::Error) -> VipsError {
    VipsError::Failed(err.to_string())
}

/// The operation's error plus libvips' error buffer (what actually went wrong).
fn details(app: &VipsApp, what: &str) -> String {
    let buffer = app.error_buffer().unwrap_or_default().trim().to_owned();
    app.error_clear();
    if buffer.is_empty() {
        what.to_owned()
    } else {
        format!("{what}: {buffer}")
    }
}

fn dimension(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}
