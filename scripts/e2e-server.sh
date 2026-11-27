#!/usr/bin/env bash
# Starts a fresh Squirrel instance on 127.0.0.1:8099 (override with SQUIRREL_E2E_PORT)
# against an empty squirrel_e2e database.
#
# Locally the database runs in deploy/docker-compose.dev.yml (port 55432). When
# SQUIRREL_E2E_DATABASE_URL is set (CI), compose is skipped and that database is
# dropped and recreated with psql through the server's `postgres` maintenance database.
#
# Files go to a data directory that is wiped and recreated on every run:
# target/e2e-data by default, or SQUIRREL_E2E_DATA_DIR (its last component must be
# e2e-data, so a typo can never wipe anything else).
set -euo pipefail
cd "$(dirname "$0")/.."

E2E_DB=squirrel_e2e
DEFAULT_URL="postgres://squirrel:squirrel@localhost:55432/${E2E_DB}"

if [[ -n "${SQUIRREL_E2E_DATABASE_URL:-}" ]]; then
	DATABASE_URL="${SQUIRREL_E2E_DATABASE_URL}"
	db_name="${DATABASE_URL##*/}"
	if [[ "${db_name}" != "${E2E_DB}" ]]; then
		echo "e2e-server: refusing to recreate '${db_name}'; the database must be named ${E2E_DB}" >&2
		exit 1
	fi
	admin_url="${DATABASE_URL%/*}/postgres"
	psql "${admin_url}" -v ON_ERROR_STOP=1 -q -c "DROP DATABASE IF EXISTS ${E2E_DB} WITH (FORCE)"
	psql "${admin_url}" -v ON_ERROR_STOP=1 -q -c "CREATE DATABASE ${E2E_DB}"
else
	DATABASE_URL="${DEFAULT_URL}"
	COMPOSE=(docker compose -f deploy/docker-compose.dev.yml)
	"${COMPOSE[@]}" up -d --wait postgres
	"${COMPOSE[@]}" exec -T postgres dropdb -U squirrel --if-exists --force "${E2E_DB}"
	"${COMPOSE[@]}" exec -T postgres createdb -U squirrel "${E2E_DB}"
fi

node_major() { if command -v node >/dev/null; then node -p 'process.versions.node.split(".")[0]'; else echo 0; fi; }
# Switch to nvm's Node 22 only when the node on PATH is older: CI runners ship nvm
# without Node 22 (setup-node puts it on PATH), and `nvm use 22` fails there.
if (( $(node_major) < 22 )) && [[ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
	set +u
	# shellcheck disable=SC1091
	source "${NVM_DIR:-$HOME/.nvm}/nvm.sh"
	nvm use 22 >/dev/null
	set -u
fi
node_major="$(node_major)"
if (( node_major < 22 )); then
	echo "e2e-server: Node 22 or newer is required, found $(node --version)" >&2
	exit 1
fi

# A fresh data directory per run. Only a directory named e2e-data is ever wiped.
DATA_DIR="${SQUIRREL_E2E_DATA_DIR:-${PWD}/target/e2e-data}"
case "${DATA_DIR}" in
	*/e2e-data) ;;
	*)
		echo "e2e-server: refusing to wipe '${DATA_DIR}'; the directory must be named e2e-data" >&2
		exit 1
		;;
esac
rm -rf "${DATA_DIR}"
mkdir -p "${DATA_DIR}"

(cd web && pnpm build)

# In debug builds rust-embed reads web/build from disk, so building the web first is enough.
export SQUIRREL_DATABASE_URL="${DATABASE_URL}"
export SQUIRREL_BIND="127.0.0.1:${SQUIRREL_E2E_PORT:-8099}"
export SQUIRREL_COOKIE_SECURE=false
# Public links on the next port, as Tailscale Funnel would expose them.
PUBLIC_PORT=$(( ${SQUIRREL_E2E_PORT:-8099} + 1 ))
export SQUIRREL_PUBLIC_BIND="127.0.0.1:${PUBLIC_PORT}"
export SQUIRREL_PUBLIC_URL="http://127.0.0.1:${PUBLIC_PORT}"
export SQUIRREL_SETUP_TOKEN=e2e-setup-token
export SQUIRREL_DATA_DIR="${DATA_DIR}"
# Media processing runs in the server, with the tiny offline geodata sample (Paris, Lyon, …).
# SQUIRREL_MEDIA_EXEC stays the default (subprocess): E2E exercises the decoding sandbox.
export SQUIRREL_EMBEDDED_WORKER=true
# Two slots whatever the core count: on a 2-core runner the default is one, and the
# label bank (minutes there) then holds it while every thumbnail waits.
export SQUIRREL_WORKER_CONCURRENCY="${SQUIRREL_WORKER_CONCURRENCY:-2}"
export SQUIRREL_GEODATA_FILE="${PWD}/crates/squirrel-media/tests/fixtures/geo-sample.tsv"
# PDF thumbnails and text when pdfium was fetched (scripts/fetch-pdfium.sh); otherwise PDFs are unsupported.
# Machine learning when ONNX Runtime and the models are installed (scripts/fetch-onnxruntime.sh,
# scripts/fetch-models.sh): the ML scenarios skip without them (and fail on CI, where they are).
ASSETS="${PWD}/.cache/squirrel-assets"
if [[ -z "${SQUIRREL_MODELS_DIR:-}" && -d "${ASSETS}/models" ]]; then
	export SQUIRREL_MODELS_DIR="${ASSETS}/models"
fi
if [[ -z "${SQUIRREL_ORT_DYLIB:-}" ]]; then
	for lib in "${ASSETS}/onnxruntime/lib/libonnxruntime.so" "${ASSETS}/onnxruntime/lib/libonnxruntime.dylib"; do
		if [[ -f "${lib}" ]]; then export SQUIRREL_ORT_DYLIB="${lib}"; fi
	done
fi
# People appear a second after their faces, not ten minutes.
export SQUIRREL_CLUSTER_DELAY_SECS=1
export SQUIRREL_DUPLICATES_DELAY_SECS=1
if [[ -z "${SQUIRREL_PDFIUM_DIR:-}" && -d "${PWD}/target/pdfium/lib" ]]; then
	export SQUIRREL_PDFIUM_DIR="${PWD}/target/pdfium/lib"
fi
exec cargo run -q -p squirrel -- serve
