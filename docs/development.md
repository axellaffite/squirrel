# Development

Running Squirrel from source, its tests, the phone app and the API contract. Settings and
operations: [`configuration.md`](configuration.md). Installing on a home server:
[`deploy/README.md`](../deploy/README.md).

## Running locally

Requirements: Rust 1.94 (pinned by `rust-toolchain.toml`), Node 22 (`web/.nvmrc`), pnpm 10, Docker.

    docker compose -f deploy/docker-compose.dev.yml up -d --wait   # Postgres on localhost:55432
    docker compose -f deploy/docker-compose.dev.yml exec postgres createdb -U squirrel squirrel_dev  # once
    SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel_dev \
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
request logs may include invitation and public link tokens (they appear in `/api/join/{token}`
and `/api/public/{token}` paths).

## Tests

    cargo test --workspace                 # needs the dev Postgres (localhost:55432)

`.cargo/config.toml` points `DATABASE_URL` at `squirrel_dev`: the database tests use it as the
master for the temporary databases they create (set `DATABASE_URL` to use another one). Keep
`squirrel_dev` for development only, never for data you care about.
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

## Troubleshooting

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
