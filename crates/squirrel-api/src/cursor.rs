//! Opaque pagination cursors: base64url of `"<a>\n<b>"`. Node names cannot
//! contain control characters, so the newline is a safe separator.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use squirrel_core::NodeKind;
use squirrel_db::nodes::ListCursor;
use squirrel_db::timeline::TimelineCursor;
use squirrel_db::trash::TrashCursor;
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

use crate::error::ApiError;

pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 500;
/// Encoded cursors are far shorter (a node name is at most 255 bytes).
const MAX_CURSOR_LEN: usize = 1024;

pub fn page_size(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE)
}

fn invalid() -> ApiError {
    ApiError::BadRequest("invalid cursor".into())
}

fn encode(first: &str, second: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{first}\n{second}"))
}

/// Decoding stops at the separator: neither part may hold a control
/// character (a NUL would otherwise reach Postgres and fail as a 500).
fn decode(raw: &str) -> Result<(String, String), ApiError> {
    if raw.len() > MAX_CURSOR_LEN {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let (first, second) = text.split_once('\n').ok_or_else(invalid)?;
    if first.chars().chain(second.chars()).any(char::is_control) {
        return Err(invalid());
    }
    Ok((first.to_owned(), second.to_owned()))
}

pub fn encode_folder(cursor: &ListCursor) -> String {
    encode(cursor.kind.as_str(), &cursor.name)
}

pub fn decode_folder(raw: &str) -> Result<ListCursor, ApiError> {
    let (kind, name) = decode(raw)?;
    let kind = kind.parse::<NodeKind>().map_err(|_| invalid())?;
    Ok(ListCursor { kind, name })
}

/// Years a cursor time may have: Postgres cannot store most of what `time`
/// can (years before 4713 BC fail as "timestamp out of range", a 500), and
/// no real row is dated outside these.
const CURSOR_YEARS: std::ops::RangeInclusive<i32> = 1..=9999;

/// The instant `nanos` after the epoch, within [`CURSOR_YEARS`].
fn cursor_time(nanos: &str) -> Result<OffsetDateTime, ApiError> {
    let nanos: i128 = nanos.parse().map_err(|_| invalid())?;
    let at = OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| invalid())?;
    if !CURSOR_YEARS.contains(&at.year()) {
        return Err(invalid());
    }
    Ok(at)
}

pub fn encode_trash(cursor: &TrashCursor) -> String {
    encode(
        &cursor.deleted_at.unix_timestamp_nanos().to_string(),
        &cursor.id.to_string(),
    )
}

pub fn decode_trash(raw: &str) -> Result<TrashCursor, ApiError> {
    let (nanos, id) = decode(raw)?;
    Ok(TrashCursor {
        deleted_at: cursor_time(&nanos)?,
        id: id.parse::<Uuid>().map_err(|_| invalid())?,
    })
}

/// The wall-clock `taken_at` is encoded as if it were UTC (it only orders).
pub fn encode_timeline(cursor: &TimelineCursor) -> String {
    encode(
        &cursor
            .taken_at
            .assume_utc()
            .unix_timestamp_nanos()
            .to_string(),
        &cursor.node_id.to_string(),
    )
}

pub fn decode_timeline(raw: &str) -> Result<TimelineCursor, ApiError> {
    let (nanos, id) = decode(raw)?;
    let at = cursor_time(&nanos)?;
    Ok(TimelineCursor {
        taken_at: PrimitiveDateTime::new(at.date(), at.time()),
        node_id: id.parse::<Uuid>().map_err(|_| invalid())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn folder_cursors_round_trip_any_name() {
        let cursor = ListCursor {
            kind: NodeKind::File,
            name: "Été 2026 (1).jpg".into(),
        };
        assert_eq!(decode_folder(&encode_folder(&cursor)).unwrap(), cursor);
    }

    #[test]
    fn trash_cursors_keep_microseconds() {
        let cursor = TrashCursor {
            deleted_at: datetime!(2026-11-13 10:00:00.123456 UTC),
            id: Uuid::now_v7(),
        };
        assert_eq!(decode_trash(&encode_trash(&cursor)).unwrap(), cursor);
    }

    #[test]
    fn timeline_cursors_round_trip_wall_clock_times() {
        for taken_at in [
            datetime!(2024-07-14 18:30:05.123456),
            datetime!(1900-01-01 00:00),
            datetime!(2099-12-31 23:59:59),
        ] {
            let cursor = TimelineCursor {
                taken_at,
                node_id: Uuid::now_v7(),
            };
            assert_eq!(decode_timeline(&encode_timeline(&cursor)).unwrap(), cursor);
        }
    }

    #[test]
    fn garbage_timeline_cursors_are_a_bad_request() {
        for raw in [
            "".to_owned(),
            "!!!".to_owned(),
            URL_SAFE_NO_PAD.encode(
                "x
y",
            ),
            URL_SAFE_NO_PAD.encode(format!(
                "1
{}x",
                Uuid::now_v7()
            )),
            URL_SAFE_NO_PAD.encode(format!(
                "{}
{}",
                i128::MAX,
                Uuid::now_v7()
            )),
        ] {
            assert!(
                matches!(decode_timeline(&raw), Err(ApiError::BadRequest(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn times_postgres_cannot_store_are_a_bad_request() {
        let id = Uuid::now_v7();
        for at in [
            datetime!(-5000-01-01 00:00 UTC),
            datetime!(0000-12-31 23:59:59 UTC),
        ] {
            let raw = URL_SAFE_NO_PAD.encode(format!("{}\n{id}", at.unix_timestamp_nanos()));
            assert!(matches!(
                decode_timeline(&raw),
                Err(ApiError::BadRequest(_))
            ));
            assert!(matches!(decode_trash(&raw), Err(ApiError::BadRequest(_))));
        }
        for at in [
            datetime!(0001-01-01 00:00 UTC),
            datetime!(9999-12-31 23:59 UTC),
        ] {
            let raw = URL_SAFE_NO_PAD.encode(format!("{}\n{id}", at.unix_timestamp_nanos()));
            assert!(decode_timeline(&raw).is_ok());
            assert!(decode_trash(&raw).is_ok());
        }
    }

    #[test]
    fn garbage_is_a_bad_request() {
        for raw in [
            "",
            "!!!",
            &URL_SAFE_NO_PAD.encode("no-separator"),
            &URL_SAFE_NO_PAD.encode("symlink\nx"),
        ] {
            assert!(
                matches!(decode_folder(raw), Err(ApiError::BadRequest(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn control_characters_are_a_bad_request() {
        for text in [
            "file\nbad\0name",
            "file\na\nb",
            "file\n\u{7f}",
            "\u{1b}file\nx",
        ] {
            let raw = URL_SAFE_NO_PAD.encode(text);
            assert!(
                matches!(decode_folder(&raw), Err(ApiError::BadRequest(_))),
                "{text:?}"
            );
        }
        let trash = URL_SAFE_NO_PAD.encode(format!("1\n{}\0", Uuid::now_v7()));
        assert!(matches!(decode_trash(&trash), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn oversized_cursors_are_a_bad_request() {
        let long = URL_SAFE_NO_PAD.encode(format!("file\n{}", "a".repeat(MAX_CURSOR_LEN)));
        assert!(matches!(decode_folder(&long), Err(ApiError::BadRequest(_))));
        let fits = ListCursor {
            kind: NodeKind::File,
            name: "a".repeat(255),
        };
        assert_eq!(decode_folder(&encode_folder(&fits)).unwrap(), fits);
    }

    #[test]
    fn page_sizes_are_clamped() {
        assert_eq!(page_size(None), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some(0)), 1);
        assert_eq!(page_size(Some(10_000)), MAX_PAGE_SIZE);
    }
}
