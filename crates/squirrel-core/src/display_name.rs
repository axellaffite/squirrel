#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayNameError {
    #[error("display name must be between 1 and 64 characters")]
    Length,
    #[error("display name must not contain control characters")]
    Control,
}

impl DisplayName {
    pub const MAX_LEN: usize = 64;

    pub fn parse(raw: &str) -> Result<Self, DisplayNameError> {
        let trimmed = raw.trim();
        let len = trimmed.chars().count();
        if len == 0 || len > Self::MAX_LEN {
            return Err(DisplayNameError::Length);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DisplayNameError::Control);
        }
        Ok(Self(trimmed.to_owned()))
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
}
