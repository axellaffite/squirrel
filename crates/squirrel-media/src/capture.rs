//! Which capture time and kind a photo gets: the `taken_at` fallback chain and
//! screenshot detection. Pure functions over what extraction found.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use crate::metadata::{CaptureTime, ImageMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenSource {
    Exif,
    Filename,
    Upload,
}

impl TakenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exif => "exif",
            Self::Filename => "filename",
            Self::Upload => "upload",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    pub local: PrimitiveDateTime,
    pub offset: Option<UtcOffset>,
    pub source: TakenSource,
}

/// EXIF, else a date in the file name, else the upload time (UTC). Candidates
/// more than a day in the future (wrong camera clock) are skipped.
pub fn resolve_taken(
    exif: Option<CaptureTime>,
    file_name: &str,
    uploaded_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Taken {
    let limit = now + time::Duration::days(1);
    let plausible = |local: PrimitiveDateTime, offset: Option<UtcOffset>| {
        local.assume_offset(offset.unwrap_or(UtcOffset::UTC)) <= limit
    };
    if let Some(capture) = exif.filter(|c| plausible(c.local, c.offset)) {
        return Taken {
            local: capture.local,
            offset: capture.offset,
            source: TakenSource::Exif,
        };
    }
    if let Some(local) = date_in_name(file_name).filter(|local| plausible(*local, None)) {
        return Taken {
            local,
            offset: None,
            source: TakenSource::Filename,
        };
    }
    let utc = uploaded_at.to_offset(UtcOffset::UTC);
    Taken {
        local: PrimitiveDateTime::new(utc.date(), utc.time()),
        offset: Some(UtcOffset::UTC),
        source: TakenSource::Upload,
    }
}

/// A capture date embedded in common phone and app file names:
/// `IMG_20240714_183005`, `PXL_20240714_183005123`, `VID_…`, `Screenshot_2024-07-14-18-30-05`,
/// `Screenshot 2024-07-14 at 18.30.05`, `2024-07-14 18.30.05`, `IMG-20240714-WA0001` (date only: noon).
pub fn date_in_name(name: &str) -> Option<PrimitiveDateTime> {
    let digits: Vec<(usize, u32)> = name
        .char_indices()
        .filter_map(|(i, c)| c.to_digit(10).map(|d| (i, d)))
        .collect();
    // Find the first run that starts with a plausible year and yields a date.
    (0..digits.len()).find_map(|start| parse_from(&digits[start..], name))
}

/// Reads YYYY MM DD [hh mm ss] from consecutive digit groups separated by at
/// most one non-digit character each (`-`, `_`, `.`, ` `, `:`), as in names.
fn parse_from(digits: &[(usize, u32)], name: &str) -> Option<PrimitiveDateTime> {
    let bytes = name.as_bytes();
    let mut values = Vec::with_capacity(14);
    let mut last: Option<usize> = None;
    for &(index, digit) in digits {
        if let Some(prev) = last {
            let gap = index - prev - 1;
            if gap > 1 || (gap == 1 && !b"-_. :T".contains(&bytes[prev + 1])) {
                // " at " between date and time (macOS screenshots).
                if !(gap == 4 && &bytes[prev + 1..index] == b" at ") {
                    break;
                }
            }
        }
        values.push(digit);
        last = Some(index);
        if values.len() == 14 {
            break;
        }
    }
    if values.len() < 8 {
        return None;
    }
    let number = |range: std::ops::Range<usize>| range.fold(0u32, |acc, i| acc * 10 + values[i]);
    let year = i32::try_from(number(0..4)).ok()?;
    if !(1990..=2100).contains(&year) {
        return None;
    }
    let date = Date::from_calendar_date(
        year,
        Month::try_from(u8::try_from(number(4..6)).ok()?).ok()?,
        u8::try_from(number(6..8)).ok()?,
    )
    .ok()?;
    let time = if values.len() >= 14 {
        Time::from_hms(
            u8::try_from(number(8..10)).ok()?,
            u8::try_from(number(10..12)).ok()?,
            u8::try_from(number(12..14)).ok()?,
        )
        .ok()?
    } else {
        Time::from_hms(12, 0, 0).ok()?
    };
    Some(PrimitiveDateTime::new(date, time))
}

const SCREENSHOT_PREFIXES: [&str; 6] = [
    "screenshot",
    "screen shot",
    "capture d’écran",
    "capture d'écran",
    "bildschirmfoto",
    "schermafbeelding",
];
/// Phone screens are at least 19.5:9 ≈ 2.17; 1.9 leaves room for status bars cropped off.
const SCREEN_ASPECT: f64 = 1.9;

/// Screenshot: the EXIF says so, the name says so, or a PNG without a camera
/// that has a phone screen's shape.
pub fn is_screenshot(
    meta: &ImageMetadata,
    file_name: &str,
    mime: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> bool {
    let lower = file_name.to_lowercase();
    if meta.says_screenshot
        || SCREENSHOT_PREFIXES.iter().any(|p| lower.starts_with(p))
        || lower.starts_with("captura de pantalla")
    {
        return true;
    }
    let no_camera = meta.camera_make.is_none() && meta.camera_model.is_none();
    match (mime, width, height) {
        ("image/png", Some(w), Some(h)) if no_camera && w > 0 && h > 0 => {
            let ratio = f64::from(w.max(h)) / f64::from(w.min(h));
            ratio >= SCREEN_ASPECT
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{datetime, offset};

    const NOW: OffsetDateTime = datetime!(2026-11-14 12:00 UTC);

    #[test]
    fn names_from_phones_and_apps() {
        let cases = [
            ("IMG_20240714_183005.jpg", datetime!(2024-07-14 18:30:05)),
            ("PXL_20240714_183005123.jpg", datetime!(2024-07-14 18:30:05)),
            (
                "Screenshot_2024-07-14-18-30-05.png",
                datetime!(2024-07-14 18:30:05),
            ),
            (
                "Screenshot 2024-07-14 at 18.30.05.png",
                datetime!(2024-07-14 18:30:05),
            ),
            ("2024-07-14 18.30.05.jpg", datetime!(2024-07-14 18:30:05)),
            ("IMG-20240714-WA0001.jpg", datetime!(2024-07-14 12:00:00)),
        ];
        for (name, expected) in cases {
            assert_eq!(date_in_name(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn names_without_dates_give_none() {
        for name in [
            "holiday.jpg",
            "DSC_0042.JPG",
            "IMG_1234.HEIC",
            "20241399_000000.jpg",
            "18000101_101010.jpg",
        ] {
            assert_eq!(date_in_name(name), None, "{name}");
        }
    }

    #[test]
    fn exif_wins_then_name_then_upload() {
        let exif = CaptureTime {
            local: datetime!(2023-01-02 03:04:05),
            offset: Some(offset!(+1)),
        };
        let uploaded = datetime!(2026-11-01 10:00 UTC);
        let taken = resolve_taken(Some(exif), "IMG_20240714_183005.jpg", uploaded, NOW);
        assert_eq!((taken.local, taken.source), (exif.local, TakenSource::Exif));
        let taken = resolve_taken(None, "IMG_20240714_183005.jpg", uploaded, NOW);
        assert_eq!(
            (taken.local, taken.offset, taken.source),
            (datetime!(2024-07-14 18:30:05), None, TakenSource::Filename)
        );
        let taken = resolve_taken(None, "holiday.jpg", uploaded, NOW);
        assert_eq!(
            (taken.local, taken.source),
            (datetime!(2026-11-01 10:00), TakenSource::Upload)
        );
    }

    #[test]
    fn future_dates_are_skipped() {
        let future = CaptureTime {
            local: datetime!(2030-01-01 00:00),
            offset: None,
        };
        let taken = resolve_taken(Some(future), "x.jpg", datetime!(2026-11-01 10:00 UTC), NOW);
        assert_eq!(taken.source, TakenSource::Upload);
    }

    #[test]
    fn screenshots_by_comment_name_or_shape() {
        let plain = ImageMetadata::default();
        let commented = ImageMetadata {
            says_screenshot: true,
            ..ImageMetadata::default()
        };
        assert!(is_screenshot(
            &commented,
            "IMG_1.jpg",
            "image/jpeg",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "Screenshot_20240714.png",
            "image/png",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "Capture d’écran 2024-07-14.png",
            "image/png",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "x.png",
            "image/png",
            Some(1080),
            Some(2400)
        ));
        assert!(!is_screenshot(
            &plain,
            "x.png",
            "image/png",
            Some(1200),
            Some(900)
        ));
        let camera = ImageMetadata {
            camera_make: Some("Apple".into()),
            ..ImageMetadata::default()
        };
        assert!(!is_screenshot(
            &camera,
            "pano.png",
            "image/png",
            Some(4000),
            Some(1000)
        ));
        assert!(!is_screenshot(
            &plain,
            "pano.jpg",
            "image/jpeg",
            Some(4000),
            Some(1000)
        ));
    }
}
