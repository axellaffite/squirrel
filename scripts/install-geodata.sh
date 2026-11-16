#!/usr/bin/env bash
# Installs the pinned GeoNames snapshot into a data directory, offline:
#
#   scripts/install-geodata.sh <data_dir>
#
# Decompresses data/geodata/<name>.gz into <data_dir>/geo/<name>, checks its
# SHA-256 against the pin compiled into squirrel (`squirrel admin geodata
# pinned`) and makes it read-only. The worker verifies it again on load.
# Set SQUIRREL_BIN to use a built binary instead of `cargo run`.
set -euo pipefail

if [[ $# -ne 1 || -z "$1" ]]; then
	echo "usage: $0 <data_dir>" >&2
	exit 2
fi
mkdir -p "$1/geo"
geo="$(cd "$1/geo" && pwd)"
cd "$(dirname "$0")/.."

if [[ -n "${SQUIRREL_BIN:-}" ]]; then
	squirrel=("${SQUIRREL_BIN}")
else
	squirrel=(cargo run -q -p squirrel --)
fi
line="$("${squirrel[@]}" admin geodata pinned)"
hash="${line%%  *}"
name="${line#*  }"
if ! [[ "${hash}" =~ ^[0-9a-f]{64}$ && "${name}" =~ ^geonames-cities500-[0-9]{8}\.tsv$ ]]; then
	echo "unexpected pin: ${line}" >&2
	exit 1
fi
source_gz="data/geodata/${name}.gz"
if [[ ! -f "${source_gz}" ]]; then
	echo "missing ${source_gz}" >&2
	exit 1
fi

tmp="$(mktemp "${geo}/.${name}.XXXXXX")"
trap 'rm -f "${tmp}"' EXIT
gzip -dc "${source_gz}" > "${tmp}"
echo "${hash}  ${tmp}" | shasum -a 256 -c --status -
chmod 0400 "${tmp}"
mv -f "${tmp}" "${geo}/${name}"
trap - EXIT
echo "installed ${geo}/${name}"
