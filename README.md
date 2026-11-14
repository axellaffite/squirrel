# Squirrel

Self-hosted home drive: per-user storage, photo classification, documents, video streaming and phone sync.
Design: `docs/plans/2026-11-13-squirrel-home-drive-design.md`.

## Development

Requirements: Rust 1.94 (pinned by `rust-toolchain.toml`), Node 22 (`web/.nvmrc`), pnpm 10, Docker.

    docker compose -f deploy/docker-compose.dev.yml up -d --wait   # Postgres on localhost:55432
    SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
    SQUIRREL_DATA_DIR="$PWD/target/dev-data" \
    SQUIRREL_COOKIE_SECURE=false cargo run -p squirrel -- serve     # http://localhost:8080
    cd web && nvm use && pnpm install && pnpm dev                    # http://localhost:5173

With nvm, `nvm use` in `web/` picks Node 22 from `.nvmrc` (or run `nvm use 22`).
The Vite dev server proxies `/api` to the Rust server on port 8080.

On first start, the server logs a one-time setup code (`first-run setup code: ...`).
Open the web UI, enter that code, and create the administrator account.
To choose the code yourself (e.g. for automated deployments), set `SQUIRREL_SETUP_TOKEN`
to a long random value (the server warns below 16 characters).
Everyone else joins with an invitation link.

Logs are sensitive: the first-start log contains the setup code, and with `RUST_LOG=debug`
request logs may include invitation tokens (they appear in `/api/join/{token}` paths).

### Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SQUIRREL_DATABASE_URL` | (required) | Postgres connection string (also read by `squirrel migrate` and `squirrel admin`) |
| `SQUIRREL_DATA_DIR` | (required) | Where file contents are stored (`blobs/`, `tmp/`). Created if missing; `tmp/` and `blobs/` must be on the same filesystem |
| `SQUIRREL_BIND` | `0.0.0.0:8080` | Listen address |
| `SQUIRREL_COOKIE_SECURE` | `true` | Set to `false` only for local HTTP development |
| `SQUIRREL_SETUP_TOKEN` | generated and logged | Setup code for the first administrator |
| `SQUIRREL_TRUSTED_PROXIES` | empty | Comma-separated IPs/CIDRs of reverse proxies whose `X-Forwarded-For` is trusted for rate limiting and logs. Leave empty when clients connect directly |
| `SQUIRREL_LOG_JSON` | `false` | JSON logs |
| `RUST_LOG` | `info,squirrel=debug,tower_http=info` | Log filter ([`tracing` `EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax) |

Each variable also exists as a `serve` flag (`--data-dir`, `--bind`, …; see `squirrel serve --help`).
Upload, quota and trash behaviour is not configurable yet; the built-in values are:

| Setting | Value |
| --- | --- |
| Upload chunk size | 8 MiB |
| Unfinished uploads expire after | 24 h without activity |
| Free disk space kept for the system | 1 GiB (uploads that would go below it are refused) |
| Trash retention | 30 days, then deleted for good |
| Unreferenced file contents removed after | 24 h |
| Storage quota | unlimited per user unless set with `squirrel admin set-quota` |
| Space one user's open uploads may reserve | half the usable free space (their only open upload may use all of it) |

These become configurable in Phase 7.

### Deployment model

Squirrel assumes **one server process per database and data directory**. Upload locks,
upload admission and rate limits live in that process's memory; two servers sharing a
database or a data directory would not see each other's locks.

### Data directory

    <SQUIRREL_DATA_DIR>/
      .squirrel-instance     id of the installation (database) this directory belongs to
      blobs/ab/cd/<sha256>   file contents, stored once per distinct content (read-only)
      tmp/uploads/<id>       uploads in progress (expire after 24 h without activity)

The database holds names, folders and who owns what; the blobs hold the bytes. Back up
both together (Phase 7 documents `pg_dump` + blob rsync). Files deleted from the trash
(or after 30 days in it) are removed from `blobs/` a day later, once nothing references them.
Uploads are refused while less than 1 GiB would remain free on that disk.

The data directory is bound to its database: each database gets an installation id
(created by its migrations), written to `.squirrel-instance` the first time the server
starts on an empty data directory. The server refuses to start when the marker names
another installation, or when the directory holds files but no marker, because the
maintenance job would otherwise treat every file the database does not know as an orphan
and delete it. Restore the database and the data directory as a pair.

A data directory from before markers existed (or restored without its marker) is adopted
once, deliberately, with the database it belongs to:

    squirrel migrate                         # SQUIRREL_DATABASE_URL set as for serve
    squirrel admin adopt-data-dir            # SQUIRREL_DATABASE_URL and SQUIRREL_DATA_DIR set

It prints how many stored files the database knows. It refuses a directory bound to another
installation, and one whose files the database knows none of (pass `--force` if you are sure).
As a second safety net, the orphan sweep skips deleting when the catalogue is empty or more
than 20 files making up over 10% of those examined look orphaned, and it first runs 6 hours
after the server starts.

### Administration

    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice 10737418240   # 10 GiB
    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice unlimited

`admin adopt-data-dir` is described under "Data directory" above.

Quotas are in bytes and count live files (not the trash). Users see their usage on the Files page.
`squirrel admin` never migrates: it refuses to run while migrations are pending
(run `squirrel migrate`, or start the server, first).

## Tests

    cargo test --workspace                 # needs the dev Postgres (localhost:55432)
    cd web && pnpm test:unit
    cd web && pnpm exec playwright install chromium && pnpm test:e2e

`pnpm test:e2e` runs `scripts/e2e-server.sh`, which recreates the `squirrel_e2e` database
in the dev Postgres and the `target/e2e-data` data directory, builds the web UI and starts
the server on `127.0.0.1:8099` (set `SQUIRREL_E2E_PORT` to use another port). In CI,
`SQUIRREL_E2E_DATABASE_URL` points it at the CI Postgres instead of docker compose, and
`SQUIRREL_E2E_DATA_DIR` moves the data directory (its last component must be `e2e-data`,
since the script deletes it).

The Playwright projects run in order: `onboarding` creates the accounts that `files`
(desktop width) and `files-phone` (phone width, row actions in the actions sheet) log in with.
The quota test calls `target/debug/squirrel admin set-quota` on the E2E database.

PDF rendering itself is checked by hand (headless Chromium has no PDF viewer): open a PDF
preview in Chrome, Firefox and Safari, reopen it, download it, and check the console for
CSP errors. Resuming a large upload is also checked by hand: start a >1 GB upload, kill the
tab midway, upload the same file to the same folder again and check it resumes from the
chunks already sent.

### Troubleshooting

- `migration ... was previously applied but has been modified`: a committed migration changed
  under an existing dev database. Reset it with `docker compose -f deploy/docker-compose.dev.yml down -v`
  (this destroys local dev data), then start it again. Never edit a migration once merged: add a new one.

## API contract

`openapi.json` is generated from the Rust code and committed. After changing an endpoint:

    cargo run -q -p squirrel -- openapi > openapi.json
    cd web && pnpm gen:api
