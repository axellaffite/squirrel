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
| `SQUIRREL_DATABASE_URL` | (required) | Postgres 17 with pgvector 0.8 (see below). Also read by `squirrel migrate` and `squirrel admin` |
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
      derived/ab/cd/<sha256>/{thumb,preview}-v<N>.webp   thumbnails, per distinct content
      geo/                   the GeoNames dataset (scripts/install-geodata.sh)

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

**Postgres** needs the pgvector extension (`deploy/docker-compose.dev.yml` pins the
`pgvector/pgvector:0.8.7-pg17` image by digest). pgvector is not a trusted extension: the
role that migrates must be a superuser (the compose role is), or a superuser must run
`CREATE EXTENSION vector;` in the database first. The server refuses to migrate otherwise.

## Media processing

Thumbnails, photo metadata (date, camera, place) and document text are computed in the
background by a job worker, from a queue kept in Postgres. Run it as its own process (the
default for production, so a decoder failure can never take the web server down):

    squirrel worker                       # same SQUIRREL_DATABASE_URL and SQUIRREL_DATA_DIR as serve

or inside the server for development and small installs: `squirrel serve --worker`
(`SQUIRREL_EMBEDDED_WORKER=true`). The worker never migrates: it waits for the server (or
`squirrel migrate`) to apply its migrations. Files stored before this existed are processed
by a backfill job queued at every worker start.

| Variable | Default | Purpose |
| --- | --- | --- |
| `SQUIRREL_WORKER_CONCURRENCY` | half the cores, at most 2 | Jobs run at once |
| `SQUIRREL_WORKER_KINDS` | every kind | Comma-separated job kinds this worker runs, e.g. `clip_embed,faces,cluster_faces` on a separate ML machine. Unknown names stop the worker |
| `SQUIRREL_WORKER_EXCLUDE_KINDS` | none | Comma-separated job kinds this worker leaves to others. Not with `SQUIRREL_WORKER_KINDS` |
| `SQUIRREL_ML_CONCURRENCY` | `1` | Model inference jobs (`clip_embed`, `faces`) run at once, so another slot stays free for fresh uploads |
| `SQUIRREL_MODELS_DIR` | `<data_dir>/models` | Pinned ML models (`scripts/fetch-models.sh`). Checked against their SHA-256 at start: a mismatch stops the worker. Without models, photo tags, search and faces are off |
| `SQUIRREL_ORT_DYLIB` | unset | The pinned ONNX Runtime library (`scripts/fetch-onnxruntime.sh`), required once models are installed |
| `SQUIRREL_ML_THREADS` | half the cores | Threads for one inference |
| `SQUIRREL_ML_EXEC` | `subprocess` | `subprocess` runs inference in a long-lived `squirrel ml-exec` child (resource limits, restarted on crash, stopped after 10 idle minutes); `in-process` is for tests only |
| `SQUIRREL_MEDIA_EXEC` | `subprocess` | `subprocess` decodes untrusted files in short-lived `squirrel media-exec` children with memory, CPU and file limits; `in-process` is for tests only |
| `SQUIRREL_PDFIUM_DIR` | unset | Directory holding the pdfium library. Without it, PDFs get no thumbnail or text |
| `SQUIRREL_GEODATA_FILE` | `<data_dir>/geo/…` | Another place-name file (tests, custom extracts); used without the SHA-256 pin |

**Models** (photo tags, text search and faces) are fetched once, from revision-pinned URLs,
and checked against their SHA-256 (about 640 MB; sources and licences in
`crates/squirrel-ml/MODELS.md`):

    scripts/fetch-models.sh               # prints SQUIRREL_MODELS_DIR=…/target/models

**pdfium** (PDF thumbnails and text) is downloaded, checked against its pinned SHA-256 and
unpacked into `target/pdfium/` by:

    scripts/fetch-pdfium.sh               # prints SQUIRREL_PDFIUM_DIR=…/target/pdfium/lib

It is Google's PDF engine (BSD-3-Clause / Apache-2.0); its `LICENSE` and the third-party
notices in `licenses/` stay next to the library and ship with it.

**Places** come from an offline GeoNames snapshot committed in `data/geodata/` (no request
leaves the server). Install it into a data directory with `scripts/install-geodata.sh <data_dir>`;
until then photos keep their coordinates and get their place once it is installed. Place names:
GeoNames (CC BY 4.0), credited in the photo viewer. `data/geodata/README.md` explains how to
rebuild the snapshot (`scripts/build-geodata.sh <out_dir>`, then update the pin).

**HEIC/HEIF** photos are decoded only in builds with `--features heic`, which need the system
libheif ≥ 1.17 with its libde265 HEVC plugin (`apt install libheif-dev libheif-plugin-libde265`,
`brew install libheif`). Other builds still read their date and place, and show them with
an "unsupported preview" tile.

**Document search** looks at file names, PDFs and text files: words of three letters or more
match as prefixes, two-letter words only as whole words, one-letter words are ignored. Each
search runs at most 5 s (then answers 503) and is rate limited per client (20 at once, then one
per second).

Administration:

    squirrel admin jobs                   # queue counts per kind, and the latest failures
    squirrel admin jobs retry <id>        # run a failed job again
    squirrel admin media backfill         # queue processing for anything not processed yet

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
`photos`, `photos-phone` and `documents` run as fresh users invited by alice, with the worker
inside the E2E server and the media in `web/e2e/fixtures/` (generated, see its README). With
pdfium in `target/pdfium/lib` (or `SQUIRREL_PDFIUM_DIR`) the PDF part of `documents` runs too;
otherwise it is skipped.

PDF rendering itself is checked by hand (headless Chromium has no PDF viewer): open a PDF
preview in Chrome, Firefox and Safari, reopen it, download it, and check the console for
CSP errors. Resuming a large upload is also checked by hand: start a >1 GB upload, kill the
tab midway, upload the same file to the same folder again and check it resumes from the
chunks already sent.

### Troubleshooting

- `migration ... was previously applied but has been modified`: a committed migration changed
  under an existing dev database. Reset it with `docker compose -f deploy/docker-compose.dev.yml down -v`
  (this destroys local dev data), then start it again. Never edit a migration once merged: add a new one.

## Phone app

The Android and iOS app lives in `mobile/` (Kotlin Multiplatform, one Compose UI for both); see
`mobile/README.md` for the JDK 21 and Android SDK setup, installing the debug APK (USB or
wireless `adb`), running on the iOS simulator, signing for your own iPhone and the end-to-end
server script. In short, from `mobile/`:

    ./gradlew build                                              # Android and shared code
    ./gradlew :androidApp:installDebug                           # onto a connected phone
    ./gradlew -Psquirrel.ios=true :shared:iosSimulatorArm64Test  # iOS targets (macOS only)

A phone signs in with a username and password and gets a device token; Settings › Devices on
the web lists the phones and revokes them. Plain HTTP is accepted only for home and VPN
addresses; anything else needs HTTPS.

## API contract

`openapi.json` is generated from the Rust code and committed. After changing an endpoint:

    cargo run -q -p squirrel -- openapi > openapi.json
    cd web && pnpm gen:api
    cd mobile && ./gradlew :shared:checkApiClient   # the app's client is generated at build time
