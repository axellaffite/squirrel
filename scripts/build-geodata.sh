#!/usr/bin/env bash
# Rebuilds the GeoNames cities500 snapshot from GeoNames' public dumps.
#
#   scripts/build-geodata.sh <out_dir>
#
# Downloads cities500.zip, admin1CodesASCII.txt and countryInfo.txt into a
# fresh temporary directory, joins them with `squirrel admin geodata build`,
# then writes <out_dir>/geonames-cities500-<YYYYMMDD>.tsv and its `.gz`
# (gzip -9n: no name or timestamp, so the .gz is reproducible from the TSV).
# Set SQUIRREL_BIN to use a built binary instead of `cargo run`.
set -euo pipefail

if [[ $# -ne 1 || -z "$1" ]]; then
	echo "usage: $0 <out_dir>" >&2
	exit 2
fi
out_dir="$1"
mkdir -p "${out_dir}"
out_dir="$(cd "${out_dir}" && pwd)"
cd "$(dirname "$0")/.."

base="https://download.geonames.org/export/dump"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

for file in cities500.zip admin1CodesASCII.txt countryInfo.txt; do
	curl --fail --silent --show-error --location --proto '=https' \
		--max-filesize 100000000 -o "${work}/${file}" "${base}/${file}"
done
unzip -q -o "${work}/cities500.zip" cities500.txt -d "${work}"

date_dashed="$(date -u +%Y-%m-%d)"
name="geonames-cities500-$(date -u +%Y%m%d).tsv"
tsv="${out_dir}/${name}"
rm -f "${tsv}" "${tsv}.gz"

if [[ -n "${SQUIRREL_BIN:-}" ]]; then
	squirrel=("${SQUIRREL_BIN}")
else
	squirrel=(cargo run -q -p squirrel --)
fi
"${squirrel[@]}" admin geodata build \
	--cities "${work}/cities500.txt" \
	--admin1 "${work}/admin1CodesASCII.txt" \
	--countries "${work}/countryInfo.txt" \
	--date "${date_dashed}" \
	--out "${tsv}"

gzip -9n -k "${tsv}"
shasum -a 256 "${tsv}" "${tsv}.gz"
ls -l "${tsv}" "${tsv}.gz"
