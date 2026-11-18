#!/usr/bin/env bash
# Downloads the pinned ML models (CLIP ViT-B/32 and YuNet + SFace) from
# revision-pinned URLs, checks each file's SHA-256 before moving it into place,
# and keeps each upstream licence next to its models. The worker never
# downloads anything; it re-checks these hashes before loading a model
# (crates/squirrel-ml/src/models.rs holds the same list; a test keeps them equal).
#
# Usage: scripts/fetch-models.sh [--without-faces] [--clip-only-image] [dest]
# dest defaults to target/models; prints SQUIRREL_MODELS_DIR=<dest> on success.
# Files already present with the right hash are kept, so a re-run is cheap.
set -euo pipefail

CLIP="https://huggingface.co/immich-app/ViT-B-32__openai/resolve/a857c8de2c07bbcfa6646adfcf31b798845afa1e"
CLIP_LICENSE="https://raw.githubusercontent.com/openai/CLIP/d05afc436d78f1c48dc0dbf8e5980a9d471f35f6/LICENSE"
ZOO_COMMIT="eb9f4f2d4b79b58e5513b5d25d5f0243a8357bf4"
ZOO_LFS="https://media.githubusercontent.com/media/opencv/opencv_zoo/${ZOO_COMMIT}/models"
ZOO_RAW="https://raw.githubusercontent.com/opencv/opencv_zoo/${ZOO_COMMIT}/models"
MARKER=".squirrel-models"

# group | path under dest | source URL | SHA-256
FILES="
clip|clip/config.json|${CLIP}/config.json|59a182a6dce0adeb303f3d72d73758d68ba29516d928dd92740465f498290409
clip|clip/LICENSE|${CLIP_LICENSE}|987e63b32f6c89ff5160e429458a872ff048e6860b590a3912e938f9da8f14db
clip-image|clip/visual/model.onnx|${CLIP}/visual/model.onnx|33a3df41ceef21acdf371af00f6dd0456ec1f9eba24d03a7720f9c3734e40859
clip-image|clip/visual/preprocess_cfg.json|${CLIP}/visual/preprocess_cfg.json|c4fd008cd177c58a8fe107b099bac98913eaa6be6114dd59a4b45145c3c7673f
clip-text|clip/textual/model.onnx|${CLIP}/textual/model.onnx|b80cf0af751533a6712d92247f0ddc0c95208748bc59a1a27f33e67be6864e3b
clip-text|clip/textual/tokenizer.json|${CLIP}/textual/tokenizer.json|6d9109cc838977f3ca94a379eec36aecc7c807e1785cd729660ca2fc0171fb35
clip-text|clip/textual/tokenizer_config.json|${CLIP}/textual/tokenizer_config.json|39bae5313c2da6467866dee758e754d69f0e302a875027b142531cb9c51c7c28
clip-text|clip/textual/special_tokens_map.json|${CLIP}/textual/special_tokens_map.json|2cdb3b8331a60c92fc1e55a13e9fd61fd2293c5a51275fdcccd62b780052530e
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
DEST="${DEST:-${ROOT}/target/models}"
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
	curl --proto '=https' --tlsv1.2 -fsSL -o "${download}" "${url}"
	if ! matches "${download}" "${sha}"; then
		echo "fetch-models: ${path} does not match its pinned SHA-256 ${sha}" >&2
		exit 1
	fi
	mkdir -p "$(dirname "${target}")"
	mv -f "${download}" "${target}"
done <<< "${FILES}"

echo "SQUIRREL_MODELS_DIR=${DEST}"
