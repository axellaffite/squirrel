use unicode_normalization::UnicodeNormalization;

/// A trimmed, NFC-normalized display name free of control and invisible
/// formatting characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayNameError {
    #[error("display name must be between 1 and 64 characters")]
    Length,
    #[error("display name must not contain control or invisible formatting characters")]
    Control,
}

impl DisplayName {
    pub const MAX_LEN: usize = 64;

    pub fn parse(raw: &str) -> Result<Self, DisplayNameError> {
        let normalized: String = raw.trim().nfc().collect();
        let len = normalized.chars().count();
        if len == 0 || len > Self::MAX_LEN {
            return Err(DisplayNameError::Length);
        }
        if normalized
            .chars()
            .any(|c| c.is_control() || is_format_char(c))
        {
            return Err(DisplayNameError::Control);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Unicode general category Cf (format) characters: invisible characters and
/// bidirectional controls that can spoof how a name renders.
pub(crate) fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_bounds_length() {
        assert_eq!(DisplayName::parse("  Alice  ").unwrap().as_str(), "Alice");
        assert_eq!(DisplayName::parse("   "), Err(DisplayNameError::Length));
        assert_eq!(
            DisplayName::parse(&"é".repeat(65)),
            Err(DisplayNameError::Length)
        );
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(
            DisplayName::parse("Al\u{0}ice"),
            Err(DisplayNameError::Control)
        );
    }

    #[test]
    fn rejects_bidi_override() {
        assert_eq!(
            DisplayName::parse("admin\u{202E}gpj.exe"),
            Err(DisplayNameError::Control)
        );
    }

    #[test]
    fn rejects_invisible_format_characters() {
        assert_eq!(
            DisplayName::parse("\u{200B}"),
            Err(DisplayNameError::Control)
        );
        for c in [
            '\u{00AD}', '\u{061C}', '\u{180E}', '\u{2060}', '\u{2066}', '\u{FEFF}',
        ] {
            assert_eq!(
                DisplayName::parse(&format!("Al{c}ice")),
                Err(DisplayNameError::Control),
                "{c:?}"
            );
        }
    }

    #[test]
    fn normalizes_to_nfc_before_counting() {
        let decomposed = "e\u{301}".repeat(64);
        let name = DisplayName::parse(&decomposed).unwrap();
        assert_eq!(name.as_str(), "é".repeat(64));
    }
}
