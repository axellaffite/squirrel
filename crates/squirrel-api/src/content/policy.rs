//! Which `Content-Type`, disposition and CSP a stored file is served with.

/// How a stored file may be served to a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPolicy {
    pub content_type: String,
    pub inline: bool,
    pub csp: &'static str,
}

/// Inline content runs in a sandbox: no scripts, an opaque origin, and only its
/// own bytes may load as an image or media.
pub const SANDBOX_CSP: &str = "sandbox; default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; frame-ancestors 'self'";
/// Built-in PDF viewers refuse to start in a sandboxed document, so inline PDFs
/// get a strict policy without `sandbox` (see Task 25 for the browser checks):
/// no scripts, form submissions or `<base>` rewrites in the document itself.
pub const PDF_CSP: &str = "default-src 'none'; object-src 'self'; script-src 'none'; \
     form-action 'none'; base-uri 'none'; frame-ancestors 'self'";
const OCTET_STREAM: &str = "application/octet-stream";
const PDF: &str = "application/pdf";
const TEXT: &str = "text/plain";

/// Types a browser displays without running code from the file.
const INLINE_TYPES: &[&str] = &[
    PDF,
    TEXT,
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/bmp",
    "video/mp4",
    "video/webm",
    "video/ogg",
    "audio/mpeg",
    "audio/ogg",
    "audio/wav",
    "audio/webm",
    "audio/flac",
    "audio/mp4",
];
/// Types that render active content when opened: never served under their own type.
const ACTIVE_TYPES: &[&str] = &[
    "text/html",
    "application/xhtml+xml",
    "image/svg+xml",
    "text/xml",
    "application/xml",
    "text/javascript",
    "application/javascript",
    "application/ecmascript",
    "text/ecmascript",
    "text/xsl",
    "application/x-shockwave-flash",
    "multipart/x-mixed-replace",
];

impl ContentPolicy {
    /// `mime` is the type sniffed at upload time; `force_download` is `?download=true`.
    pub fn for_mime(mime: Option<&str>, force_download: bool) -> Self {
        let essence = mime
            .and_then(|m| m.split(';').next())
            .map(|m| m.trim().to_ascii_lowercase())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| OCTET_STREAM.to_owned());
        if ACTIVE_TYPES.contains(&essence.as_str()) || essence.ends_with("+xml") {
            return Self {
                content_type: OCTET_STREAM.to_owned(),
                inline: false,
                csp: SANDBOX_CSP,
            };
        }
        let inline = !force_download && INLINE_TYPES.contains(&essence.as_str());
        let csp = if inline && essence == PDF {
            PDF_CSP
        } else {
            SANDBOX_CSP
        };
        let content_type = if essence == TEXT {
            "text/plain; charset=utf-8".to_owned()
        } else {
            essence
        };
        Self {
            content_type,
            inline,
            csp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfs_are_inline_without_sandbox_unless_downloaded() {
        let inline = ContentPolicy::for_mime(Some("application/pdf"), false);
        assert_eq!(
            (inline.content_type.as_str(), inline.inline, inline.csp),
            (PDF, true, PDF_CSP)
        );
        let download = ContentPolicy::for_mime(Some("application/pdf"), true);
        assert_eq!((download.inline, download.csp), (false, SANDBOX_CSP));
    }

    #[test]
    fn images_and_text_are_inline_and_sandboxed() {
        let png = ContentPolicy::for_mime(Some("image/png"), false);
        assert_eq!((png.inline, png.csp), (true, SANDBOX_CSP));
        let text = ContentPolicy::for_mime(Some("Text/Plain"), false);
        assert_eq!(text.content_type, "text/plain; charset=utf-8");
        assert!(text.inline);
    }

    #[test]
    fn active_types_are_downgraded_to_attachments() {
        for mime in [
            "text/html",
            "image/svg+xml",
            "application/xml",
            "application/rss+xml",
            "text/javascript",
        ] {
            let policy = ContentPolicy::for_mime(Some(mime), false);
            assert_eq!(policy.content_type, OCTET_STREAM, "{mime}");
            assert!(!policy.inline, "{mime}");
        }
    }

    #[test]
    fn unknown_types_are_attachments() {
        let none = ContentPolicy::for_mime(None, false);
        assert_eq!(
            (none.content_type.as_str(), none.inline),
            (OCTET_STREAM, false)
        );
        let zip = ContentPolicy::for_mime(Some("application/zip"), false);
        assert_eq!(
            (zip.content_type.as_str(), zip.inline),
            ("application/zip", false)
        );
    }
}
