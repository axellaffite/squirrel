//! Entity tags for stored files.

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;

use super::ContentPolicy;

/// Strong ETag of one representation: the bytes (blob hash) and every header
/// derived from the node (type, CSP, disposition with the name). A rename or
/// `?download=true` is a different representation, so caches revalidate.
pub fn entity_tag(hash: &ContentHash, policy: &ContentPolicy, disposition: &str) -> String {
    let mut hasher = Sha256::new();
    // NUL never occurs in a hash, a MIME type, a CSP or a header value.
    for part in [
        hash.as_str(),
        policy.content_type.as_str(),
        policy.csp,
        disposition,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("\"{:x}\"", hasher.finalize())
}

/// `If-None-Match` matching (weak comparison, RFC 9110 §13.1.2): `*` or any listed tag.
pub fn etag_matches(if_none_match: Option<&str>, etag: &str) -> bool {
    if_none_match.is_some_and(|value| {
        value
            .split(',')
            .map(str::trim)
            .any(|tag| tag == "*" || tag.trim_start_matches("W/") == etag)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::content_disposition;

    const HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn tag(name: &str, download: bool) -> String {
        let policy = ContentPolicy::for_mime(Some("text/plain"), download);
        let disposition = content_disposition(policy.inline, name);
        entity_tag(&ContentHash::parse(HASH).unwrap(), &policy, &disposition)
    }

    #[test]
    fn tags_are_strong_and_stable() {
        let etag = tag("a.txt", false);
        assert!(etag.starts_with('"') && etag.ends_with('"'));
        assert_eq!(etag, tag("a.txt", false));
    }

    #[test]
    fn names_and_dispositions_change_the_tag() {
        assert_ne!(tag("a.txt", false), tag("b.txt", false));
        assert_ne!(tag("a.txt", false), tag("a.txt", true));
    }

    #[test]
    fn if_none_match_uses_weak_comparison() {
        let etag = "\"abc\"";
        assert!(etag_matches(Some("\"abc\""), etag));
        assert!(etag_matches(Some("W/\"abc\", \"def\""), etag));
        assert!(etag_matches(Some("*"), etag));
        assert!(!etag_matches(Some("\"def\""), etag));
        assert!(!etag_matches(None, etag));
    }
}
