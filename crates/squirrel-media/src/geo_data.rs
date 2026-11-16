//! Which geocoding dataset to use, and loading it only when it is the
//! expected file.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::geo::{GeoError, GeoIndex};

/// A dataset file identified by name and SHA-256 of its (uncompressed) bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub file_name: &'static str,
    pub sha256: &'static str,
}

/// The committed snapshot: `data/geodata/<file_name>.gz`.
/// Hash of the uncompressed TSV. Update both fields together with the file;
/// backfill then re-geocodes every geotagged photo.
pub const PINNED: Pinned = Pinned {
    file_name: "geonames-cities500-20261116.tsv",
    sha256: "946c4928b1b4af3421e34e45f7a45bda6ca2b1482d80976ef6ed4b3333dcbe00",
};

/// Directory of the installed dataset inside the data directory.
pub const DATA_SUBDIR: &str = "geo";
/// Larger files are refused before hashing (the real one is ~16 MB).
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum GeoDataError {
    #[error("geodata checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: String, actual: String },
    #[error("geodata file is larger than {MAX_FILE_BYTES} bytes")]
    TooLarge,
    #[error("reading geodata {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parsing geodata: {0}")]
    Parse(#[from] GeoError),
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, GeoDataError> {
    let io = |source| GeoDataError::Io {
        path: path.to_owned(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(GeoDataError::TooLarge);
    }
    Ok(bytes)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Loads `path` if its SHA-256 is `pin.sha256`. The bytes hashed are the bytes
/// parsed: the file cannot change in between.
pub fn load_verified(path: &Path, pin: &Pinned) -> Result<GeoIndex, GeoDataError> {
    let bytes = read_bounded(path)?;
    let actual = sha256_hex(&bytes);
    if actual != pin.sha256 {
        return Err(GeoDataError::Checksum {
            expected: pin.sha256.to_owned(),
            actual,
        });
    }
    Ok(GeoIndex::load(bytes.as_slice())?)
}

/// Where the worker's reverse geocoder gets its places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeoSource {
    /// No geocoding (tests).
    Disabled,
    /// An operator-chosen file (`SQUIRREL_GEODATA_FILE`), trusted without a pin.
    Explicit(PathBuf),
    /// The pinned snapshot, installed at this path; verified on load.
    Pinned(PathBuf),
}

impl GeoSource {
    /// `explicit` when set, else the pinned snapshot inside `data_dir`.
    pub fn resolve(explicit: Option<PathBuf>, data_dir: &Path) -> Self {
        match explicit {
            Some(path) => Self::Explicit(path),
            None => Self::Pinned(data_dir.join(DATA_SUBDIR).join(PINNED.file_name)),
        }
    }

    /// Recorded in `media.geocoded_with`: a different id means "geocode again".
    pub fn dataset_id(&self) -> Option<String> {
        match self {
            Self::Disabled => None,
            Self::Pinned(_) => Some(PINNED.file_name.to_owned()),
            Self::Explicit(path) => Some(format!(
                "custom:{}",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )),
        }
    }

    /// The index; `Ok(None)` when there is nothing to load yet (disabled, or
    /// the pinned snapshot is not installed). Slow (reads and parses ~16 MB):
    /// call it off the async runtime and cache the result.
    pub fn load(&self) -> Result<Option<GeoIndex>, GeoDataError> {
        match self {
            Self::Disabled => Ok(None),
            Self::Pinned(path) if !path.exists() => Ok(None),
            Self::Pinned(path) => load_verified(path, &PINNED).map(Some),
            Self::Explicit(path) => {
                let bytes = read_bounded(path)?;
                Ok(Some(GeoIndex::load(bytes.as_slice())?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        "# sample\n48.85341\t2.3488\tParis\tÎle-de-France\tFR\tFrance\tEurope/Paris\n";

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn pin_of(text: &str) -> Pinned {
        Pinned {
            file_name: "sample.tsv",
            sha256: Box::leak(sha256_hex(text.as_bytes()).into_boxed_str()),
        }
    }

    #[test]
    fn loads_a_file_matching_its_pin() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "sample.tsv", SAMPLE);
        let index = load_verified(&path, &pin_of(SAMPLE)).unwrap();
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn a_modified_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "sample.tsv", &SAMPLE.replace("Paris", "Evil"));
        let err = load_verified(&path, &pin_of(SAMPLE)).err().unwrap();
        match err {
            GeoDataError::Checksum { expected, actual } => {
                assert_eq!(expected, pin_of(SAMPLE).sha256);
                assert_ne!(actual, expected);
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn sources_resolve_and_name_their_dataset() {
        let data_dir = Path::new("/srv/squirrel");
        let pinned = GeoSource::resolve(None, data_dir);
        assert_eq!(
            pinned,
            GeoSource::Pinned(data_dir.join("geo").join(PINNED.file_name))
        );
        assert_eq!(pinned.dataset_id().as_deref(), Some(PINNED.file_name));
        let explicit = GeoSource::resolve(Some("/x/geo-sample.tsv".into()), data_dir);
        assert_eq!(explicit, GeoSource::Explicit("/x/geo-sample.tsv".into()));
        assert_eq!(
            explicit.dataset_id().as_deref(),
            Some("custom:geo-sample.tsv")
        );
        assert_eq!(GeoSource::Disabled.dataset_id(), None);
    }

    #[test]
    fn a_missing_pinned_file_is_not_an_error_but_a_missing_explicit_one_is() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            GeoSource::resolve(None, dir.path())
                .load()
                .unwrap()
                .is_none()
        );
        assert!(GeoSource::Disabled.load().unwrap().is_none());
        let missing = GeoSource::Explicit(dir.path().join("nope.tsv"));
        assert!(matches!(missing.load(), Err(GeoDataError::Io { .. })));
        let present = GeoSource::Explicit(write(dir.path(), "s.tsv", SAMPLE));
        assert_eq!(present.load().unwrap().unwrap().len(), 1);
    }

    #[test]
    fn a_corrupt_pinned_file_fails_its_checksum() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(DATA_SUBDIR)).unwrap();
        write(
            &dir.path().join(DATA_SUBDIR),
            PINNED.file_name,
            "not the snapshot\n",
        );
        let err = GeoSource::resolve(None, dir.path()).load().err().unwrap();
        assert!(matches!(err, GeoDataError::Checksum { .. }), "{err}");
    }

    #[test]
    fn the_pin_is_a_real_sha256() {
        assert_eq!(PINNED.sha256.len(), 64);
        assert!(
            PINNED
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(PINNED.sha256.bytes().any(|b| b != b'0'));
        assert!(PINNED.file_name.starts_with("geonames-cities500-"));
        assert!(PINNED.file_name.ends_with(".tsv"));
    }

    /// The committed snapshot and the pin cannot drift apart.
    #[test]
    fn the_committed_snapshot_matches_the_pin() {
        let gz = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/geodata")
            .join(format!("{}.gz", PINNED.file_name));
        assert!(gz.exists(), "missing {}", gz.display());
        let output = std::process::Command::new("gzip")
            .arg("-dc")
            .arg(&gz)
            .output()
            .expect("running gzip");
        assert!(output.status.success());
        assert_eq!(sha256_hex(&output.stdout), PINNED.sha256);
        let index = GeoIndex::load(output.stdout.as_slice()).unwrap();
        assert!(index.len() > 100_000, "{}", index.len());
        assert_eq!(index.nearest(48.85341, 2.3488).unwrap().name, "Paris");
    }
}
