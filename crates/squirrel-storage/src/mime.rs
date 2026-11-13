/// Bytes of the file start used for content sniffing.
pub const SNIFF_LEN: usize = 8192;
const FALLBACK: &str = "application/octet-stream";

/// The MIME type of a file, from its leading bytes first (magic numbers via
/// `infer`), then from the name's extension, else `application/octet-stream`.
/// Never derived from what the client claims.
pub fn detect(head: &[u8], name: &str) -> String {
    if let Some(kind) = infer::get(head) {
        return kind.mime_type().to_owned();
    }
    mime_guess::from_path(name)
        .first_raw()
        .unwrap_or(FALLBACK)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_wins_over_the_extension() {
        assert_eq!(detect(b"%PDF-1.7\n", "notes.txt"), "application/pdf");
        assert_eq!(
            detect(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR", "x.jpg"),
            "image/png"
        );
    }

    #[test]
    fn falls_back_to_the_extension_then_octet_stream() {
        assert_eq!(detect(b"hello", "notes.txt"), "text/plain");
        assert_eq!(detect(b"hello", "notes"), "application/octet-stream");
    }

    #[test]
    fn sniffs_html_even_when_named_otherwise() {
        assert_eq!(detect(b"<!DOCTYPE html><html>", "photo.png"), "text/html");
    }
}
