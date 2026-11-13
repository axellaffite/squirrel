use std::fmt;

use unicode_normalization::UnicodeNormalization;

/// A validated file or folder name: trimmed, NFC-normalized, 1–255 UTF-8 bytes,
/// no `/`, no control or bidirectional-override characters, not `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeNameError {
    #[error("name must not be empty")]
    Empty,
    #[error("name must be at most 255 bytes")]
    TooLong,
    #[error("name must not be \".\" or \"..\"")]
    Reserved,
    #[error("name must not contain \"/\"")]
    Slash,
    #[error("name must not contain control or text-direction characters")]
    Control,
}

impl NodeName {
    pub const MAX_BYTES: usize = 255;

    pub fn parse(raw: &str) -> Result<Self, NodeNameError> {
        let name: String = raw.trim().nfc().collect();
        if name.is_empty() {
            return Err(NodeNameError::Empty);
        }
        if name.len() > Self::MAX_BYTES {
            return Err(NodeNameError::TooLong);
        }
        if name == "." || name == ".." {
            return Err(NodeNameError::Reserved);
        }
        if name.contains('/') {
            return Err(NodeNameError::Slash);
        }
        if name.chars().any(|c| c.is_control() || is_bidi_control(c)) {
            return Err(NodeNameError::Control);
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `n`-th alternative for a name that is taken: `report (1).pdf`,
    /// `report (2).pdf`… The suffix goes before the last extension; dotfiles and
    /// names without an extension get it at the end. `n == 0` is the name itself.
    /// The stem is shortened (on a character boundary) to stay within 255 bytes.
    pub fn variant(&self, n: u32) -> Self {
        if n == 0 {
            return self.clone();
        }
        let (stem, ext) = match self.0.rfind('.') {
            Some(dot) if dot > 0 => self.0.split_at(dot),
            _ => (self.0.as_str(), ""),
        };
        let suffix = format!(" ({n}){ext}");
        let budget = Self::MAX_BYTES.saturating_sub(suffix.len());
        let mut cut = stem.len().min(budget);
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        Self(format!("{}{suffix}", &stem[..cut]))
    }
}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Bidirectional controls that can make `invoice\u{202E}fdp.exe` render as `invoice.exe.pdf`.
/// Other format characters (e.g. the zero-width joiner in emoji) stay allowed.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_normalizes_to_nfc() {
        assert_eq!(
            NodeName::parse("  e\u{301}t\u{e9}.txt ").unwrap().as_str(),
            "\u{e9}t\u{e9}.txt"
        );
    }

    #[test]
    fn rejects_empty_dot_and_dotdot() {
        assert_eq!(NodeName::parse("   "), Err(NodeNameError::Empty));
        assert_eq!(NodeName::parse("."), Err(NodeNameError::Reserved));
        assert_eq!(NodeName::parse(".."), Err(NodeNameError::Reserved));
        assert!(NodeName::parse("...").is_ok());
        assert!(NodeName::parse(".bashrc").is_ok());
    }

    #[test]
    fn rejects_slashes_and_control_characters() {
        assert_eq!(NodeName::parse("a/b"), Err(NodeNameError::Slash));
        assert_eq!(NodeName::parse("a\0b"), Err(NodeNameError::Control));
        assert_eq!(NodeName::parse("a\nb"), Err(NodeNameError::Control));
        assert_eq!(
            NodeName::parse("invoice\u{202E}fdp.exe"),
            Err(NodeNameError::Control)
        );
    }

    #[test]
    fn allows_backslashes_emoji_and_zero_width_joiner() {
        assert!(NodeName::parse("a\\b").is_ok());
        assert!(NodeName::parse("family \u{1F468}\u{200D}\u{1F469}.jpg").is_ok());
    }

    #[test]
    fn limits_length_in_bytes_after_normalization() {
        assert!(NodeName::parse(&"a".repeat(255)).is_ok());
        assert_eq!(
            NodeName::parse(&"a".repeat(256)),
            Err(NodeNameError::TooLong)
        );
        assert_eq!(
            NodeName::parse(&"\u{e9}".repeat(128)),
            Err(NodeNameError::TooLong)
        );
    }

    #[test]
    fn variants_insert_a_counter_before_the_extension() {
        let name = NodeName::parse("report.pdf").unwrap();
        assert_eq!(name.variant(0).as_str(), "report.pdf");
        assert_eq!(name.variant(1).as_str(), "report (1).pdf");
        assert_eq!(
            NodeName::parse("archive.tar.gz")
                .unwrap()
                .variant(2)
                .as_str(),
            "archive.tar (2).gz"
        );
        assert_eq!(
            NodeName::parse("README").unwrap().variant(1).as_str(),
            "README (1)"
        );
        assert_eq!(
            NodeName::parse(".bashrc").unwrap().variant(1).as_str(),
            ".bashrc (1)"
        );
    }

    #[test]
    fn variants_stay_within_the_byte_limit() {
        let long = NodeName::parse(&format!("{}.txt", "\u{e9}".repeat(125))).unwrap();
        let variant = long.variant(12);
        assert!(variant.as_str().len() <= NodeName::MAX_BYTES);
        assert!(variant.as_str().ends_with(" (12).txt"));
    }
}
