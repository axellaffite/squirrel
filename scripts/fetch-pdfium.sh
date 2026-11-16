#!/usr/bin/env bash
# Downloads the pinned pdfium build (bblanchon/pdfium-binaries, chromium/8125,
# the version pdfium-render 0.9.5's `pdfium_latest` feature targets), checks
# its SHA-256 before unpacking anything, and installs it into target/pdfium
# (or $1), which must be empty, missing, or an earlier install of this script.
# Prints the SQUIRREL_PDFIUM_DIR to export:
#
#   eval "export $(scripts/fetch-pdfium.sh | tail -1)"
#
# The archive's LICENSE and licenses/ stay next to the library.
set -euo pipefail

RELEASE="chromium/8125"
MARKER=".squirrel-pdfium"
case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) ASSET=pdfium-linux-x64.tgz; SHA=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659 ;;
	Linux-aarch64) ASSET=pdfium-linux-arm64.tgz; SHA=8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa ;;
	Darwin-arm64) ASSET=pdfium-mac-arm64.tgz; SHA=4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d ;;
	*) echo "fetch-pdfium: no pinned pdfium build for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:-${ROOT}/target/pdfium}"
if [[ -e "${DEST}" && ! -f "${DEST}/${MARKER}" ]] && [[ -n "$(ls -A "${DEST}")" ]]; then
	echo "fetch-pdfium: ${DEST} is not empty and was not made by this script" >&2
	exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
curl --proto '=https' --tlsv1.2 -fsSL -o "${TMP}/${ASSET}" \
	"https://github.com/bblanchon/pdfium-binaries/releases/download/${RELEASE}/${ASSET}"
if ! echo "${SHA}  ${TMP}/${ASSET}" | shasum -a 256 -c - >/dev/null; then
	echo "fetch-pdfium: ${ASSET} does not match its pinned SHA-256" >&2
	exit 1
fi
mkdir "${TMP}/unpacked"
tar -xzf "${TMP}/${ASSET}" --no-same-owner -C "${TMP}/unpacked"
echo "${RELEASE} ${ASSET} ${SHA}" > "${TMP}/unpacked/${MARKER}"

rm -rf "${DEST}"
mkdir -p "$(dirname "${DEST}")"
mv "${TMP}/unpacked" "${DEST}"
echo "SQUIRREL_PDFIUM_DIR=${DEST}/lib"
