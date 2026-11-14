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

# Use Node 22 from nvm when available; otherwise (CI) rely on the node on PATH.
if [[ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
	set +u
	# shellcheck disable=SC1091
	source "${NVM_DIR:-$HOME/.nvm}/nvm.sh"
	nvm use 22 >/dev/null
	set -u
fi
node_major="$(node -p 'process.versions.node.split(".")[0]')"
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
export SQUIRREL_SETUP_TOKEN=e2e-setup-token
export SQUIRREL_DATA_DIR="${DATA_DIR}"
exec cargo run -q -p squirrel -- serve
