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

/// A valid one-page PDF (A4 portrait) whose page shows `text` in Helvetica.
pub fn pdf_with_text(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ),
    ];
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
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
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
    let obj5 = text.find("5 0 obj").unwrap();
    assert!(text.contains(&format!("{obj5:010} 00000 n ")));
}
