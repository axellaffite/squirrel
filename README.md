# Squirrel

Self-hosted home drive: per-user storage, photo classification, documents, video streaming and phone sync.
Design: `docs/plans/2026-11-13-squirrel-home-drive-design.md`.

## Development

Requirements: Rust 1.94 (pinned by `rust-toolchain.toml`), Node 22 (`web/.nvmrc`), pnpm 10, Docker.

    docker compose -f deploy/docker-compose.dev.yml up -d --wait   # Postgres on localhost:55432
    SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
    SQUIRREL_COOKIE_SECURE=false cargo run -p squirrel -- serve     # http://localhost:8080
    cd web && nvm use && pnpm install && pnpm dev                    # http://localhost:5173

With nvm, `nvm use` in `web/` picks Node 22 from `.nvmrc` (or run `nvm use 22`).
The Vite dev server proxies `/api` to the Rust server on port 8080.

On first start, the server logs a one-time setup code (`first-run setup code: ...`).
Open the web UI, enter that code, and create the administrator account.
To choose the code yourself (e.g. for automated deployments), set `SQUIRREL_SETUP_TOKEN`.
Everyone else joins with an invitation link.

### Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SQUIRREL_DATABASE_URL` | (required) | Postgres connection string |
| `SQUIRREL_BIND` | `0.0.0.0:8080` | Listen address |
| `SQUIRREL_COOKIE_SECURE` | `true` | Set to `false` only for local HTTP development |
| `SQUIRREL_SETUP_TOKEN` | generated and logged | Setup code for the first administrator |
| `SQUIRREL_TRUSTED_PROXIES` | empty | Comma-separated IPs/CIDRs of reverse proxies whose `X-Forwarded-For` is trusted for rate limiting and logs. Leave empty when clients connect directly |
| `SQUIRREL_LOG_JSON` | `false` | JSON logs |

## Tests

    cargo test --workspace                 # needs the dev Postgres (localhost:55432)
    cd web && pnpm test:unit
    cd web && pnpm exec playwright install chromium && pnpm test:e2e

`pnpm test:e2e` runs `scripts/e2e-server.sh`, which recreates the `squirrel_e2e` database
in the dev Postgres, builds the web UI and starts the server on `127.0.0.1:8099`
(set `SQUIRREL_E2E_PORT` to use another port). In CI, `SQUIRREL_E2E_DATABASE_URL`
points it at the CI Postgres instead of docker compose.

## API contract

`openapi.json` is generated from the Rust code and committed. After changing an endpoint:

    cargo run -q -p squirrel -- openapi > openapi.json
    cd web && pnpm gen:api
