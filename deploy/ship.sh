#!/usr/bin/env bash
# Builds the image for the server's architecture, copies it over SSH (no registry), updates
# the compose file and restarts on the new tag. The previous tags stay on the server, so a
# rollback is `SQUIRREL_TAG=<old>` in ~/squirrel/.env and `docker compose up -d`.
#
#   deploy/ship.sh <ssh-host> [platform]      # platform defaults to linux/amd64 (the NUC)
#
# First time only: create ~/squirrel/.env on the server from deploy/env.example.
set -euo pipefail
cd "$(dirname "$0")/.."

host="${1:?usage: deploy/ship.sh <ssh-host> [platform]}"
platform="${2:-linux/amd64}"
if [[ -n "$(git status --porcelain)" ]]; then
	echo "ship: the working tree has uncommitted changes; commit them first" >&2
	exit 1
fi
tag="$(git rev-parse --short=12 HEAD)"

docker buildx build --platform "${platform}" -t "squirrel:${tag}" --load .
echo "ship: copying squirrel:${tag} to ${host}"
docker save "squirrel:${tag}" | gzip -1 | ssh "${host}" 'gunzip | docker load'

ssh "${host}" 'mkdir -p ~/squirrel && test -f ~/squirrel/.env' || {
	echo "ship: create ~/squirrel/.env on ${host} first (deploy/env.example)" >&2
	exit 1
}
scp -q deploy/docker-compose.yml "${host}:squirrel/docker-compose.yml"
ssh "${host}" "cd ~/squirrel \
	&& docker tag squirrel:${tag} squirrel:latest \
	&& sed -i 's/^SQUIRREL_TAG=.*/SQUIRREL_TAG=${tag}/' .env \
	&& docker compose up -d --remove-orphans \
	&& docker compose ps"
echo "ship: ${host} runs squirrel:${tag}"
