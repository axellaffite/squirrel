//! Request and response bodies of the document search.

use serde::{Deserialize, Serialize};
use squirrel_db::documents::{HIGHLIGHT_END, HIGHLIGHT_START};
use utoipa::{IntoParams, ToSchema};

use crate::files_dto::NodeDto;

/// Characters of snippet text returned per hit, at most.
pub const MAX_SNIPPET_CHARS: usize = 600;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// Words to find (1–200 characters): each must start a word of the
    /// file's name or text, accents and case ignored.
    pub q: Option<String>,
    /// Results to skip; `offset + limit` is at most 200.
    #[param(minimum = 0, maximum = 199)]
    pub offset: Option<u32>,
    /// Page size, 1–50, default 20.
    #[param(minimum = 1, maximum = 50)]
    pub limit: Option<u32>,
}

/// A piece of a snippet; `highlight` marks a matched word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct SnippetPart {
    pub text: String,
    pub highlight: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentHit {
    pub node: NodeDto,
    /// Pages of a PDF; `null` for other files or before it is processed.
    pub page_count: Option<i32>,
    /// Passages of the text around the matches; empty when only the name matched.
    pub snippet: Vec<SnippetPart>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentResults {
    /// Name matches first, then by relevance.
    pub items: Vec<DocumentHit>,
    /// Whether a page at `offset + limit` would return more (never past
    /// 200 results).
    pub has_more: bool,
}

/// Splits a snippet whose matches sit between the database's start and end
/// marks into plain and highlighted parts. Unbalanced marks and any other
/// control character (except tabs and line breaks, turned into spaces) are
/// dropped; the text is cut at [`MAX_SNIPPET_CHARS`].
pub fn snippet_parts(raw: &str) -> Vec<SnippetPart> {
    let mut parts: Vec<SnippetPart> = Vec::new();
    let mut current = String::new();
    let mut highlight = false;
    let mut budget = MAX_SNIPPET_CHARS;
    for c in raw.chars() {
        match c {
            HIGHLIGHT_START if !highlight => {
                flush(&mut current, false, &mut parts);
                highlight = true;
            }
            HIGHLIGHT_END if highlight => {
                flush(&mut current, true, &mut parts);
                highlight = false;
            }
            '\n' | '\r' | '\t' if budget > 0 => {
                current.push(' ');
                budget -= 1;
            }
            c if c.is_control() => {}
            c if budget > 0 => {
                current.push(c);
                budget -= 1;
            }
            _ => {}
        }
    }
    // An unclosed highlight is shown as plain text.
    flush(&mut current, false, &mut parts);
    parts
}

fn flush(text: &mut String, highlight: bool, parts: &mut Vec<SnippetPart>) {
    if !text.is_empty() {
        parts.push(SnippetPart {
            text: std::mem::take(text),
            highlight,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(raw: &str) -> Vec<(String, bool)> {
        snippet_parts(raw)
            .into_iter()
            .map(|p| (p.text, p.highlight))
            .collect()
    }

    fn owned(expected: &[(&str, bool)]) -> Vec<(String, bool)> {
        expected
            .iter()
            .map(|(t, h)| ((*t).to_owned(), *h))
            .collect()
    }

    #[test]
    fn marks_become_highlighted_parts() {
        assert_eq!(
            parts("a \u{2}b\u{3} c"),
            owned(&[("a ", false), ("b", true), (" c", false)])
        );
        assert_eq!(
            parts("\u{2}x\u{3}\u{2}y\u{3}"),
            owned(&[("x", true), ("y", true)])
        );
    }

    #[test]
    fn stray_marks_and_controls_are_dropped() {
        assert_eq!(parts("a\u{3}b"), owned(&[("ab", false)]));
        assert_eq!(parts("a\u{2}b"), owned(&[("a", false), ("b", false)]));
        assert_eq!(
            parts("a\u{2}\u{2}b\u{3}\u{3}c\u{7}\nd"),
            owned(&[("a", false), ("b", true), ("c d", false)])
        );
        assert!(parts("").is_empty());
    }

    #[test]
    fn snippets_are_bounded() {
        let long = "x".repeat(10_000);
        let total: usize = parts(&long).iter().map(|(t, _)| t.chars().count()).sum();
        assert_eq!(total, MAX_SNIPPET_CHARS);
    }
}
