//! Opaque pagination cursors: base64url of `"<a>\n<b>"`. Node names cannot
//! contain control characters, so the newline is a safe separator.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use squirrel_core::NodeKind;
use squirrel_db::nodes::ListCursor;
use squirrel_db::trash::TrashCursor;
use time::OffsetDateTime;
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

pub fn encode_trash(cursor: &TrashCursor) -> String {
    encode(
        &cursor.deleted_at.unix_timestamp_nanos().to_string(),
        &cursor.id.to_string(),
    )
}

pub fn decode_trash(raw: &str) -> Result<TrashCursor, ApiError> {
    let (nanos, id) = decode(raw)?;
    let nanos: i128 = nanos.parse().map_err(|_| invalid())?;
    Ok(TrashCursor {
        deleted_at: OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| invalid())?,
        id: id.parse::<Uuid>().map_err(|_| invalid())?,
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
