//! Writes the E2E media fixtures (`web/e2e/fixtures/`), all generated here: no third-party files.
//!
//! cargo run -p squirrel-media --example make_fixtures --features test-util -- web/e2e/fixtures

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{ImageBuffer, ImageFormat, Rgb};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif, pdf_with_text};

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let path = dir.join(name);
    std::fs::write(&path, bytes)?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

/// A tall PNG like a phone screenshot, without EXIF.
fn screenshot_png(width: u32, height: u32) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        let band = u8::try_from((y * 6 / height) * 40).unwrap_or(200);
        Rgb([
            240 - band,
            242 - band / 2,
            u8::try_from(x % 256).unwrap_or(0) / 4 + 180,
        ])
    });
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .expect("png");
    png
}

/// A plain shape on white, for photo search ("red circle"): generated scenes are not reliable
/// for CLIP, plain shapes are.
fn shape_jpeg(shape: &str) -> Vec<u8> {
    let (w, h) = (640u32, 480u32);
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32 - 0.5, y as f32 / h as f32 - 0.5);
        let inside = match shape {
            "red-circle" => (fx * 4.0 / 3.0).powi(2) + fy.powi(2) < 0.09,
            "blue-square" => fx.abs() < 0.22 && fy.abs() < 0.3,
            _ => fy > -0.32 && fy < 0.32 && fx.abs() < (fy + 0.32) * 0.55,
        };
        let colour = match shape {
            "red-circle" => [220, 30, 30],
            "blue-square" => [30, 70, 210],
            _ => [40, 170, 60],
        };
        Rgb(if inside { colour } else { [250, 250, 250] })
    });
    let mut jpeg = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
        .expect("jpeg");
    jpeg
}

/// The committed public-domain portraits (`crates/squirrel-ml/tests/fixtures/faces`) mirrored:
/// with the originals, four photos of each person, enough for clustering to found a person.
fn mirrored_portrait(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../squirrel-ml/tests/fixtures/faces")
        .join(format!("{name}.jpg"));
    let portrait = image::open(path).expect("portrait").to_rgb8();
    let mirrored = image::imageops::flip_horizontal(&portrait);
    let mut jpeg = Vec::new();
    mirrored
        .write_to(&mut Cursor::new(&mut jpeg), ImageFormat::Jpeg)
        .expect("jpeg");
    jpeg
}

fn main() -> std::io::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("web/e2e/fixtures"));
    std::fs::create_dir_all(&dir)?;

    // Paris, 48°51′24″N 2°21′03″E (the geo sample names it Paris, Île-de-France, France).
    let paris = ExifSpec {
        date_time_original: Some("2024:07:14 18:30:05"),
        offset_time_original: Some("+02:00"),
        gps: Some(((48, 51, 2400), b'N', (2, 21, 300), b'E')),
        make: Some("Google"),
        model: Some("Pixel 8"),
        exposure_time: Some((1, 250)),
        f_number: Some((17, 10)),
        iso: Some(64),
        ..ExifSpec::default()
    };
    write(
        &dir,
        "paris-2024.jpg",
        &jpeg_sized_with_exif(1200, 800, &paris),
    )?;

    // Lyon, 45°45′N 4°50′E, no recorded offset.
    let lyon = ExifSpec {
        date_time_original: Some("2023:05:01 09:00:00"),
        gps: Some(((45, 45, 0), b'N', (4, 50, 0), b'E')),
        make: Some("Apple"),
        model: Some("iPhone 15"),
        ..ExifSpec::default()
    };
    write(
        &dir,
        "lyon-2023.jpg",
        &jpeg_sized_with_exif(800, 1200, &lyon),
    )?;

    write(
        &dir,
        "Screenshot_2024-07-15-10-00-00.png",
        &screenshot_png(1080, 2400),
    )?;
    write(
        &dir,
        "notes.txt",
        b"Quarterly squirrel budget: acorns, hazelnuts and winter storage.\n",
    )?;
    write(
        &dir,
        "invoice.pdf",
        &pdf_with_text("Invoice 2026 for acorn storage"),
    )?;
    for shape in ["red-circle", "blue-square", "green-triangle"] {
        write(&dir, &format!("{shape}.jpg"), &shape_jpeg(shape))?;
    }
    std::fs::create_dir_all(dir.join("faces"))?;
    for name in ["koch-a", "koch-b", "glover-a", "glover-b"] {
        write(
            &dir,
            &format!("faces/{name}-mirrored.jpg"),
            &mirrored_portrait(name),
        )?;
    }
    Ok(())
}
