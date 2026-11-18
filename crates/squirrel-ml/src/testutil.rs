//! Locating the native pieces model-dependent tests need. Unset, a test is
//! skipped (`None`), except on CI (`CI` set), where skipping would hide a
//! broken setup: it panics. Same contract as `squirrel_media::testutil::test_pdfium_dir`.

use std::path::PathBuf;

use crate::models::{ModelDir, ModelGroup};

/// The pinned ONNX Runtime library (`scripts/fetch-onnxruntime.sh`): `SQUIRREL_ORT_DYLIB`.
pub fn test_ort() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SQUIRREL_ORT_DYLIB") {
        return Some(path.into());
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "CI is set but SQUIRREL_ORT_DYLIB is not: run scripts/fetch-onnxruntime.sh"
    );
    eprintln!("skipped: SQUIRREL_ORT_DYLIB is not set");
    None
}

/// The pinned models (`scripts/fetch-models.sh`): `SQUIRREL_MODELS_DIR`, with
/// every group installed.
pub fn test_models() -> Option<ModelDir> {
    let Some(path) = std::env::var_os("SQUIRREL_MODELS_DIR") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI is set but SQUIRREL_MODELS_DIR is not: run scripts/fetch-models.sh"
        );
        eprintln!("skipped: SQUIRREL_MODELS_DIR is not set");
        return None;
    };
    let dir = ModelDir::new(path);
    for group in ModelGroup::ALL {
        assert!(
            dir.installed(group),
            "SQUIRREL_MODELS_DIR {} lacks the {} models: run scripts/fetch-models.sh",
            dir.path().display(),
            group.name()
        );
    }
    Some(dir)
}

/// Both the runtime and the models, or `None` (see the module docs).
pub fn test_ml() -> Option<(PathBuf, ModelDir)> {
    Some((test_ort()?, test_models()?))
}
