//! EXIF metadata of an image file (JPEG, HEIC/HEIF, PNG, WebP, TIFF).

use std::io::{BufRead, Seek};

use exif::{Exif, Field, In, Tag, Value};
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

/// When the photo was taken, as the camera recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureTime {
    /// Wall-clock time where the photo was taken.
    pub local: PrimitiveDateTime,
    /// UTC offset, when the file records one (`OffsetTimeOriginal`, EXIF 2.31).
    pub offset: Option<UtcOffset>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsPosition {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres above sea level.
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageMetadata {
    pub taken: Option<CaptureTime>,
    pub gps: Option<GpsPosition>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    /// EXIF orientation 1..=8 (1 = upright).
    pub orientation: Option<u16>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    pub focal_length_mm: Option<f64>,
    /// `UserComment` or `ImageDescription` says "Screenshot" (iOS, some Androids).
    pub says_screenshot: bool,
}

/// Longest text value kept (make, model, lens); EXIF strings are untrusted.
const MAX_TEXT: usize = 128;

/// Parses EXIF from `reader`. A file without EXIF gives the default (empty)
/// metadata; only I/O errors are errors.
pub fn read_metadata<R: BufRead + Seek>(reader: &mut R) -> std::io::Result<ImageMetadata> {
    match exif::Reader::new().read_from_container(reader) {
        Ok(exif) => Ok(from_exif(&exif)),
        Err(exif::Error::Io(err)) => Err(err),
        Err(_) => Ok(ImageMetadata::default()),
    }
}

fn from_exif(exif: &Exif) -> ImageMetadata {
    let field = |tag| exif.get_field(tag, In::PRIMARY);
    ImageMetadata {
        taken: capture_time(exif),
        gps: gps(exif),
        camera_make: field(Tag::Make).and_then(text),
        camera_model: field(Tag::Model).and_then(text),
        lens_model: field(Tag::LensModel).and_then(text),
        orientation: field(Tag::Orientation)
            .and_then(|f| f.value.get_uint(0))
            .and_then(|o| u16::try_from(o).ok())
            .filter(|o| (1..=8).contains(o)),
        exposure_time: field(Tag::ExposureTime)
            .and_then(|f| rational(&f.value))
            .filter(|s| *s > 0.0)
            .map(format_exposure),
        f_number: field(Tag::FNumber).and_then(|f| rational(&f.value)),
        iso: field(Tag::PhotographicSensitivity).and_then(|f| f.value.get_uint(0)),
        focal_length_mm: field(Tag::FocalLength).and_then(|f| rational(&f.value)),
        says_screenshot: [Tag::UserComment, Tag::ImageDescription]
            .into_iter()
            .filter_map(field)
            .any(|f| raw_text(f).to_ascii_lowercase().contains("screenshot")),
    }
}

/// `DateTimeOriginal`, else `DateTimeDigitized`, else `DateTime`, with the
/// matching sub-seconds and offset tags when present.
fn capture_time(exif: &Exif) -> Option<CaptureTime> {
    const CANDIDATES: [(Tag, Tag, Tag); 3] = [
        (
            Tag::DateTimeOriginal,
            Tag::SubSecTimeOriginal,
            Tag::OffsetTimeOriginal,
        ),
        (
            Tag::DateTimeDigitized,
            Tag::SubSecTimeDigitized,
            Tag::OffsetTimeDigitized,
        ),
        (Tag::DateTime, Tag::SubSecTime, Tag::OffsetTime),
    ];
    CANDIDATES.iter().find_map(|&(main, subsec, offset)| {
        let mut dt = exif::DateTime::from_ascii(first_ascii(exif, main)?).ok()?;
        if let Some(raw) = first_ascii(exif, subsec) {
            let _ = dt.parse_subsec(raw);
        }
        if let Some(raw) = first_ascii(exif, offset) {
            let _ = dt.parse_offset(raw);
        }
        to_capture_time(&dt)
    })
}

/// The first string of an ASCII field of the primary image.
fn first_ascii(exif: &Exif, tag: Tag) -> Option<&[u8]> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(parts) => parts.first().map(Vec::as_slice),
        _ => None,
    }
}

fn to_capture_time(dt: &exif::DateTime) -> Option<CaptureTime> {
    // Cameras without a set clock write 0000:00:00 or 1970/2000 defaults.
    if dt.year < 1900 {
        return None;
    }
    let date =
        Date::from_calendar_date(i32::from(dt.year), Month::try_from(dt.month).ok()?, dt.day)
            .ok()?;
    let time =
        Time::from_hms_nano(dt.hour, dt.minute, dt.second, dt.nanosecond.unwrap_or(0)).ok()?;
    let offset = dt
        .offset
        .and_then(|minutes| UtcOffset::from_whole_seconds(i32::from(minutes) * 60).ok());
    Some(CaptureTime {
        local: PrimitiveDateTime::new(date, time),
        offset,
    })
}

fn gps(exif: &Exif) -> Option<GpsPosition> {
    let field = |tag| exif.get_field(tag, In::PRIMARY);
    let latitude = signed(
        dms(field(Tag::GPSLatitude)?)?,
        field(Tag::GPSLatitudeRef),
        b'S',
    )?;
    let longitude = signed(
        dms(field(Tag::GPSLongitude)?)?,
        field(Tag::GPSLongitudeRef),
        b'W',
    )?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    // (0, 0) is what phones write when they had no fix.
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }
    let altitude = field(Tag::GPSAltitude)
        .and_then(|f| rational(&f.value))
        .map(|alt| {
            let below = field(Tag::GPSAltitudeRef).and_then(|f| f.value.get_uint(0)) == Some(1);
            if below { -alt } else { alt }
        });
    Some(GpsPosition {
        latitude,
        longitude,
        altitude,
    })
}

/// Degrees, minutes, seconds → decimal degrees.
fn dms(field: &Field) -> Option<f64> {
    let Value::Rational(ref parts) = field.value else {
        return None;
    };
    let value = |i: usize| parts.get(i).filter(|r| r.denom != 0).map(|r| r.to_f64());
    let degrees = value(0)? + value(1).unwrap_or(0.0) / 60.0 + value(2).unwrap_or(0.0) / 3600.0;
    degrees.is_finite().then_some(degrees)
}

fn signed(value: f64, reference: Option<&Field>, negative: u8) -> Option<f64> {
    let is_negative = match reference.map(|f| &f.value) {
        Some(Value::Ascii(parts)) => parts.first().and_then(|p| p.first()) == Some(&negative),
        _ => false,
    };
    Some(if is_negative { -value } else { value })
}

fn rational(value: &Value) -> Option<f64> {
    match value {
        Value::Rational(parts) => parts.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        Value::SRational(parts) => parts.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// `1/250` for fast shutters, `2.5 s` for slow ones.
fn format_exposure(seconds: f64) -> String {
    if seconds >= 1.0 {
        format!("{} s", (seconds * 10.0).round() / 10.0)
    } else {
        format!("1/{}", (1.0 / seconds).round())
    }
}

/// Printable text of an ASCII or UNDEFINED field, trimmed and length-capped.
fn text(field: &Field) -> Option<String> {
    let value = raw_text(field);
    let cleaned: String = value
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn raw_text(field: &Field) -> String {
    match &field.value {
        Value::Ascii(parts) => parts
            .first()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .unwrap_or_default(),
        // UserComment: 8-byte character code, then the text.
        Value::Undefined(bytes, _) => {
            String::from_utf8_lossy(bytes.get(8..).unwrap_or_default()).into_owned()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_with_exif};
    use std::io::Cursor;
    use time::macros::{datetime, offset};

    #[test]
    fn reads_capture_time_with_offset_and_gps() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            date_time_original: Some("2024:07:14 18:30:05"),
            offset_time_original: Some("+02:00"),
            gps: Some(((48, 51, 2400), b'N', (2, 21, 300), b'E')),
            make: Some("Google"),
            model: Some("Pixel 8"),
            orientation: Some(6),
            ..ExifSpec::default()
        });
        let meta = read_metadata(&mut Cursor::new(jpeg)).unwrap();
        assert_eq!(
            meta.taken,
            Some(CaptureTime {
                local: datetime!(2024-07-14 18:30:05),
                offset: Some(offset!(+2)),
            })
        );
        let gps = meta.gps.unwrap();
        assert!((gps.latitude - 48.8566).abs() < 1e-3, "{gps:?}");
        assert!((gps.longitude - 2.3508).abs() < 1e-3, "{gps:?}");
        assert_eq!(meta.camera_make.as_deref(), Some("Google"));
        assert_eq!(meta.camera_model.as_deref(), Some("Pixel 8"));
        assert_eq!(meta.orientation, Some(6));
    }

    #[test]
    fn southern_and_western_coordinates_are_negative() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            gps: Some(((33, 52, 0), b'S', (151, 12, 0), b'W')),
            ..ExifSpec::default()
        });
        let gps = read_metadata(&mut Cursor::new(jpeg)).unwrap().gps.unwrap();
        assert!(gps.latitude < 0.0 && gps.longitude < 0.0, "{gps:?}");
    }

    #[test]
    fn null_island_and_unset_clocks_are_ignored() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            date_time_original: Some("0000:00:00 00:00:00"),
            gps: Some(((0, 0, 0), b'N', (0, 0, 0), b'E')),
            ..ExifSpec::default()
        });
        let meta = read_metadata(&mut Cursor::new(jpeg)).unwrap();
        assert_eq!((meta.taken, meta.gps), (None, None));
    }

    #[test]
    fn files_without_exif_give_empty_metadata() {
        let jpeg = jpeg_with_exif(&ExifSpec::default());
        assert_eq!(
            read_metadata(&mut Cursor::new(b"not an image".to_vec())).unwrap(),
            ImageMetadata::default()
        );
        assert!(
            read_metadata(&mut Cursor::new(jpeg))
                .unwrap()
                .taken
                .is_none()
        );
    }

    #[test]
    fn screenshot_comments_are_detected() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            user_comment: Some("Screenshot"),
            ..ExifSpec::default()
        });
        assert!(
            read_metadata(&mut Cursor::new(jpeg))
                .unwrap()
                .says_screenshot
        );
    }

    #[test]
    fn heic_png_and_webp_containers_are_read() {
        for name in ["exif.heic", "exif.png", "exif.webp"] {
            let bytes = std::fs::read(format!("tests/fixtures/{name}")).unwrap();
            // The kamadak fixtures carry a few fields but no capture time: parsing must succeed.
            let parsed = exif::Reader::new().read_from_container(&mut Cursor::new(bytes));
            assert!(parsed.is_ok(), "{name}: {:?}", parsed.err());
        }
    }

    #[test]
    fn exposure_is_formatted_like_cameras_do() {
        assert_eq!(format_exposure(0.004), "1/250");
        assert_eq!(format_exposure(2.5), "2.5 s");
    }
}
