/// A validated, lowercase username: 3–32 chars of `[a-z0-9._-]`, starting with `[a-z0-9]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsernameError {
    #[error("username must be between 3 and 32 characters")]
    Length,
    #[error("username may only contain lowercase letters, digits, '.', '_' and '-'")]
    Charset,
    #[error("username must start with a letter or a digit")]
    Start,
}

impl Username {
    pub const MIN_LEN: usize = 3;
    pub const MAX_LEN: usize = 32;

    pub fn parse(raw: &str) -> Result<Self, UsernameError> {
        let normalized = raw.trim().to_ascii_lowercase();
        let len = normalized.chars().count();
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&len) {
            return Err(UsernameError::Length);
        }
        let allowed =
            |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-');
        if !normalized.chars().all(allowed) {
            return Err(UsernameError::Charset);
        }
        if !normalized.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return Err(UsernameError::Start);
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
    fn normalizes_case_and_whitespace() {
        assert_eq!(Username::parse("  Alice ").unwrap().as_str(), "alice");
    }

    #[test]
    fn accepts_dots_underscores_dashes() {
        assert!(Username::parse("a.b_c-d").is_ok());
    }

    #[test]
    fn rejects_too_short_and_too_long() {
        assert_eq!(Username::parse("ab"), Err(UsernameError::Length));
        assert_eq!(Username::parse(&"a".repeat(33)), Err(UsernameError::Length));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(Username::parse("al ice"), Err(UsernameError::Charset));
        assert_eq!(Username::parse("alïce"), Err(UsernameError::Charset));
    }

    #[test]
    fn must_start_with_letter_or_digit() {
        assert_eq!(Username::parse(".alice"), Err(UsernameError::Start));
    }

    #[test]
    fn rejects_non_ascii_that_lowercases_to_ascii() {
        assert_eq!(
            Username::parse("\u{212A}elvin"),
            Err(UsernameError::Charset)
        );
    }

    #[test]
    fn length_boundaries() {
        assert!(Username::parse("abc").is_ok());
        assert!(Username::parse(&"a".repeat(32)).is_ok());
        assert_eq!(Username::parse("  ab  "), Err(UsernameError::Length));
    }

    #[test]
    fn start_rules() {
        assert!(Username::parse("1abc").is_ok());
        assert_eq!(Username::parse("_abc"), Err(UsernameError::Start));
        assert_eq!(Username::parse("-abc"), Err(UsernameError::Start));
    }
}
