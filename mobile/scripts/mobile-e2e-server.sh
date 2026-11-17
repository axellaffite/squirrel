#!/usr/bin/env bash
# A throwaway Squirrel for the phone's end-to-end checks: its own database on the dev Postgres
# (localhost:55432, never 5433), its own data directory, the embedded worker, one user.
#
#   mobile/scripts/mobile-e2e-server.sh start          # prints the address and credentials
#   mobile/scripts/mobile-e2e-server.sh check <name>   # taken_source of the asset of file <name>
#   mobile/scripts/mobile-e2e-server.sh sql '<query>'  # any read on the e2e database
#   mobile/scripts/mobile-e2e-server.sh stop           # stops the server, drops everything
#
# The emulator reaches it with `adb reverse tcp:$PORT tcp:$PORT` and http://localhost:$PORT
# (an emulator proxy cannot reach 10.0.2.2).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${SQUIRREL_MOBILE_E2E_PORT:-8188}"
DB="${SQUIRREL_MOBILE_E2E_DB:-squirrel_mobile_e2e}"
STATE="${SQUIRREL_MOBILE_E2E_STATE:-${TMPDIR:-/tmp}/squirrel-mobile-e2e}"
USER_NAME="phone"
PASSWORD="phone-e2e-password"
SETUP_TOKEN="mobile-e2e-setup-token"

case "$DB" in
	squirrel_mobile_e2e*) ;;
	*) echo "refusing database '$DB': the e2e database must start with squirrel_mobile_e2e" >&2; exit 2 ;;
esac

pg() {
	local container
	container="$(docker compose -f "$ROOT/deploy/docker-compose.dev.yml" ps -q postgres)"
	[[ -n "$container" ]] || { echo "dev Postgres is not running: docker compose -f deploy/docker-compose.dev.yml up -d --wait" >&2; exit 1; }
	docker exec -i "$container" "$@"
}

start() {
	pg dropdb -U squirrel --if-exists "$DB"
	pg createdb -U squirrel "$DB"
	rm -rf "$STATE" && mkdir -p "$STATE/data"
	(cd "$ROOT" && cargo build -q -p squirrel)
	SQUIRREL_BIN="$ROOT/target/debug/squirrel" "$ROOT/scripts/install-geodata.sh" "$STATE/data" >/dev/null
	SQUIRREL_DATABASE_URL="postgres://squirrel:squirrel@localhost:55432/$DB" \
	SQUIRREL_DATA_DIR="$STATE/data" \
	SQUIRREL_BIND="127.0.0.1:$PORT" \
	SQUIRREL_COOKIE_SECURE=false \
	SQUIRREL_SETUP_TOKEN="$SETUP_TOKEN" \
	SQUIRREL_EMBEDDED_WORKER=true \
		nohup "$ROOT/target/debug/squirrel" serve >"$STATE/server.log" 2>&1 &
	echo $! >"$STATE/pid"
	for _ in $(seq 1 60); do
		curl -fs "http://127.0.0.1:$PORT/api/healthz" >/dev/null && break
		sleep 0.5
	done
	curl -fs -X POST "http://127.0.0.1:$PORT/api/setup/admin" -H 'content-type: application/json' \
		-d "{\"setup_token\":\"$SETUP_TOKEN\",\"username\":\"$USER_NAME\",\"display_name\":\"Phone Tester\",\"password\":\"$PASSWORD\"}" >/dev/null
	echo "server=http://localhost:$PORT username=$USER_NAME password=$PASSWORD log=$STATE/server.log"
}

sql() {
	pg psql -U squirrel -d "$DB" -At -c "$1"
}

check() {
	sql "SELECT n.name || '|' || a.taken_source FROM nodes n JOIN assets a ON a.node_id = n.id WHERE n.name = '$1' AND n.deleted_at IS NULL"
}

stop() {
	if [[ -f "$STATE/pid" ]]; then kill "$(cat "$STATE/pid")" 2>/dev/null || true; sleep 1; fi
	pg dropdb -U squirrel --if-exists "$DB"
	rm -rf "$STATE"
}

case "${1:-}" in
	start) start ;;
	check) check "$2" ;;
	sql) sql "$2" ;;
	stop) stop ;;
	*) echo "usage: $0 start|check <name>|sql <query>|stop" >&2; exit 2 ;;
esac
