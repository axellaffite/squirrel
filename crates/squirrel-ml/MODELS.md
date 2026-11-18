# Pinned models

`scripts/fetch-models.sh [--without-faces] [--clip-only-image] [dest]` downloads these files from
revision-pinned URLs into `dest` (default `.cache/squirrel-assets/models`, gitignored; in production `<data_dir>/models` or
`SQUIRREL_MODELS_DIR`) and checks each SHA-256 before moving it into place. The worker and the
`ml-exec` child never download anything. They re-hash every file of a group before loading it and
refuse a mismatch. `src/models.rs` (`PINNED`) holds the same list. A test keeps the script and the
manifest equal.

## CLIP (`openai-vit-b32-224`)

- **Source:** `huggingface.co/immich-app/ViT-B-32__openai` at revision
  `a857c8de2c07bbcfa6646adfcf31b798845afa1e`, an OpenCLIP→ONNX export of OpenAI's CLIP ViT-B/32.
  The fp32 ONNX files only: `fp16/`, `rknpu/` and `*.armnn` are skipped. The LFS `oid` of each
  model file is its SHA-256. The small files were hashed after download.
- **Licence:** MIT. OpenAI's `LICENSE` ("Copyright (c) 2021 OpenAI") from `github.com/openai/CLIP`
  at commit `d05afc436d78f1c48dc0dbf8e5980a9d471f35f6` is kept as `clip/LICENSE`. The export repo
  declares no licence of its own.
- **Use:** English only. OpenAI's model card says its use "should be limited to English language
  use cases".

| File | Group | Size (B) | SHA-256 |
|---|---|---|---|
| `clip/config.json` | clip-image, clip-text | 196 | `59a182a6dce0adeb303f3d72d73758d68ba29516d928dd92740465f498290409` |
| `clip/LICENSE` | clip-image, clip-text | 1 064 | `987e63b32f6c89ff5160e429458a872ff048e6860b590a3912e938f9da8f14db` |
| `clip/visual/model.onnx` | clip-image | 351 613 724 | `33a3df41ceef21acdf371af00f6dd0456ec1f9eba24d03a7720f9c3734e40859` |
| `clip/visual/preprocess_cfg.json` | clip-image | 197 | `c4fd008cd177c58a8fe107b099bac98913eaa6be6114dd59a4b45145c3c7673f` |
| `clip/textual/model.onnx` | clip-text | 254 193 396 | `b80cf0af751533a6712d92247f0ddc0c95208748bc59a1a27f33e67be6864e3b` |
| `clip/textual/tokenizer.json` | clip-text | 3 642 073 | `6d9109cc838977f3ca94a379eec36aecc7c807e1785cd729660ca2fc0171fb35` |
| `clip/textual/tokenizer_config.json` | clip-text | 736 | `39bae5313c2da6467866dee758e754d69f0e302a875027b142531cb9c51c7c28` |
| `clip/textual/special_tokens_map.json` | clip-text | 588 | `2cdb3b8331a60c92fc1e55a13e9fd61fd2293c5a51275fdcccd62b780052530e` |

## Faces (`yunet-sface-2021dec`)

- **Source:** `github.com/opencv/opencv_zoo` at commit `eb9f4f2d4b79b58e5513b5d25d5f0243a8357bf4`
  (models from `media.githubusercontent.com`, the Git LFS store, so the pointer's `oid` is the
  SHA-256; licences from `raw.githubusercontent.com`). The Hugging Face mirror
  `opencv/face_detection_yunet` only has the 2023mar detector, so GitHub is the pinned source for
  both files.
- **Licences:** YuNet is MIT (`faces/LICENSE-yunet`). SFace is Apache-2.0
  (`faces/LICENSE-sface`).
- **Never InsightFace:** its models are for non-commercial research only.

| File | Size (B) | SHA-256 |
|---|---|---|
| `faces/face_detection_yunet_2026may.onnx` | 229 738 | `ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0` |
| `faces/LICENSE-yunet` | 1 085 | `c83b8120c50ccbd4c4f96edf53141bdd566ebb8f8e9227e415326aa1b1aba958` |
| `faces/face_recognition_sface_2021dec.onnx` | 38 696 353 | `0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79` |
| `faces/LICENSE-sface` | 11 358 | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |
