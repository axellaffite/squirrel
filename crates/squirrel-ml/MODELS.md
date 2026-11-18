# Pinned models

`scripts/fetch-models.sh [--without-faces] [--clip-only-image] [dest]` downloads these files from
revision-pinned URLs into `dest` (default `.cache/squirrel-assets/models`, gitignored; in production `<data_dir>/models` or
`SQUIRREL_MODELS_DIR`) and checks each SHA-256 before moving it into place. The worker and the
`ml-exec` child never download anything. They re-hash every file of a group before loading it and
refuse a mismatch. `src/models.rs` (`PINNED`) holds the same list. A test keeps the script and the
manifest equal.

## SigLIP2 (`siglip2-vit-b16-224`)

Image–text model for tags and photo search (the code keeps the generic "clip" names).

- **Source:** `huggingface.co/immich-app/ViT-B-16-SigLIP2__webli` at revision
  `5f3fc29104ebcf51e9d893af57604badd51a3ea6`, an OpenCLIP→ONNX export of Google's SigLIP2
  ViT-B/16 (224 px, WebLI, 768-d). The fp32 ONNX files only: `fp16/`, `rknpu/` and `*.armnn` are
  skipped. The LFS `oid` of each model file is its SHA-256. The small files were hashed after
  download.
- **Licence:** Apache-2.0. Google's `LICENSE` from `github.com/google-research/big_vision` at
  commit `0127fb6b337ee2a27bf4e54dea79cff176527356` is kept as `siglip2/LICENSE`; the model cards
  of `google/siglip2-base-patch16-224` and `timm/ViT-B-16-SigLIP2` declare Apache-2.0. The export
  repo declares no licence of its own.
- **Model card caveats (verbatim):** the export's README says "This repo is specifically intended
  for use with Immich, a self-hosted photo library." Google's card says "Intended uses: You can
  use the raw model for tasks like zero-shot image classification and image-text retrieval, or as
  a vision encoder for VLMs (and other vision tasks)." No card has a limitations or
  out-of-scope section.
- **Use:** multilingual (trained on WebLI, Gemma tokenizer). Text is cleaned like open_clip's
  `canonicalize`, tokenized to 64 ids ending in `<eos>` and padded with `<pad>`. Images are
  squashed to 224 × 224 (bicubic), mean = std = 0.5.
- **Not in the export:** the learned `logit_scale` (4.724453449249268) and `logit_bias`
  (−16.771724700927734), read from the F32 header-addressed tensors of `open_clip_model.safetensors`
  in `timm/ViT-B-16-SigLIP2` at `eee10eff6dd8cabae2d7f379d4e8cfcd352030aa`. They turn a cosine into
  SigLIP's match probability, `σ(e^scale · cos + bias)` (`src/clip/mod.rs`).

| File | Group | Size (B) | SHA-256 |
|---|---|---|---|
| `siglip2/config.json` | clip-image, clip-text | 547 | `a33af509278a65855fd6b4fd8a17ed1b1e3482ff5d438c3fc43b07d4999e01b5` |
| `siglip2/LICENSE` | clip-image, clip-text | 11 356 | `43070e2d4e532684de521b885f385d0841030efa2b1a20bafb76133a5e1379c1` |
| `siglip2/visual/model.onnx` | clip-image | 371 674 292 | `fee10c729875dd203d94f396ac3d664e301d39c977ef6d92e7f467a2a716f0b0` |
| `siglip2/visual/preprocess_cfg.json` | clip-image | 154 | `d78a80d49dca4e62d17f802a685d67d821f7102844cb89f65a0bb1a5d0601f43` |
| `siglip2/textual/model.onnx` | clip-text | 1 129 435 819 | `fc9991b415d7d0fac7ed540e875fd08a647c8a1abba99f12542531b004c4b68f` |
| `siglip2/textual/tokenizer.json` | clip-text | 34 362 885 | `220c63d496e0c14e63eb656c91e0215e926202e4c74b1f089e09f1920d779b04` |
| `siglip2/textual/tokenizer_config.json` | clip-text | 46 386 | `958146aa289a0a39e4fcb20087a78facb157d29c7ef017374ac3ccab0549aae4` |
| `siglip2/textual/special_tokens_map.json` | clip-text | 555 | `db82f8bd9b25d14f9c788e6bde64de84d42f1c2538f1c245ba6cb3e872d14b18` |

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
