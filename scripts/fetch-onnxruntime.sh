#!/usr/bin/env bash
# Installs the pinned ONNX Runtime 1.31.1 (CPU) library that `ort` 2.0.0-rc.15
# loads at run time (load-dynamic), from Microsoft's official NuGet package
# Microsoft.ML.OnnxRuntime 1.31.1. The package is checked against its pinned
# SHA-256 and SHA-512 (the SHA-512 nuget.org publishes); only this platform's
# library, LICENSE and ThirdPartyNotices.txt are extracted, and the library is
# checked against its own pin, which the runtime checks again before loading it.
#
# Alternative source with the same library: the GitHub release
# github.com/microsoft/onnxruntime/releases/tag/v1.31.1
# (onnxruntime-{linux-x64,linux-aarch64,osx-arm64}-1.31.1.tgz); it must yield a
# library with the SHA-256 below.
#
# Usage: scripts/fetch-onnxruntime.sh [dest]   (default target/onnxruntime)
# The package is cached in target/onnxruntime-cache/ (or $SQUIRREL_ORT_CACHE).
# Prints SQUIRREL_ORT_DYLIB=<dest>/lib/<library> on success.
set -euo pipefail

VERSION="1.31.1"
NUPKG="microsoft.ml.onnxruntime.${VERSION}.nupkg"
URL="https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/${VERSION}/${NUPKG}"
NUPKG_SHA256=6871ccbc97f3d5bdb4095a1dcec33aa78d56f6ed8de5980ef5fa1d9f2e7c679a
NUPKG_SHA512=42f13cf89ca111c71f38bececf59be7125d919877840aeff06db1da026a7d1e685daaaa039254357f1fb84de43e492467ac279bc83420cb0d57472e73b4c6327
MARKER=".squirrel-onnxruntime"
case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) RID=linux-x64; LIB=libonnxruntime.so; LIB_SHA=a96f132e5eb5ca04e23a7d2fc1078fb95aa591e8d71d0f237702ece3a1e0f4ea ;;
	Linux-aarch64) RID=linux-arm64; LIB=libonnxruntime.so; LIB_SHA=7e17c55d675be73f67a218dbecb55522395f61dafb3da03ebc7ade56a4c2f813 ;;
	Darwin-arm64) RID=osx-arm64; LIB=libonnxruntime.dylib; LIB_SHA=2685dc296147c47b0dbd8e0615547b12fc0c445d158d1f374cbd0705d9626fdc ;;
	*) echo "fetch-onnxruntime: no pinned ONNX Runtime for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:-${ROOT}/target/onnxruntime}"
CACHE="${SQUIRREL_ORT_CACHE:-${ROOT}/target/onnxruntime-cache}"
if [[ -e "${DEST}" && ! -f "${DEST}/${MARKER}" ]] && [[ -n "$(ls -A "${DEST}")" ]]; then
	echo "fetch-onnxruntime: ${DEST} is not empty and was not made by this script" >&2
	exit 1
fi

package_ok() {
	echo "${NUPKG_SHA256}  $1" | shasum -a 256 -c - >/dev/null 2>&1 &&
		echo "${NUPKG_SHA512}  $1" | shasum -a 512 -c - >/dev/null 2>&1
}

mkdir -p "${CACHE}"
PACKAGE="${CACHE}/${NUPKG}"
if [[ ! -f "${PACKAGE}" ]] || ! package_ok "${PACKAGE}"; then
	rm -f "${PACKAGE}"
	curl --proto '=https' --tlsv1.2 -fsSL -o "${PACKAGE}.part" "${URL}"
	if ! package_ok "${PACKAGE}.part"; then
		rm -f "${PACKAGE}.part"
		echo "fetch-onnxruntime: ${NUPKG} does not match its pinned SHA-256/SHA-512" >&2
		exit 1
	fi
	mv "${PACKAGE}.part" "${PACKAGE}"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
unzip -q -o "${PACKAGE}" "runtimes/${RID}/native/${LIB}" LICENSE ThirdPartyNotices.txt -d "${TMP}/x"
mkdir -p "${TMP}/unpacked/lib"
mv "${TMP}/x/runtimes/${RID}/native/${LIB}" "${TMP}/unpacked/lib/${LIB}"
mv "${TMP}/x/LICENSE" "${TMP}/x/ThirdPartyNotices.txt" "${TMP}/unpacked/"
if ! echo "${LIB_SHA}  ${TMP}/unpacked/lib/${LIB}" | shasum -a 256 -c - >/dev/null; then
	echo "fetch-onnxruntime: ${LIB} does not match its pinned SHA-256" >&2
	exit 1
fi
echo "${VERSION} ${RID} ${NUPKG_SHA256} ${LIB_SHA}" > "${TMP}/unpacked/${MARKER}"

rm -rf "${DEST}"
mkdir -p "$(dirname "${DEST}")"
mv "${TMP}/unpacked" "${DEST}"
echo "SQUIRREL_ORT_DYLIB=${DEST}/lib/${LIB}"
