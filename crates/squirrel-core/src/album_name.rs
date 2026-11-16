use unicode_normalization::UnicodeNormalization;

use crate::display_name::is_format_char;

/// A trimmed, NFC-normalized album name of 1–100 characters, free of control
/// and invisible formatting characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlbumNameError {
    #[error("album name must be between 1 and 100 characters")]
    Length,
    #[error("album name must not contain control or invisible formatting characters")]
    Control,
}

impl AlbumName {
    pub const MAX_LEN: usize = 100;

    pub fn parse(raw: &str) -> Result<Self, AlbumNameError> {
        let normalized: String = raw.trim().nfc().collect();
        let len = normalized.chars().count();
        if len == 0 || len > Self::MAX_LEN {
            return Err(AlbumNameError::Length);
        }
        if normalized
            .chars()
            .any(|c| c.is_control() || is_format_char(c))
        {
            return Err(AlbumNameError::Control);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_bounds_length() {
        assert_eq!(
            AlbumName::parse("  Été 2024 ").unwrap().as_str(),
            "Été 2024"
        );
        assert_eq!(AlbumName::parse(""), Err(AlbumNameError::Length));
        assert_eq!(AlbumName::parse(" \t "), Err(AlbumNameError::Length));
        assert!(AlbumName::parse(&"é".repeat(100)).is_ok());
        assert_eq!(
            AlbumName::parse(&"é".repeat(101)),
            Err(AlbumNameError::Length)
        );
    }

    #[test]
    fn rejects_control_and_invisible_characters() {
        for raw in ["a\u{0}b", "a\nb", "a\u{7f}b", "a\u{202E}b", "a\u{200B}b"] {
            assert_eq!(
                AlbumName::parse(raw),
                Err(AlbumNameError::Control),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn normalizes_to_nfc_before_counting() {
        let name = AlbumName::parse(&"e\u{301}".repeat(100)).unwrap();
        assert_eq!(name.as_str(), "é".repeat(100));
    }
}
