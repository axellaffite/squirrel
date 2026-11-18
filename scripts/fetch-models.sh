#!/usr/bin/env bash
# Downloads the pinned ML models (SigLIP2 ViT-B/16 and YuNet + SFace) from
# revision-pinned URLs, checks each file's SHA-256 before moving it into place,
# and keeps each upstream licence next to its models. The worker never
# downloads anything; it re-checks these hashes before loading a model
# (crates/squirrel-ml/src/models.rs holds the same list; a test keeps them equal).
#
# Usage: scripts/fetch-models.sh [--without-faces] [--clip-only-image] [dest]
# dest defaults to .cache/squirrel-assets/models (gitignored, outside target/); prints SQUIRREL_MODELS_DIR=<dest> on success.
# Files already present with the right hash are kept, so a re-run is cheap.
set -euo pipefail

CLIP="https://huggingface.co/immich-app/ViT-B-16-SigLIP2__webli/resolve/5f3fc29104ebcf51e9d893af57604badd51a3ea6"
CLIP_LICENSE="https://raw.githubusercontent.com/google-research/big_vision/0127fb6b337ee2a27bf4e54dea79cff176527356/LICENSE"
ZOO_COMMIT="eb9f4f2d4b79b58e5513b5d25d5f0243a8357bf4"
ZOO_LFS="https://media.githubusercontent.com/media/opencv/opencv_zoo/${ZOO_COMMIT}/models"
ZOO_RAW="https://raw.githubusercontent.com/opencv/opencv_zoo/${ZOO_COMMIT}/models"
MARKER=".squirrel-models"

# group | path under dest | source URL | SHA-256
FILES="
clip|siglip2/config.json|${CLIP}/config.json|a33af509278a65855fd6b4fd8a17ed1b1e3482ff5d438c3fc43b07d4999e01b5
clip|siglip2/LICENSE|${CLIP_LICENSE}|43070e2d4e532684de521b885f385d0841030efa2b1a20bafb76133a5e1379c1
clip-image|siglip2/visual/model.onnx|${CLIP}/visual/model.onnx|fee10c729875dd203d94f396ac3d664e301d39c977ef6d92e7f467a2a716f0b0
clip-image|siglip2/visual/preprocess_cfg.json|${CLIP}/visual/preprocess_cfg.json|d78a80d49dca4e62d17f802a685d67d821f7102844cb89f65a0bb1a5d0601f43
clip-text|siglip2/textual/model.onnx|${CLIP}/textual/model.onnx|fc9991b415d7d0fac7ed540e875fd08a647c8a1abba99f12542531b004c4b68f
clip-text|siglip2/textual/tokenizer.json|${CLIP}/textual/tokenizer.json|220c63d496e0c14e63eb656c91e0215e926202e4c74b1f089e09f1920d779b04
clip-text|siglip2/textual/tokenizer_config.json|${CLIP}/textual/tokenizer_config.json|958146aa289a0a39e4fcb20087a78facb157d29c7ef017374ac3ccab0549aae4
clip-text|siglip2/textual/special_tokens_map.json|${CLIP}/textual/special_tokens_map.json|db82f8bd9b25d14f9c788e6bde64de84d42f1c2538f1c245ba6cb3e872d14b18
faces|faces/face_detection_yunet_2026may.onnx|${ZOO_LFS}/face_detection_yunet/face_detection_yunet_2026may.onnx|ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0
faces|faces/LICENSE-yunet|${ZOO_RAW}/face_detection_yunet/LICENSE|c83b8120c50ccbd4c4f96edf53141bdd566ebb8f8e9227e415326aa1b1aba958
faces|faces/face_recognition_sface_2021dec.onnx|${ZOO_LFS}/face_recognition_sface/face_recognition_sface_2021dec.onnx|0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79
faces|faces/LICENSE-sface|${ZOO_RAW}/face_recognition_sface/LICENSE|cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30
"

WITH_FACES=1
WITH_TEXT=1
DEST=""
for arg in "$@"; do
	case "${arg}" in
		--without-faces) WITH_FACES=0 ;;
		--clip-only-image) WITH_TEXT=0 ;;
		-*) echo "fetch-models: unknown option ${arg}" >&2; exit 2 ;;
		*) [[ -z "${DEST}" ]] || { echo "fetch-models: one destination only" >&2; exit 2; }; DEST="${arg}" ;;
	esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${DEST:-${ROOT}/.cache/squirrel-assets/models}"
if [[ -e "${DEST}" && ! -f "${DEST}/${MARKER}" ]] && [[ -n "$(ls -A "${DEST}")" ]]; then
	echo "fetch-models: ${DEST} is not empty and was not made by this script" >&2
	exit 1
fi
mkdir -p "${DEST}"
touch "${DEST}/${MARKER}"
DEST="$(cd "${DEST}" && pwd)"

# On the destination's file system, so the final move is a rename.
TMP="$(mktemp -d "${DEST}/.fetch.XXXXXX")"
trap 'rm -rf "${TMP}"' EXIT

matches() { echo "$2  $1" | shasum -a 256 -c - >/dev/null 2>&1; }

while IFS='|' read -r group path url sha; do
	[[ -n "${group}" ]] || continue
	[[ "${group}" == faces && "${WITH_FACES}" == 0 ]] && continue
	[[ "${group}" == clip-text && "${WITH_TEXT}" == 0 ]] && continue
	target="${DEST}/${path}"
	if [[ -f "${target}" ]] && matches "${target}" "${sha}"; then
		continue
	fi
	echo "fetch-models: ${path}" >&2
	download="${TMP}/download"
	curl --proto '=https' --tlsv1.2 -fsSL \
		--connect-timeout 30 --retry 3 --retry-all-errors --max-time 1800 -o "${download}" "${url}"
	if ! matches "${download}" "${sha}"; then
		echo "fetch-models: ${path} does not match its pinned SHA-256 ${sha}" >&2
		exit 1
	fi
	mkdir -p "$(dirname "${target}")"
	mv -f "${download}" "${target}"
done <<< "${FILES}"

echo "SQUIRREL_MODELS_DIR=${DEST}"
