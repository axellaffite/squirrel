#!/bin/sh
# Container entrypoint: installs the GeoNames snapshot shipped in the image into the data
# directory when it is missing (the worker verifies its SHA-256 on load), then runs squirrel.
set -eu

data="${SQUIRREL_DATA_DIR:-/data}"
for src in /opt/squirrel/geo/*; do
	[ -f "$src" ] || continue
	dest="$data/geo/$(basename "$src")"
	if [ ! -e "$dest" ]; then
		mkdir -p "$data/geo"
		tmp="$(mktemp "$data/geo/.install.XXXXXX")"
		cp "$src" "$tmp"
		chmod 0400 "$tmp"
		mv -f "$tmp" "$dest"
	fi
done

exec /usr/local/bin/squirrel "$@"
