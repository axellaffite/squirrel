//! The pinned model files and their verification. The worker never downloads
//! a model (`scripts/fetch-models.sh` does); it checks every file against the
//! SHA-256 below before loading it, wherever the directory came from.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// OpenAI CLIP ViT-B/32 (`immich-app/ViT-B-32__openai`), 512-d, 224 px.
pub const CLIP_MODEL_ID: &str = "openai-vit-b32-224";
/// YuNet (2026may) + SFace (2021dec), 128-d.
pub const FACE_MODEL_ID: &str = "yunet-sface-2021dec";

/// Files loaded together: a child loads only the groups it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelGroup {
    ClipImage,
    ClipText,
    Faces,
}

impl ModelGroup {
    pub const ALL: [Self; 3] = [Self::ClipImage, Self::ClipText, Self::Faces];

    /// The name used on command lines (`--groups clip-image,faces`).
    pub const fn name(self) -> &'static str {
        match self {
            Self::ClipImage => "clip-image",
            Self::ClipText => "clip-text",
            Self::Faces => "faces",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.name() == name)
    }
}

/// What a file is, beyond its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    Model,
    /// Configuration or tokenizer data read with the model.
    Data,
    /// Kept next to the models, never loaded.
    Licence,
}

#[derive(Debug, Clone, Copy)]
pub struct PinnedFile {
    /// Every group that needs this file.
    pub groups: &'static [ModelGroup],
    /// Relative to the models directory, `/`-separated, never `..`.
    pub path: &'static str,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: &'static str,
    pub role: FileRole,
}

const CLIP_ALL: &[ModelGroup] = &[ModelGroup::ClipImage, ModelGroup::ClipText];
const CLIP_IMAGE: &[ModelGroup] = &[ModelGroup::ClipImage];
const CLIP_TEXT: &[ModelGroup] = &[ModelGroup::ClipText];
const FACES: &[ModelGroup] = &[ModelGroup::Faces];

const fn pin(
    groups: &'static [ModelGroup],
    path: &'static str,
    size: u64,
    sha256: &'static str,
    role: FileRole,
) -> PinnedFile {
    assert!(
        is_safe_relative(path),
        "a pinned path must stay inside the models directory"
    );
    PinnedFile {
        groups,
        path,
        size,
        sha256,
        role,
    }
}

/// Sources, revisions and licences: `crates/squirrel-ml/MODELS.md`.
/// `scripts/fetch-models.sh` carries the same list (a test keeps them equal).
#[rustfmt::skip]
pub static PINNED: &[PinnedFile] = &[
    // immich-app/ViT-B-32__openai @ a857c8de2c07bbcfa6646adfcf31b798845afa1e
    pin(CLIP_ALL, "clip/config.json", 196, "59a182a6dce0adeb303f3d72d73758d68ba29516d928dd92740465f498290409", FileRole::Data),
    pin(CLIP_IMAGE, "clip/visual/model.onnx", 351_613_724, "33a3df41ceef21acdf371af00f6dd0456ec1f9eba24d03a7720f9c3734e40859", FileRole::Model),
    pin(CLIP_IMAGE, "clip/visual/preprocess_cfg.json", 197, "c4fd008cd177c58a8fe107b099bac98913eaa6be6114dd59a4b45145c3c7673f", FileRole::Data),
    pin(CLIP_TEXT, "clip/textual/model.onnx", 254_193_396, "b80cf0af751533a6712d92247f0ddc0c95208748bc59a1a27f33e67be6864e3b", FileRole::Model),
    pin(CLIP_TEXT, "clip/textual/tokenizer.json", 3_642_073, "6d9109cc838977f3ca94a379eec36aecc7c807e1785cd729660ca2fc0171fb35", FileRole::Data),
    pin(CLIP_TEXT, "clip/textual/tokenizer_config.json", 736, "39bae5313c2da6467866dee758e754d69f0e302a875027b142531cb9c51c7c28", FileRole::Data),
    pin(CLIP_TEXT, "clip/textual/special_tokens_map.json", 588, "2cdb3b8331a60c92fc1e55a13e9fd61fd2293c5a51275fdcccd62b780052530e", FileRole::Data),
    // openai/CLIP @ d05afc436d78f1c48dc0dbf8e5980a9d471f35f6 (MIT)
    pin(CLIP_ALL, "clip/LICENSE", 1064, "987e63b32f6c89ff5160e429458a872ff048e6860b590a3912e938f9da8f14db", FileRole::Licence),
    // opencv/opencv_zoo @ eb9f4f2d4b79b58e5513b5d25d5f0243a8357bf4
    pin(FACES, "faces/face_detection_yunet_2026may.onnx", 229_738, "ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0", FileRole::Model),
    pin(FACES, "faces/LICENSE-yunet", 1085, "c83b8120c50ccbd4c4f96edf53141bdd566ebb8f8e9227e415326aa1b1aba958", FileRole::Licence),
    pin(FACES, "faces/face_recognition_sface_2021dec.onnx", 38_696_353, "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79", FileRole::Model),
    pin(FACES, "faces/LICENSE-sface", 11_358, "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30", FileRole::Licence),
];

/// No absolute path, no `..`, no empty or `.` component, no backslash.
pub const fn is_safe_relative(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.is_empty() || bytes[0] == b'/' {
        return false;
    }
    let mut start = 0;
    let mut i = 0;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'/' {
            let len = i - start;
            let dot = len == 1 && bytes[start] == b'.';
            let dotdot = len == 2 && bytes[start] == b'.' && bytes[start + 1] == b'.';
            if len == 0 || dot || dotdot {
                return false;
            }
            start = i + 1;
        } else if bytes[i] == b'\\' || bytes[i] == 0 {
            return false;
        }
        i += 1;
    }
    true
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model file {path} is missing (run scripts/fetch-models.sh)")]
    Missing { path: PathBuf },
    #[error("model file {path} has {actual} bytes, the pin says {expected}")]
    Size {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    #[error("model file {path} does not match its pin: expected SHA-256 {expected}, got {actual}")]
    Hash {
        path: PathBuf,
        expected: &'static str,
        actual: String,
    },
    #[error("reading model file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Where the models live: `SQUIRREL_MODELS_DIR`, or `<data_dir>/models`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDir(PathBuf);

impl ModelDir {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn resolve(configured: Option<PathBuf>, data_dir: &Path) -> Self {
        Self(configured.unwrap_or_else(|| data_dir.join("models")))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn file(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.0.clone(), |path, part| path.join(part))
    }

    /// Every file of `group` is present with its pinned size (cheap: no hashing).
    pub fn installed(&self, group: ModelGroup) -> bool {
        self.installed_files(group_files(PINNED, group))
    }

    /// Hashes every file of `group` (streamed) and compares it with its pin.
    pub fn verify(&self, group: ModelGroup) -> Result<(), ModelError> {
        self.verify_files(group_files(PINNED, group))
    }

    fn installed_files<'a>(&self, files: impl IntoIterator<Item = &'a PinnedFile>) -> bool {
        files.into_iter().all(|pinned| {
            std::fs::metadata(self.file(pinned.path))
                .is_ok_and(|meta| meta.is_file() && meta.len() == pinned.size)
        })
    }

    /// Reads the pinned file at `relative` into memory and checks its size
    /// and SHA-256 there: what is loaded is exactly what was hashed (no
    /// window for the file to change in between).
    pub fn read_verified(&self, relative: &str) -> Result<Vec<u8>, ModelError> {
        self.read_verified_as(pinned(relative))
    }

    fn read_verified_as(&self, pinned: &PinnedFile) -> Result<Vec<u8>, ModelError> {
        let path = self.file(pinned.path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ModelError::Missing { path });
            }
            Err(source) => return Err(ModelError::Io { path, source }),
        };
        let actual_size = bytes.len() as u64;
        if actual_size != pinned.size {
            return Err(ModelError::Size {
                path,
                expected: pinned.size,
                actual: actual_size,
            });
        }
        let actual = sha256_hex(&mut bytes.as_slice()).map_err(|source| ModelError::Io {
            path: path.clone(),
            source,
        })?;
        if actual != pinned.sha256 {
            return Err(ModelError::Hash {
                path,
                expected: pinned.sha256,
                actual,
            });
        }
        Ok(bytes)
    }

    fn verify_files<'a>(
        &self,
        files: impl IntoIterator<Item = &'a PinnedFile>,
    ) -> Result<(), ModelError> {
        files
            .into_iter()
            .try_for_each(|pinned| self.verify_file(pinned))
    }

    fn verify_file(&self, pinned: &PinnedFile) -> Result<(), ModelError> {
        let path = self.file(pinned.path);
        let io = |source| ModelError::Io {
            path: path.clone(),
            source,
        };
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ModelError::Missing { path });
            }
            Err(err) => return Err(io(err)),
        };
        let actual_size = file.metadata().map_err(io)?.len();
        if actual_size != pinned.size {
            return Err(ModelError::Size {
                path,
                expected: pinned.size,
                actual: actual_size,
            });
        }
        let actual = sha256_hex(&mut file).map_err(io)?;
        if actual != pinned.sha256 {
            return Err(ModelError::Hash {
                path,
                expected: pinned.sha256,
                actual,
            });
        }
        Ok(())
    }
}

fn group_files(files: &[PinnedFile], group: ModelGroup) -> impl Iterator<Item = &PinnedFile> {
    files
        .iter()
        .filter(move |file| file.groups.contains(&group))
}

/// The pinned file of `group` at `path` (panics on a typo: paths are constants).
pub fn pinned(path: &str) -> &'static PinnedFile {
    PINNED
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("{path} is not a pinned model file"))
}

pub(crate) fn sha256_hex(reader: &mut impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const HELLO_SHA: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    static TEST_FILES: &[PinnedFile] = &[pin(
        &[ModelGroup::Faces],
        "faces/a.onnx",
        5,
        HELLO_SHA,
        FileRole::Model,
    )];

    fn dir_with(content: &[u8]) -> (tempfile::TempDir, ModelDir) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("faces")).unwrap();
        std::fs::write(tmp.path().join("faces/a.onnx"), content).unwrap();
        let dir = ModelDir::new(tmp.path());
        (tmp, dir)
    }

    #[test]
    fn a_missing_file_or_a_wrong_size_is_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ModelDir::new(tmp.path());
        assert!(!dir.installed_files(TEST_FILES));
        assert!(!dir.installed(ModelGroup::Faces));
        let (_tmp, dir) = dir_with(b"hello!");
        assert!(!dir.installed_files(TEST_FILES));
        let (_tmp, dir) = dir_with(b"hello");
        assert!(dir.installed_files(TEST_FILES));
    }

    #[test]
    fn verify_accepts_the_pinned_bytes() {
        let (_tmp, dir) = dir_with(b"hello");
        dir.verify_files(TEST_FILES).unwrap();
    }

    #[test]
    fn verify_names_the_file_and_both_hashes_on_mismatch() {
        let (_tmp, dir) = dir_with(b"jello");
        let err = dir.verify_files(TEST_FILES).unwrap_err().to_string();
        assert!(err.contains("a.onnx"), "{err}");
        assert!(err.contains(HELLO_SHA), "{err}");
        let actual = sha256_hex(&mut &b"jello"[..]).unwrap();
        assert!(err.contains(&actual), "{err}");
    }

    #[test]
    fn read_verified_returns_the_hashed_bytes_or_refuses_them() {
        let (_tmp, dir) = dir_with(b"hello");
        assert_eq!(dir.read_verified_as(&TEST_FILES[0]).unwrap(), b"hello");
        let (_tmp, dir) = dir_with(b"jello");
        let err = dir.read_verified_as(&TEST_FILES[0]).unwrap_err();
        assert!(matches!(err, ModelError::Hash { .. }), "{err}");
        let tmp = tempfile::tempdir().unwrap();
        let err = ModelDir::new(tmp.path())
            .read_verified_as(&TEST_FILES[0])
            .unwrap_err();
        assert!(matches!(err, ModelError::Missing { .. }), "{err}");
    }

    #[test]
    fn verify_reports_a_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let err = ModelDir::new(tmp.path())
            .verify_files(TEST_FILES)
            .unwrap_err();
        assert!(matches!(err, ModelError::Missing { .. }), "{err}");
        assert!(err.to_string().contains("fetch-models"), "{err}");
    }

    #[test]
    fn pinned_paths_cannot_escape_the_directory() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "./a",
            "a/",
            "a\\b",
        ] {
            assert!(!is_safe_relative(bad), "{bad:?}");
        }
        for good in ["a", "clip/visual/model.onnx", "faces/LICENSE-yunet", "a..b"] {
            assert!(is_safe_relative(good), "{good:?}");
        }
        assert!(PINNED.iter().all(|file| is_safe_relative(file.path)));
    }

    #[test]
    #[should_panic(expected = "inside the models directory")]
    fn pinning_an_escaping_path_panics() {
        pin(CLIP_ALL, "../x", 1, HELLO_SHA, FileRole::Data);
    }

    #[test]
    fn the_manifest_is_well_formed() {
        let paths: HashSet<_> = PINNED.iter().map(|file| file.path).collect();
        assert_eq!(paths.len(), PINNED.len(), "duplicate paths");
        for file in PINNED {
            assert_eq!(file.sha256.len(), 64, "{}", file.path);
            assert!(
                file.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{}",
                file.path
            );
            assert!(!file.groups.is_empty(), "{}", file.path);
        }
        for group in ModelGroup::ALL {
            let roles: Vec<_> = group_files(PINNED, group).map(|f| f.role).collect();
            assert!(
                roles.contains(&FileRole::Licence),
                "{group:?} has no licence"
            );
            assert!(roles.contains(&FileRole::Model), "{group:?} has no model");
        }
    }

    #[test]
    fn group_names_round_trip() {
        for group in ModelGroup::ALL {
            assert_eq!(ModelGroup::parse(group.name()), Some(group));
        }
        assert_eq!(ModelGroup::parse("clip"), None);
    }

    #[test]
    fn resolve_prefers_the_configured_directory() {
        let data = Path::new("/data");
        assert_eq!(
            ModelDir::resolve(Some("/m".into()), data).path(),
            Path::new("/m")
        );
        assert_eq!(
            ModelDir::resolve(None, data).path(),
            Path::new("/data/models")
        );
        assert_eq!(
            ModelDir::new("/m").file("clip/visual/model.onnx"),
            Path::new("/m/clip/visual/model.onnx")
        );
    }
}

#[cfg(test)]
mod pinned_files {
    use super::*;

    /// `scripts/fetch-models.sh` downloads exactly the files pinned here.
    #[test]
    fn the_fetch_script_pins_the_same_files() {
        let script = include_str!("../../../scripts/fetch-models.sh");
        let mut listed: Vec<(&str, &str)> = script
            .lines()
            .filter(|line| !line.starts_with('#') && line.matches('|').count() == 3)
            .map(|line| {
                let fields: Vec<&str> = line.split('|').collect();
                (fields[1], fields[3])
            })
            .collect();
        let mut pinned: Vec<(&str, &str)> =
            PINNED.iter().map(|file| (file.path, file.sha256)).collect();
        listed.sort_unstable();
        pinned.sort_unstable();
        assert_eq!(listed, pinned);
    }

    /// With the real models (`scripts/fetch-models.sh`), every group verifies.
    #[test]
    fn the_installed_models_verify() {
        let Some(dir) = crate::testutil::test_models() else {
            return;
        };
        for group in ModelGroup::ALL {
            dir.verify(group)
                .unwrap_or_else(|err| panic!("{}: {err}", group.name()));
        }
    }
}
