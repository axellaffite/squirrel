//! Test fixtures generated in code: no binary files with unclear licences.

use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::{ImageBuffer, Rgb};

type Dms = (u32, u32, u32);

#[derive(Debug, Default, Clone)]
pub struct ExifSpec {
    pub date_time_original: Option<&'static str>,
    pub offset_time_original: Option<&'static str>,
    /// (latitude d/m/centiseconds, 'N'|'S', longitude d/m/centiseconds, 'E'|'W').
    pub gps: Option<(Dms, u8, Dms, u8)>,
    pub make: Option<&'static str>,
    pub model: Option<&'static str>,
    pub orientation: Option<u16>,
    pub user_comment: Option<&'static str>,
    /// Exposure time as a fraction of a second: `(1, 250)` is 1/250 s.
    pub exposure_time: Option<(u32, u32)>,
    /// Aperture as a fraction: `(17, 10)` is f/1.7.
    pub f_number: Option<(u32, u32)>,
    pub iso: Option<u16>,
}

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn dms((d, m, cs): Dms) -> Value {
    Value::Rational(vec![
        Rational { num: d, denom: 1 },
        Rational { num: m, denom: 1 },
        Rational {
            num: cs,
            denom: 100,
        },
    ])
}

/// The TIFF-structured EXIF payload for `spec`.
/// `None` when `spec` sets no field (the writer refuses an empty IFD).
pub fn exif_payload(spec: &ExifSpec) -> Option<Vec<u8>> {
    let mut fields = Vec::new();
    if let Some(v) = spec.date_time_original {
        fields.push(ascii(Tag::DateTimeOriginal, v));
    }
    if let Some(v) = spec.offset_time_original {
        fields.push(ascii(Tag::OffsetTimeOriginal, v));
    }
    if let Some(v) = spec.make {
        fields.push(ascii(Tag::Make, v));
    }
    if let Some(v) = spec.model {
        fields.push(ascii(Tag::Model, v));
    }
    if let Some(v) = spec.orientation {
        fields.push(Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![v]),
        });
    }
    if let Some((num, denom)) = spec.exposure_time {
        fields.push(Field {
            tag: Tag::ExposureTime,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num, denom }]),
        });
    }
    if let Some((num, denom)) = spec.f_number {
        fields.push(Field {
            tag: Tag::FNumber,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num, denom }]),
        });
    }
    if let Some(v) = spec.iso {
        fields.push(Field {
            tag: Tag::PhotographicSensitivity,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![v]),
        });
    }
    if let Some(v) = spec.user_comment {
        let mut bytes = b"ASCII\0\0\0".to_vec();
        bytes.extend_from_slice(v.as_bytes());
        fields.push(Field {
            tag: Tag::UserComment,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(bytes, 0),
        });
    }
    if let Some((lat, lat_ref, lon, lon_ref)) = spec.gps {
        fields.push(Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: dms(lat),
        });
        fields.push(ascii(Tag::GPSLatitudeRef, &(lat_ref as char).to_string()));
        fields.push(Field {
            tag: Tag::GPSLongitude,
            ifd_num: In::PRIMARY,
            value: dms(lon),
        });
        fields.push(ascii(Tag::GPSLongitudeRef, &(lon_ref as char).to_string()));
    }
    if fields.is_empty() {
        return None;
    }
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out, false).expect("exif payload");
    Some(out.into_inner())
}

/// A small RGB JPEG with an APP1 EXIF segment built from `spec` (no segment when empty).
pub fn jpeg_with_exif(spec: &ExifSpec) -> Vec<u8> {
    jpeg_sized_with_exif(32, 24, spec)
}

pub fn jpeg_sized_with_exif(width: u32, height: u32, spec: &ExifSpec) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x * 7) as u8, (y * 5) as u8, 128])
    });
    let mut jpeg = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .expect("jpeg");
    let Some(payload) = exif_payload(spec) else {
        return jpeg;
    };
    let mut segment = Vec::with_capacity(payload.len() + 10);
    segment.extend_from_slice(&[0xFF, 0xE1]);
    let len = u16::try_from(payload.len() + 8).expect("small exif");
    segment.extend_from_slice(&len.to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&payload);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&segment);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// The pdfium for tests that need one (`scripts/fetch-pdfium.sh`):
/// `SQUIRREL_PDFIUM_DIR`. Unset, the test is skipped (`None`), except on CI
/// (`CI` set), where skipping would hide a broken setup: it panics.
pub fn test_pdfium_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("SQUIRREL_PDFIUM_DIR") {
        return Some(dir.into());
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI is set but SQUIRREL_PDFIUM_DIR is not: run scripts/fetch-pdfium.sh"
    );
    eprintln!("skipped: SQUIRREL_PDFIUM_DIR is not set");
    None
}

/// A fixture that only some machines can produce (e.g. `rotated.heic`, encoded with libheif's
/// `heif-enc` in CI). Missing, the test is skipped (`None`), except on CI (`CI` set), where
/// skipping would hide a broken setup: it panics.
pub fn ci_fixture(path: impl AsRef<std::path::Path>) -> Option<Vec<u8>> {
    let path = path.as_ref();
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI is set but {} is missing ({err}): see crates/squirrel-media/tests/fixtures/README.md",
                path.display()
            );
            eprintln!("skipped: {} is missing", path.display());
            None
        }
    }
}

/// The JPEG `rotated.heic` is encoded from: 64 × 32 with EXIF orientation 6
/// (`tests/fixtures/rotated-source.jpg`), so a correct decode is 32 × 64.
pub fn rotated_source_jpeg() -> Vec<u8> {
    jpeg_sized_with_exif(
        64,
        32,
        &ExifSpec {
            orientation: Some(6),
            ..ExifSpec::default()
        },
    )
}

/// A valid one-page PDF (A4 portrait) whose page shows `text` in Helvetica.
pub fn pdf_with_text(text: &str) -> Vec<u8> {
    pdf_with_pages(&[text], "")
}

/// A valid PDF with one A4 page per entry of `pages`, each showing its text.
/// `trailer_extra` is appended to the trailer dictionary.
pub fn pdf_with_pages(pages: &[&str], trailer_extra: &str) -> Vec<u8> {
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + 2 * i).collect();
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    for (text, id) in pages.iter().zip(&page_ids) {
        let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            id + 1
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer_extra}>>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// A PDF that needs a user password to open (standard security handler,
/// revision 2, with a `/U` entry no empty password produces).
pub fn encrypted_pdf() -> Vec<u8> {
    let hex32 = "AB".repeat(32);
    let encrypt = format!(
        "/Encrypt << /Filter /Standard /V 1 /R 2 /Length 40 /P -4 /O <{hex32}> /U <{hex32}> >> \
         /ID [<{id}> <{id}>] ",
        id = "01".repeat(16)
    );
    pdf_with_pages(&["Secret"], &encrypt)
}

#[test]
fn generated_pdfs_have_a_consistent_xref() {
    let pdf = pdf_with_text("Hello");
    let text = String::from_utf8(pdf).unwrap();
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(text[start..].starts_with("xref\n0 6\n"));
    let two = String::from_utf8(pdf_with_pages(&["a", "b"], "")).unwrap();
    assert!(two.contains("/Kids [4 0 R 6 0 R] /Count 2"));
    let obj5 = text.find("5 0 obj").unwrap();
    assert!(text.contains(&format!("{obj5:010} 00000 n ")));
}
