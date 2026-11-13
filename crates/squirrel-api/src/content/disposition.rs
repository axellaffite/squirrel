//! `Content-Disposition` with RFC 5987 filenames.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// RFC 5987 `attr-char`: everything else in `filename*` is percent-encoded.
const ATTR_CHAR_ESCAPES: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// `Content-Disposition` value with an ASCII `filename` fallback and the exact
/// UTF-8 name in `filename*` (RFC 6266 / RFC 5987).
pub fn content_disposition(inline: bool, name: &str) -> String {
    let kind = if inline { "inline" } else { "attachment" };
    let fallback: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' && c != '%' => c,
            _ => '_',
        })
        .collect();
    let encoded = utf8_percent_encode(name, ATTR_CHAR_ESCAPES);
    format!("{kind}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_names_are_kept() {
        assert_eq!(
            content_disposition(true, "report.pdf"),
            "inline; filename=\"report.pdf\"; filename*=UTF-8''report.pdf"
        );
    }

    #[test]
    fn unicode_and_quotes_are_encoded() {
        assert_eq!(
            content_disposition(false, "café \"menu\".pdf"),
            "attachment; filename=\"caf_ _menu_.pdf\"; filename*=UTF-8''caf%C3%A9%20%22menu%22.pdf"
        );
    }

    #[test]
    fn header_injection_characters_never_appear_raw() {
        let value = content_disposition(false, "a\r\nSet-Cookie: x.txt");
        assert!(!value.contains('\r') && !value.contains('\n'));
    }
}
