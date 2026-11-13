//! `Range` header handling (RFC 9110 §14).

use std::ops::RangeInclusive;

use http_range_header::RangeUnsatisfiableError;

/// What to send for a `Range` request header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeRequest {
    /// No usable `Range`: the whole content with 200.
    Full,
    /// One satisfiable byte range (inclusive) with 206.
    Partial(RangeInclusive<u64>),
    /// Well-formed but outside the content: 416 with `Content-Range: bytes */size`.
    Unsatisfiable,
}

/// Interprets a `Range` header for content of `size` bytes. Only single ranges
/// are honoured; multiple ranges, malformed values and other units fall back to
/// the full content, which RFC 9110 §14.2 allows.
pub fn resolve(header: Option<&str>, size: u64) -> RangeRequest {
    let Some(value) = header else {
        return RangeRequest::Full;
    };
    let Ok(parsed) = http_range_header::parse_range_header(value) else {
        return RangeRequest::Full;
    };
    match parsed.validate(size) {
        Ok(mut ranges) if ranges.len() == 1 => RangeRequest::Partial(ranges.remove(0)),
        Ok(_) => RangeRequest::Full,
        Err(RangeUnsatisfiableError::Unsatisfiable) => RangeRequest::Unsatisfiable,
        Err(_) => RangeRequest::Full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_means_full_content() {
        assert_eq!(resolve(None, 10), RangeRequest::Full);
    }

    #[test]
    fn single_ranges_are_clamped_to_the_content() {
        assert_eq!(resolve(Some("bytes=0-3"), 10), RangeRequest::Partial(0..=3));
        assert_eq!(resolve(Some("bytes=5-"), 10), RangeRequest::Partial(5..=9));
        assert_eq!(resolve(Some("bytes=-4"), 10), RangeRequest::Partial(6..=9));
        assert_eq!(
            resolve(Some("bytes=8-100"), 10),
            RangeRequest::Partial(8..=9)
        );
    }

    #[test]
    fn ranges_past_the_end_are_unsatisfiable() {
        assert_eq!(resolve(Some("bytes=10-"), 10), RangeRequest::Unsatisfiable);
        assert_eq!(resolve(Some("bytes=0-0"), 0), RangeRequest::Unsatisfiable);
    }

    #[test]
    fn malformed_or_multiple_ranges_fall_back_to_full_content() {
        assert_eq!(resolve(Some("bytes=abc"), 10), RangeRequest::Full);
        assert_eq!(resolve(Some("items=0-1"), 10), RangeRequest::Full);
        assert_eq!(resolve(Some("bytes=0-1,4-5"), 10), RangeRequest::Full);
    }
}
