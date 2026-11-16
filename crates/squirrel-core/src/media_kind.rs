//! Which stored files the media pipeline processes, by their sniffed MIME type.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Pdf,
    Text,
}

const IMAGES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/tiff",
    "image/bmp",
    "image/heic",
    "image/heif",
    "image/avif",
];
const HEIF_FAMILY: &[&str] = &["image/heic", "image/heif", "image/avif"];
const PDFS: &[&str] = &["application/pdf"];
const TEXTS: &[&str] = &["text/plain", "text/markdown"];

impl MediaKind {
    pub fn for_mime(mime: &str) -> Option<Self> {
        let essence = essence(mime);
        if IMAGES.contains(&essence.as_str()) {
            Some(Self::Image)
        } else if PDFS.contains(&essence.as_str()) {
            Some(Self::Pdf)
        } else if TEXTS.contains(&essence.as_str()) {
            Some(Self::Text)
        } else {
            None
        }
    }

    /// Decodable only when the server is built with the `heic` feature.
    pub fn is_heif_family(mime: &str) -> bool {
        HEIF_FAMILY.contains(&essence(mime).as_str())
    }

    /// The MIME types (essences) of this kind.
    pub fn mimes(self) -> &'static [&'static str] {
        match self {
            Self::Image => IMAGES,
            Self::Pdf => PDFS,
            Self::Text => TEXTS,
        }
    }

    /// Every MIME type the pipeline processes.
    pub fn all_mimes() -> Vec<&'static str> {
        [Self::Image, Self::Pdf, Self::Text]
            .into_iter()
            .flat_map(Self::mimes)
            .copied()
            .collect()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }
}

/// `image/JPEG; q=1` → `image/jpeg`.
fn essence(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

impl FromStr for MediaKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "image" => Ok(Self::Image),
            "pdf" => Ok(Self::Pdf),
            "text" => Ok(Self::Text),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_types() {
        assert_eq!(MediaKind::for_mime("image/jpeg"), Some(MediaKind::Image));
        assert_eq!(MediaKind::for_mime("image/heic"), Some(MediaKind::Image));
        assert_eq!(MediaKind::for_mime("application/pdf"), Some(MediaKind::Pdf));
        assert_eq!(MediaKind::for_mime("text/markdown"), Some(MediaKind::Text));
        assert_eq!(
            MediaKind::for_mime("image/svg+xml"),
            None,
            "active content is never decoded"
        );
        assert_eq!(MediaKind::for_mime("video/mp4"), None, "Phase 4");
        assert_eq!(MediaKind::for_mime("application/zip"), None);
    }

    #[test]
    fn every_listed_mime_maps_back_to_its_kind() {
        for kind in [MediaKind::Image, MediaKind::Pdf, MediaKind::Text] {
            for mime in kind.mimes() {
                assert_eq!(MediaKind::for_mime(mime), Some(kind), "{mime}");
            }
        }
        assert_eq!(MediaKind::all_mimes().len(), 12);
    }

    #[test]
    fn heif_family_needs_the_heic_feature() {
        assert!(MediaKind::is_heif_family("image/heic"));
        assert!(MediaKind::is_heif_family("image/avif"));
        assert!(!MediaKind::is_heif_family("image/jpeg"));
    }

    #[test]
    fn round_trips_through_its_database_name() {
        for kind in [MediaKind::Image, MediaKind::Pdf, MediaKind::Text] {
            assert_eq!(kind.as_str().parse::<MediaKind>(), Ok(kind));
        }
    }
}
