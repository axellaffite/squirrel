# Phase 3 — Media Pipeline, Timeline, Albums & Document Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Every image a user stores appears in a Photos timeline (newest first, grouped by day, with a month scrubber), opens in a lightbox with its EXIF details, place name and an offline map locator, and can be gathered into albums; every image and PDF gets a WebP thumbnail with a blurhash placeholder (also shown in the Files and Home lists); PDFs and plain-text files become searchable by content. All of it is computed in the background by a Postgres-backed job queue and a separate `squirrel worker` process, including for files uploaded before Phase 3.

**Architecture:** A new `squirrel-jobs` crate owns the `jobs` table (claimed with `FOR UPDATE SKIP LOCKED`, leases with fencing tokens, retries with exponential backoff, `failed` after 5 attempts, priorities, `LISTEN/NOTIFY` wake-up) and a generic worker runtime. A new `squirrel-media` crate holds pure, I/O-light media code: EXIF (`kamadak-exif`), the `taken_at` fallback chain, screenshot detection, bounded decoding (`image`), resizing (`fast_image_resize`), lossy WebP (`webp`/libwebp), blurhash, PDF rendering and text (`pdfium-render`, pdfium loaded at runtime), optional HEIC (`libheif-rs`, cargo feature `heic`) and an offline GeoNames reverse geocoder. A new `squirrel-worker` crate holds the job handlers (`process_media`, `extract_metadata`, `thumbnails`, `geocode`, `extract_text`, `backfill`) that glue Postgres, the blob store and `squirrel-media`; untrusted decoding runs in short-lived `squirrel media-exec` child processes with rlimits and a wall-clock timeout. Content-derived facts live per blob (`media`, `document_text`; derived files under `<data_dir>/derived/ab/cd/<sha256>/`, so deduplicated content shares them), timeline entries per node (`assets`), albums per user. Upload completion and dedup links enqueue `process_media` in the same transaction that creates the node. The SPA gains `/photos` (virtualized justified grid loaded month by month, scrubber, lightbox, multi-select, albums) and document search, built only from MASTER.md tokens and `web/src/lib/components/ui/`.

**Tech Stack:** Phase 2 stack, plus (all listed again under "Open questions" for approval): `kamadak-exif` 0.6.2, `image` 0.25.10 (jpeg, png, gif, webp, tiff, bmp), `fast_image_resize` 6.1.0, `webp` 0.3.1 (vendored libwebp, built with `cc`), `blurhash` 0.2.3, `pdfium-render` 0.9.5 + a pinned pdfium binary (bblanchon/pdfium-binaries `chromium/8125`), optional `libheif-rs` 3.0.0 + system libheif ≥ 1.17; `serde_json` for job payloads (sqlx `json` feature); `rustix` gains its `process` feature (rlimits). Web: no new npm dependency (justified layout, virtualization and blurhash decoding are hand-rolled and unit-tested).

**Design reference:** `docs/plans/2026-11-13-squirrel-home-drive-design.md` (sections "Media pipeline", "Storage and sync", "Video streaming" for what Phase 4 adds, "Testing", "Delivery phases").
**Phase 2 reference:** `docs/plans/2026-11-13-phase-2-storage-files.md` — read its "Upload hardening" and "Download and maintenance hardening" sections; the code is the source of truth, not that plan's snippets.
**Visual design:** `MASTER.md` governs every screen in this plan. Milestone F starts by recording the few photo-specific additions in MASTER.md (Task 31), which needs the user's validation first (see Open question 8).

## Conventions / environment

- Run commands from the repo root unless stated. Never `cd` outside the worktree; never `git stash` (use a WIP commit).
- **Dev Postgres is `localhost:55432` only** (`deploy/docker-compose.dev.yml`, `DATABASE_URL` in `.cargo/config.toml`). **Never use port 5433.**
- Node/pnpm commands are prefixed with `source ~/.nvm/nvm.sh && nvm use 22 >/dev/null &&` and run in `web/`.
- After each Rust task, before committing:
  `cargo fmt --all && rtk proxy cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- E2E: `cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && SQUIRREL_E2E_PORT=8199 pnpm test:e2e` (8099 is taken on this machine).
- Tests that touch the filesystem use `tempfile::tempdir()`. Media fixtures are **generated in code** (`squirrel_media::testutil`: JPEGs with chosen EXIF, PNGs, minimal PDFs) so no binary of unclear licence is committed; the only committed binaries are the E2E fixtures produced by `cargo run -p squirrel-media --example make_fixtures --features test-util` (Task 42).
- `sqlx` runtime queries (`query_as::<_, T>`) as before; no compile-time macros. SQL whose `WHERE` depends on optional filters is assembled with `sqlx::QueryBuilder` from fixed fragments and `push_bind` (never user input in the SQL text) so keyset conditions stay `Index Cond`s — no `$n IS NULL OR …`.
- Existing migrations are **frozen** (CI refuses edits). Phase 3 adds new files only: `20261114000002_jobs.sql` (A), `20261114000003_media.sql` (B), `20261114000004_albums.sql` (D), `20261114000005_documents.sql` (E). If the calendar moved on, use the current date for the prefix, keeping this order.
- Every API change regenerates the contract in the same task, or `tests/openapi.rs` fails:
  `cargo run -q -p squirrel -- openapi > openapi.json && (cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm gen:api)`
- Commit messages follow the repo pattern `<type>(<scope>): <description>` (scopes: `core`, `db`, `storage`, `api`, `jobs`, `media`, `worker`, `web`, …), short body optional, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Keep files under ~400 lines; split modules rather than growing one.
- **Optional native pieces in dev:**
  - pdfium: `scripts/fetch-pdfium.sh` (Task 28) downloads the pinned build into `target/pdfium/` and prints `SQUIRREL_PDFIUM_DIR=…`. Without it, PDFs get `thumbs_state = 'unsupported'` and PDF tests skip with a message (never fail).
  - HEIC: `brew install libheif` (macOS) or `apt install libheif-dev libheif-plugin-libde265` (Debian trixie / Ubuntu 24.04), then build with `--features heic`. Default builds do not need it.
  - GeoNames: `scripts/fetch-geodata.sh` (Task 20) or `SQUIRREL_GEODATA_FILE=crates/squirrel-media/tests/fixtures/geo-sample.tsv` for a tiny offline sample.
- `libwebp` is compiled from the vendored C sources by `libwebp-sys`: a C compiler must be present (Xcode Command Line Tools on macOS, `build-essential` on Debian; GitHub's `ubuntu-latest` has one).

### What was verified while writing this plan

- **Compiled and tested** (Rust 1.94, a scratch crate, `cargo clippy -D warnings` clean, 23 tests green): `squirrel-media`'s `metadata.rs` (EXIF capture time + offset, GPS with hemispheres, camera, orientation, screenshot comment, against EXIF written with `kamadak-exif`'s `experimental::Writer` and kamadak's own HEIC/PNG/WebP fixtures), `thumbs.rs` (bounded decode, EXIF orientation, `fast_image_resize` Lanczos3, libwebp lossy encode, blurhash), `capture.rs` (file-name dates, the `taken_at` chain, screenshot detection), `geo.rs` (grid index, haversine, antimeridian, 50 km cut-off) and `testutil.rs` (JPEG+EXIF and minimal PDF generators). The code in Tasks 8, 9, 10 and 19 is that code, verbatim.
- **Compiled, not run:** `pdf.rs` against `pdfium-render` 0.9.5 (loading a downloaded native pdfium library was not allowed in the planning sandbox). Its test runs only when `SQUIRREL_PDFIUM_DIR` is set.
- **Applied to Postgres 17 (pgvector image)**: all four new migrations on top of the existing three; the job claim (`FOR UPDATE SKIP LOCKED` CTE), lease reaping, dedupe with `ON CONFLICT … WHERE` on the partial unique index (and that a `failed` job no longer blocks its key); the timeline keyset query plan on 20 000 assets (`Index Scan using assets_timeline_idx`, `Index Cond` on the row comparison, 3 ms); the months aggregate (5 ms); `ON DELETE SET NULL (cover_node_id)`; the `squirrel_text` configuration with `unaccent` (`"ecole elec"` → `'ecole':* & 'elec':*` matches "École Électricité"), `ts_headline` with control-character markers, the name GIN index; and that `CREATE EXTENSION unaccent` works for a **non-superuser** database owner.
- **Run in Node 22:** `blurhash.ts` (decoded mean colour equals the DC colour) and `justified.ts` (rows end exactly at the container width, last row at target height, virtualization window) — Task 32's code, verbatim.
- **Downloaded and hashed:** pdfium `chromium/8125` archives (SHA-256 pinned in Task 28).
- **UNVERIFIED** (check against compiler/docs when implementing): the `squirrel-jobs` runtime and every SQLx/axum glue snippet (they follow Phase 2 patterns and sqlx 0.8 `PgListener` docs but were not compiled); `libheif-rs` integration (needs the system library; API read from the 3.0.0 sources: `integration::image::register_all_decoding_hooks`); whether libheif's decoder already applies the HEIF rotation (`irot`/`imir`) so EXIF orientation must be skipped for HEIF (the plan assumes yes, as libheif documents, and tests it with a generated fixture); Debian package names `libheif-plugin-libde265`/`libheif1` on trixie; the size and row count of the cities500 extract (≈ 200 k places, ≈ 12 MB TSV); `nom-exif`'s licence (crates.io metadata shows none; the repository says MIT); `image-webp` 0.2 being lossless-only for encoding.

## Key decisions (read before starting)

### Job queue (`squirrel-jobs`)

| Topic | Decision | Why |
|---|---|---|
| Storage | One `jobs` table (Task 1): `kind`, `payload jsonb`, `priority smallint` (higher first), `status` ∈ `queued`/`running`/`failed`, `attempts`/`max_attempts` (5), `run_at`, `locked_until` (lease), `claim_id` (fencing token), `claimed_by`, `last_error`, `dedupe_key`. **Successful jobs are deleted**; failed ones are kept 30 days for admins. | Postgres is already there; enqueueing in the same transaction as the node insert makes "file exists ⇔ its processing is queued" atomic — no outbox, no lost jobs. |
| Claiming | `WITH picked AS (SELECT id … WHERE status='queued' AND run_at <= now() AND kind = ANY($kinds) ORDER BY priority DESC, run_at, id LIMIT $free FOR UPDATE SKIP LOCKED) UPDATE … SET status='running', attempts+1, locked_until = now()+lease, claim_id = gen_random_uuid()`. Partial index `jobs_claim_idx (priority DESC, run_at, id) WHERE status='queued'`. | Many workers never block each other; a worker only claims kinds it has handlers for (Phase 5's ML worker can be a separate process). |
| Visibility timeout | Lease 5 min, extended every lease/3 by a heartbeat while the handler runs. Before each claim, `reap_expired` puts `running` jobs with an expired lease back to `queued` (or `failed` if attempts are exhausted), noting "lease expired". | A crashed worker never strands work; a long job is not stolen while alive. |
| Fencing | Complete / retry / fail / heartbeat all match `id AND claim_id AND status='running'`. | A worker that lost its lease (paused VM, network partition) cannot overwrite the new claimant's outcome. |
| Retries | `JobError::Retry` → `run_at = now() + min(30 s · 4^(attempt−1), 1 h) × jitter(0.8–1.2)` (30 s, 2 min, 8 min, 32 min); after attempt 5 → `failed` with `last_error`. `JobError::Permanent` (bad payload, programming error) → `failed` at once. `JobError::Reschedule(d)` (a dependency is missing, e.g. no geodata yet) → back to `queued` after `d` **without** consuming an attempt. Expected domain failures (a corrupt JPEG) are not job errors: the handler records `thumbs_state = 'failed'` and returns `Ok`. | Transient errors heal; bad files do not burn five attempts; the admin list only shows things worth looking at. |
| Last attempt | `JobContext::is_last_attempt()` lets a handler write its terminal state (e.g. `thumbs_state = 'failed'`) before returning the final `Retry`, so the UI never shows "Preparing…" forever. | — |
| Dedupe | Partial unique index on `dedupe_key WHERE status IN ('queued','running')`; `enqueue` uses `ON CONFLICT (dedupe_key) WHERE … DO NOTHING`. Keys: `process_media:<node_id>`, `extract_metadata:<sha256>`, `thumbnails:<sha256>`, `geocode:<sha256>`, `extract_text:<sha256>`, `backfill`. | The same content uploaded twice, or by two users, is processed once; backfill can run repeatedly. |
| Priorities | `priority::INTERACTIVE = 100` (process_media of a fresh upload), `THUMBNAILS = 80`, `METADATA = 70`, `TEXT = 40`, `GEOCODE = 30`, `BACKGROUND_PENALTY = −50` (subtracted for work coming from backfill), `BACKFILL = 0`. Phase 5 ML runs below 0. | Thumbnails of what the user just uploaded come first. |
| Wake-up | Statement-level `AFTER INSERT` trigger → `pg_notify('squirrel_jobs', '')`; the worker `LISTEN`s with `sqlx::postgres::PgListener` and also polls every 10 s (delayed retries, expired leases, missed notifications while reconnecting). | Sub-second latency without hammering the database. |
| Concurrency | `--concurrency` (default `min(available cores, 4) / 2`, at least 1) jobs at once; one claim fills the free slots. Each handler has a timeout below the lease (thumbnails 3 min, PDF 3 min, others 1 min). | The mini PC also serves HTTP. |
| Process | `squirrel worker` (new subcommand, Task 5): connects, **waits for migrations** (polls `pending_migrations` every 5 s, never migrates itself), loads handlers, runs until SIGTERM/Ctrl-C, then stops claiming, waits 30 s for running jobs and **releases** the rest (back to `queued`, attempt not counted). `squirrel serve --worker` (`SQUIRREL_EMBEDDED_WORKER=true`) runs the same loop inside the server for development, E2E and tiny installs. | Separate process by default (design), so a decoder blow-up can never take the web server down; embedded mode keeps dev to one command. |
| Admin visibility | `GET /api/admin/jobs` (counts per kind/status + the 50 most recent failures with `last_error`), `POST /api/admin/jobs/{id}/retry`; CLI `squirrel admin jobs [--failed]` / `squirrel admin jobs retry <id>`. No admin UI in Phase 3 (design lists it under Admin; it lands with the admin pages). | Design: "failed with last error visible to admins". |

### Media processing (`squirrel-media`, `squirrel-worker`)

| Topic | Decision | Why |
|---|---|---|
| EXIF crate | **`kamadak-exif` 0.6.2** (BSD-2-Clause). Reads JPEG, TIFF, HEIF/HEIC/AVIF, PNG, WebP; tiny sync API; ships an EXIF writer we use to generate test fixtures. Rejected **`nom-exif` 3.8.0**: adds MP4/MOV/MKV metadata and async readers, but its 3.x API is newer and moving, crates.io shows no licence field, and Phase 4 reads video metadata with `ffprobe` anyway (design). | Mature, verified (Task 8 code compiled and tested against it). |
| `taken_at` chain | 1. EXIF `DateTimeOriginal` (+ `SubSecTimeOriginal`, + `OffsetTimeOriginal`), else `DateTimeDigitized`, else IFD0 `DateTime`; years < 1900 are "clock not set". 2. A date in the file name (`IMG_20240714_183005`, `PXL_20240714_183005123`, `Screenshot_2024-07-14-18-30-05`, `Screenshot 2024-07-14 at 18.30.05`, `IMG-20240714-WA0001` (noon), `2024-07-14 18.30.05`). 3. (Phase 6) the `taken_at` the phone sends with the upload. 4. The node's `created_at` (upload time, UTC). Any candidate later than now + 1 day is skipped. `assets.taken_source` records which one won. | Phones, WhatsApp and screenshots all covered; the timeline never has a hole. |
| Time model | `assets.taken_at` is a **wall-clock `TIMESTAMP`** (what the camera showed) plus `taken_offset_min` when known. The timeline sorts and groups days/months on the wall clock. | Days must not split at UTC midnight; travel photos stay on the day they were taken. Offsets are often absent in EXIF, so sorting by UTC would be wrong more often than right. The GeoNames timezone column is kept to fill offsets later. |
| Screenshot detection | `kind = 'screenshot'` when the EXIF `UserComment`/`ImageDescription` says "Screenshot" (iOS), or the file name starts with a known screenshot prefix (`Screenshot`, `Screen Shot`, `Capture d’écran`, `Bildschirmfoto`, `Schermafbeelding`, `Captura de pantalla`), or the image is a PNG without camera make/model whose aspect ratio is ≥ 1.9 or ≤ 1/1.9 (phone screens). | Lets the timeline filter screenshots later (Phase 5 filters); cheap and explainable. |
| Decoding | `image` 0.25 (features `jpeg`, `png`, `gif`, `webp`, `tiff`, `bmp`; JPEG via `zune-jpeg`). Dimensions are read from the header first: more than **100 megapixels** is refused before any pixel buffer exists; `Limits::max_alloc` = 1 GiB. EXIF orientation applied with `apply_orientation` (not for HEIF, whose decoder already rotates). Animated GIF/WebP: first frame. | Decompression-bomb safe by construction. |
| Resizing | `fast_image_resize` 6 (SIMD, Lanczos3). | 5–10× faster than `image::imageops` on a 48 MP photo. |
| Variants | `thumb`: height **360 px** (width ≤ 1080) — the grid; `preview`: longest side **1440 px** — the lightbox. Lossy WebP q 72 / q 80. **Deviation from the design's 256 px**: grid rows are 112–208 CSS px tall (Task 31), so 256 px is soft on 2×/3× screens; a 360 px WebP is ~20–35 KB. Files are versioned (`thumb-v1.webp`): bumping `THUMBS_VERSION` + backfill regenerates. | — |
| WebP encoder | **`webp` 0.3.1** → `libwebp-sys` (compiles the vendored libwebp C sources with `cc`, BSD-3-Clause). The pure-Rust `image-webp` encoder is lossless only (UNVERIFIED for 0.2.4), ~5–10× larger files for photos. | Lossy WebP needs libwebp; vendoring avoids a system dependency. |
| Blurhash | `blurhash` 0.2.3, 4×3 components computed on a ≤ 32 px copy; stored in `media.blurhash` (≤ 30 chars). The web decodes it with a 70-line hand-rolled decoder (Task 32). | MASTER allows "a blurred colour preview while loading"; its average colour is available instantly from 4 characters. |
| HEIC/HEIF | **`libheif-rs` 3.0.0 behind the cargo feature `heic`** (crate features `v1_17` + `image`), which registers `image` decoding hooks so the same `decode()` handles HEIC/HEIF (and AVIF when libheif has a decoder). Needs system libheif ≥ 1.17 with an HEVC decoder plugin (libde265), both LGPL-3.0, dynamically linked. Off by default (dev and CI's main job need nothing); the Docker image (Phase 7) builds with it; a separate CI job builds and tests with it (Task 44). **Without the feature**, HEIC files still get EXIF, `taken_at`, GPS and a place (kamadak reads HEIF EXIF) and appear in the timeline with an "unsupported preview" tile. The design's libvips was rejected for Phase 3: a much larger system dependency, and its Rust bindings (`libvips` 2.3, `rs-vips` 0.7) wrap the whole pipeline we already have in safe Rust. | Graceful degradation, no mandatory system library. |
| Isolation | Decoders that parse untrusted bytes in C/C++ (libwebp's decoder is not used; pdfium, libheif) and the image decoders run in a **child process**: `squirrel media-exec` (hidden subcommand) reads one JSON request on stdin, applies `RLIMIT_AS` 2 GiB, `RLIMIT_CPU` 120 s and `RLIMIT_NOFILE` 64 to itself before touching the input, writes outputs into a directory the parent created, prints one JSON response. The parent kills it after a 150 s wall clock (`kill_on_drop`; the handler's own timeout is 180 s, the lease 300 s). EXIF parsing (pure safe Rust, bounded reads) runs in the worker. `SQUIRREL_MEDIA_EXEC=in-process` exists for tests and as an escape hatch. | A crash, hang or exploit in a decoder dies with a short-lived, memory-capped process; the worker just retries. Cost ≈ 10–30 ms per job. |
| File size caps | Images > 512 MiB and PDFs > 512 MiB are `unsupported` (no decode attempted); EXIF is read from at most the first 64 MiB (kamadak reads whole TIFFs into memory). | Bounded memory before any limit fires. |
| Supported MIME | images: `image/jpeg`, `image/png`, `image/gif`, `image/webp`, `image/tiff`, `image/bmp` (+ `image/heic`, `image/heif`, `image/avif` → decoded only with `heic`); documents: `application/pdf`, `text/plain`, `text/markdown`. Videos wait for Phase 4. | Matches `squirrel_storage::mime::detect` (infer + extension). |

### Geocoding, PDFs, derived files

| Topic | Decision | Why |
|---|---|---|
| Dataset | GeoNames **cities500** (places with population ≥ 500; CC BY 4.0) joined with `admin1CodesASCII.txt` and `countryInfo.txt` into one TSV line per place: `lat \t lon \t name \t admin1 \t country code \t country \t timezone`, first lines `#` comments carrying the attribution and source date. Built by `squirrel admin geodata build` (Task 19) from the three GeoNames files. | Offline, private, ~12 MB, parses in ~0.2 s. |
| Pinning | GeoNames republishes daily, so its own files cannot be pinned. The built snapshot (`geonames-cities500-<YYYYMMDD>.tsv`) is published once as a release asset of the Squirrel repository (**Open question 4**) and pinned in `squirrel_media::geo::PINNED` (file name + SHA-256). | A pinned SHA-256 needs a stable file. |
| Fetching | Not by the worker (no HTTP client in Phase 3, **Open question 5**): `scripts/fetch-geodata.sh` (curl + `shasum -a 256 -c`) puts it in `<data_dir>/geo/`; the Phase 7 Docker image bakes it in. The worker loads it lazily on the first `geocode` job and **re-verifies the SHA-256** before use. `SQUIRREL_GEODATA_FILE=<path>` overrides it with an operator-trusted file (tests, E2E, custom extracts) without a pin. Missing dataset → `geocode` reschedules in 1 h (no attempt used); places stay empty; coordinates still show. Backfill enqueues `geocode` for media with GPS whose `geocoded_with` differs from the loaded dataset, so installing the file later fills every place. | — |
| Index | Hand-rolled 1° × 1° grid (`HashMap<(lat, lon), Vec<u32>>`), haversine, nearest place within **50 km** (else no place), longitude wrap at the antimeridian. `kiddo` 6.3 (k-d tree) was considered: a dependency for ~80 lines of verified code, and its query API changed shape in 6.x. | Verified (Task 19 tests). |
| Attribution | "Place names: GeoNames (CC BY 4.0)" under the place in the lightbox info panel and in the README. | Licence requirement. |
| PDF engine | **`pdfium-render` 0.9.5** (MIT/Apache-2.0) + the **pdfium** shared library from `bblanchon/pdfium-binaries` release `chromium/8125` (matches the crate's `pdfium_latest = pdfium_8125` feature; non-V8 build, so no JavaScript engine), BSD-3-Clause/Apache-2.0 with third-party notices shipped in the archive's `licenses/`. Bound at runtime from `SQUIRREL_PDFIUM_DIR`; if absent, PDFs get `unsupported` thumbnails/text (logged once). Rejected: **MuPDF** (`mupdf` 0.8 crate) — **AGPL-3.0** (or a commercial licence) would force Squirrel's licence; **`pdf-extract` 0.12 / `lopdf` 0.47** — text only (no first-page render), and `pdf-extract` panics on some malformed files; **poppler-utils** CLIs — viable GPL fallback as separate executables, but two more system tools and a looser output contract. | First-page render and text from one engine with the best real-world PDF coverage (Chromium's). |
| PDF work | Thumbnail: first page rendered to fit 1440 × 1440, then the same `thumb`/`preview` WebP + blurhash. Text: pages in order until 256 KiB of UTF-8, at most 200 pages. `page_count` stored. Encrypted PDFs → `unsupported`. | Bounded. |
| Plain text | `text/plain`, `text/markdown`: first 256 KiB, lossy UTF-8, in the worker (no subprocess needed). | Free win for notes. |
| Derived files | `<data_dir>/derived/ab/cd/<sha256>/{thumb,preview}-v<N>.webp` (dirs 0700, files 0400), written to `<data_dir>/tmp/` then renamed (same filesystem, checked at start like `blobs/`). Keyed by **blob** hash: dedup shares them, any user. Removed together with the blob by GC (same session advisory lock, right after the blob file) and by an orphan sweep (derived directories without a `blobs` row, older than the grace period). Phase 4 adds `poster-v1.webp` and `hls/` in the same directory. | One place to back up or wipe; no per-user copies. |

### Schema and API

| Topic | Decision | Why |
|---|---|---|
| `media` (per blob) | `sha256` PK → `blobs ON DELETE CASCADE`; `kind` (`image`/`pdf`/`text`); per-step states `metadata_state`, `thumbs_state`, `text_state` ∈ `pending`/`ready`/`failed`/`unsupported`; `thumbs_version`; displayed `width`/`height`; `blurhash`; EXIF time/offset, GPS, camera fields, `is_screenshot`, `page_count`; place fields + `geocoded_with`; `last_error`. | Content-derived facts, computed once; never served without an owner-scoped join through `nodes`/`assets`. |
| `assets` (per node) | `node_id` PK, `owner_id`, FK `(node_id, owner_id) → nodes ON DELETE CASCADE`, `sha256 → media`, `kind` (`photo`/`screenshot`; Phase 4 adds `video`), `taken_at` (wall clock), `taken_offset_min`, `taken_source`. Index `(owner_id, taken_at DESC, node_id DESC)`. | The timeline is per user and survives renames/moves; trashed nodes are filtered by joining `nodes.deleted_at IS NULL`, so restore needs no bookkeeping; permanent deletion cascades. |
| Timeline scope | **Every live image file of the user, wherever it is** (not only under `Photos/`). | Moves never add/remove timeline entries; matches Google Photos/Immich (**Open question 9**). |
| `albums`, `album_items` | Per owner; `album_items (album_id, node_id, owner_id)` with composite FKs to `albums (id, owner_id)` and `assets (node_id, owner_id)` — the database itself forbids putting another user's photo in an album. Cover: `cover_node_id` (`ON DELETE SET NULL (cover_node_id)`, Postgres ≥ 15), else the newest item. Items are ordered by `taken_at`. Trashed photos disappear from albums and come back on restore. | — |
| `document_text` | `sha256` PK → `media`, `body` (≤ 256 KiB, for snippets), `content tsvector` (GIN). Text search configuration `squirrel_text` = `simple` + `unaccent` (trusted extension); queries are built from the user's words as prefix terms (`'ecole':* & 'elec':*`), words shorter than 2 characters dropped. File **names** are searched too (GIN expression index on `to_tsvector('squirrel_text', name)` for live files). | Language-agnostic (French and English mixed at home), accent-insensitive, matches as you type (**Open question 11**). |
| Timeline API | `GET /api/photos?month=YYYY-MM&cursor&limit` (keyset on `(taken_at, node_id)`; `month` optional) and `GET /api/photos/months` (`[{month, count}]`, newest first). The SPA lays out months from their counts and loads each month's items when it nears the viewport; the scrubber is built from the same list. | Exact scrubber without loading everything; a 50 000-photo library costs one small query plus the months on screen. |
| Asset detail | `GET /api/photos/{id}`: summary + size, `place`, `location` (lat/lon/alt), `camera` (make, model, lens, exposure, f-number, ISO, focal length), `albums` it belongs to. | The lightbox info panel. |
| Thumbnails | `GET /api/files/{id}/thumbnail?size=thumb|preview&v=N` for any live file of the caller with ready thumbnails (images and PDFs). `image/webp`, `nosniff`, `Cross-Origin-Resource-Policy: same-origin`, the sandbox CSP and `X-Frame-Options: SAMEORIGIN` as for content; strong ETag `"<sha256>-<size>-v<N>"`; `Cache-Control: private, max-age=31536000, immutable` (the URL carries the version, the node's bytes never change). Not ready → 404 `not_ready`. Served from memory (≤ ~300 KB) **without** a download slot (a grid fires dozens at once). In `json_routes` (30 s timeout). | Scrolling must not revalidate hundreds of images. |
| Logout and caches | `POST /api/auth/logout` and `logout-all` responses add `Clear-Site-Data: "cache"`. | Long-lived private thumbnails must not outlive the session on a shared device (Safari < 17 ignores the header: **Open question 10**). |
| Thumbnails in lists | `NodeDto` gains `thumbnail: {version, blurhash} | null` (non-null only when ready), filled by one batch query per listing page (`media::thumbnails_for(&[sha256])`). | Files and Home rows show real thumbnails without one probe per row. |
| Other endpoints | `GET /api/media/status` → `{queued, processing, failed}` counts of the caller's media (drives "Preparing 12 photos…"); albums CRUD; `GET /api/search/documents?q&offset&limit` → `{items: [{node, snippet: [{text, highlight}], page_count}], has_more}` (offset paging, ≤ 200 results; snippet split server-side, never HTML). | — |
| Errors | Existing envelope. New code: `not_ready` (404). Everything owner-scoped: another user's node, album or asset is exactly `not_found`. | — |
| Backfill | `backfill` job (enqueued at every worker start and by `squirrel admin media backfill`): pages through **all** file nodes (live and trashed, so restores need no hook) by id, 500 per run, enqueuing `process_media` with background priority for supported files without an `assets` row (images) or `media` row (documents); also re-enqueues `thumbnails` where `thumbs_version < THUMBS_VERSION` and `geocode` where GPS exists and `geocoded_with` ≠ the loaded dataset; then re-enqueues itself with the next cursor (and `run_at` a few seconds later). | Files from Phase 2 get processed; upgrades regenerate; idempotent thanks to dedupe keys. |

## Open questions for the user

Each needs a yes/no (or a choice) before the task that uses it. The plan is written for the recommended answer.

| # | Question | Recommendation |
|---|---|---|
| 1 | **New Rust crates** (licences): `kamadak-exif` 0.6.2 (BSD-2), `image` 0.25.10 (MIT/Apache-2.0), `fast_image_resize` 6.1.0 (MIT/Apache-2.0), `webp` 0.3.1 + `libwebp-sys` 0.9 (MIT/Apache-2.0; vendored libwebp BSD-3, needs a C compiler), `blurhash` 0.2.3 (MIT/Apache-2.0), `pdfium-render` 0.9.5 (MIT/Apache-2.0), optional `libheif-rs` 3.0.0 (MIT). Workspace feature changes: `sqlx` + `json`, `rustix` + `process`. | **Accept.** All permissive; each replaces substantial hand-written code. |
| 2 | **pdfium binary** (Google's PDF engine, BSD-3/Apache-2.0 + bundled third-party notices; ~6 MB) downloaded from `github.com/bblanchon/pdfium-binaries` release `chromium/8125`, pinned by SHA-256, loaded at runtime from `SQUIRREL_PDFIUM_DIR`. | **Accept.** Building pdfium ourselves is a multi-hour Chromium toolchain job; pinning the hash gives the same integrity. |
| 3 | **HEIC support via libheif + libde265** (system libraries, LGPL-3.0, dynamically linked). HEVC is patent-encumbered; decoding for private home use is what every Linux distribution ships. | **Optional cargo feature `heic`, on in the Docker image (Phase 7), off in default dev builds.** |
| 4 | **Where to publish the GeoNames snapshot** (CC BY 4.0 allows redistribution with attribution). | **A GitHub release asset of the Squirrel repository** (`geodata-2026-11`), pinned by SHA-256; the maintainer publishes it once (Task 20 documents the steps). Alternative: no pin, download GeoNames' daily zip at install time. |
| 5 | **Should the worker download data files itself** (adds `reqwest` 0.13 + rustls/aws-lc, ~60 transitive crates)? | **No, not in Phase 3:** a script for dev and the Docker build for production. Revisit for Phase 5 models. |
| 6 | **Map in the lightbox:** a privacy-preserving offline locator (Natural Earth 1:110m land outlines, public domain, ~40 KB static SVG) with an accent dot, the coordinates, and an "Open in OpenStreetMap" link that only the user's click follows. A real tile map needs `maplibre-gl` (BSD-3, ~800 KB) + self-hosted tiles (Protomaps PMTiles, `pmtiles` npm) or third-party tile servers (leak locations). | **Offline locator now**; decide on self-hosted vector tiles with the Explore → Places map. |
| 7 | **No npm dependency** for the justified grid, virtualization and blurhash (hand-rolled, ~250 lines, unit-tested). Alternatives: `@tanstack/svelte-virtual`, `justified-layout` (Flickr, MIT), `blurhash` (MIT). | **Hand-roll** (done and verified in planning). |
| 8 | **Design additions to MASTER.md** (needs re-validation, per CLAUDE.md): photo tiles `--radius-tile` **6 px** (cards' 18 px is too round for dense 112 px tiles), grid gap `--space-1` (4 px, existing), row heights `--photo-row` 112 / 168 / 208 px (phone / ≥ 768 / ≥ 1440), album cards `--album-min` 160 px, lightbox on a **graphite stage** reusing the rail tokens (`--rail`, `--rail-ink`, `--rail-well`), selection = 2 px `--select-ring` + a 24 px accent check circle, scrubber = month labels in `--text-xs` tabular `--ink-muted` with a white pill thumb. | **Accept these values** (Task 31 writes them into MASTER.md and tokens.css first). |
| 9 | **Timeline scope:** all images anywhere in the drive, or only those under `Photos/`? | **Anywhere** (simpler, no bookkeeping on move; scanned JPEGs in `Documents/` do appear). |
| 10 | **Thumbnail caching:** `private, max-age=1 year, immutable` + versioned URL + `Clear-Site-Data: "cache"` on logout. On Safari < 17 the cache survives logout on that device. | **Accept** (home use; the alternative revalidates every tile on every scroll). |
| 11 | **Search language:** `simple` + `unaccent` + prefix matching (no stemming: "facture" does not find "factures" unless typed as a prefix). | **Accept**; stemming per language can come later with a `language` column. |
| 12 | **Decode isolation:** child process per media job (default) vs in-process. | **Child process** (pdfium and libheif are C/C++ parsing untrusted files). |
| 13 | **Thumbnail sizes:** 360 px tall grid tiles and 1440 px previews instead of the design's 256 px. | **Accept** (sharp on 2×/3× screens). |
| 14 | **Squirrel has no LICENSE file yet.** Every dependency chosen here is compatible with both permissive and copyleft licences (MuPDF was rejected because AGPL would decide it for you). | **Pick a licence before Phase 7** (not blocking Phase 3). |

## API added in Phase 3

| Method & path | Group | Purpose |
|---|---|---|
| `GET /api/photos?month&cursor&limit` | json | Timeline page: `{items: [AssetSummary], next_cursor}` |
| `GET /api/photos/months` | json | `[{month: "2024-07", count}]`, newest first |
| `GET /api/photos/{id}` | json | `AssetDetails` (EXIF, place, location, albums) |
| `GET /api/files/{id}/thumbnail?size&v` | json | WebP thumbnail/preview of an image or PDF |
| `GET /api/media/status` | json | `{queued, processing, failed}` for the caller |
| `GET /api/albums` · `POST /api/albums` | json | List (with count and cover) · create `{name, node_ids?}` |
| `GET` · `PATCH` · `DELETE /api/albums/{id}` | json | Album · rename / set cover · delete (photos stay) |
| `GET /api/albums/{id}/items?cursor&limit` | json | Album items, newest first |
| `POST /api/albums/{id}/items` · `POST /api/albums/{id}/items/remove` | json | Add / remove `{node_ids: [..500]}` → `{changed}` |
| `GET /api/search/documents?q&offset&limit` | json | Name + content search over the caller's files |
| `GET /api/admin/jobs` · `POST /api/admin/jobs/{id}/retry` | json | Queue summary and failures (admins) · retry |

Changed: `NodeDto.thumbnail`; logout responses carry `Clear-Site-Data`. New error code: `not_ready` (404).

## Milestones

- **A — Job queue & worker** (Tasks 1–7): ships a working queue, `squirrel worker`, `serve --worker`, admin visibility. No media yet.
- **B — Media extraction & thumbnails** (Tasks 8–18): `squirrel-media`, sandbox, `media`/`assets`, handlers, enqueue on upload, backfill, thumbnail endpoint. Ships thumbnails for every image.
- **C — Offline geocoding** (Tasks 19–21): places for geotagged photos.
- **D — Timeline & albums API** (Tasks 22–26).
- **E — Documents: PDF thumbnails, text, search API** (Tasks 27–30).
- **F — Web: Photos timeline & lightbox** (Tasks 31–37).
- **G — Web: albums, list thumbnails, document search** (Tasks 38–41).
- **H — E2E, CI & docs** (Tasks 42–44).

Each milestone ends green (`cargo test --workspace`, `pnpm check`, `pnpm test:unit`, and from F on the E2E suite) and can be reviewed and merged on its own.

---

## Milestone A — Job queue & worker

### Task 1: Migration `20261114000002_jobs.sql`

**Files:**
- Create: `crates/squirrel-db/migrations/20261114000002_jobs.sql`
- Create: `crates/squirrel-db/tests/jobs_schema.rs`
- Modify: `Cargo.toml` (workspace `sqlx` features: add `"json"`)

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/jobs_schema.rs`:

```rust
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use std::time::Duration;

async fn insert(pool: &PgPool, kind: &str, key: Option<&str>) -> u64 {
    sqlx::query(
        "INSERT INTO jobs (kind, dedupe_key) VALUES ($1, $2) \
         ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') \
         DO NOTHING",
    )
    .bind(kind)
    .bind(key)
    .execute(pool)
    .await
    .unwrap()
    .rows_affected()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn dedupe_keys_are_unique_while_queued_or_running(pool: PgPool) {
    assert_eq!(insert(&pool, "thumbnails", Some("thumbnails:a")).await, 1);
    assert_eq!(insert(&pool, "thumbnails", Some("thumbnails:a")).await, 0);
    assert_eq!(insert(&pool, "thumbnails", None).await, 1);
    assert_eq!(insert(&pool, "thumbnails", None).await, 1, "no key, no dedupe");
    sqlx::query("UPDATE jobs SET status = 'failed' WHERE dedupe_key = 'thumbnails:a'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(insert(&pool, "thumbnails", Some("thumbnails:a")).await, 1, "failed jobs do not block");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn running_jobs_must_hold_a_lease_and_a_claim(pool: PgPool) {
    insert(&pool, "x", None).await;
    let err = sqlx::query("UPDATE jobs SET status = 'running'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("jobs_running_is_claimed"), "{err}");
    for bad in ["Bad-Kind", "", "x y"] {
        assert!(sqlx::query("INSERT INTO jobs (kind) VALUES ($1)").bind(bad).execute(&pool).await.is_err(), "{bad}");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn inserting_jobs_notifies_listeners(pool: PgPool) {
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen("squirrel_jobs").await.unwrap();
    insert(&pool, "x", None).await;
    let note = tokio::time::timeout(Duration::from_secs(5), listener.recv())
        .await
        .expect("a notification")
        .unwrap();
    assert_eq!(note.channel(), "squirrel_jobs");
}
```

**Step 2: Run** — `cargo test -p squirrel-db --test jobs_schema` → FAIL (`relation "jobs" does not exist`).

**Step 3: Write the migration** (verified on Postgres 17):

```sql
-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: a Postgres job queue (claimed with FOR UPDATE SKIP LOCKED).

CREATE TABLE jobs (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_]{0,63}$'),
    payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Higher runs first. Interactive work ~100, backfill ~0, ML (Phase 5) below 0.
    priority      SMALLINT NOT NULL DEFAULT 0,
    -- queued → running → (deleted on success) | queued again (retry) | failed.
    status        TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'failed')),
    attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts  INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    -- Not before this time (retries back off by pushing it forward).
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Visibility timeout: a running job whose lease expired goes back to the queue.
    locked_until  TIMESTAMPTZ,
    -- Fencing token of the current claim: a worker that lost its lease cannot
    -- complete, retry or extend a job another worker has claimed since.
    claim_id      UUID,
    -- Which worker holds it (for admins), e.g. 'minipc:4242'.
    claimed_by    TEXT CHECK (claimed_by IS NULL OR char_length(claimed_by) <= 200),
    last_error    TEXT CHECK (last_error IS NULL OR char_length(last_error) <= 2000),
    -- At most one queued or running job per key (e.g. 'thumbnails:<sha256>').
    dedupe_key    TEXT CHECK (dedupe_key IS NULL OR char_length(dedupe_key) <= 200),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT jobs_running_is_claimed CHECK (
        (status = 'running') = (locked_until IS NOT NULL AND claim_id IS NOT NULL))
);
-- Claim order. Partial: finished jobs are deleted, failed ones are never claimed.
CREATE INDEX jobs_claim_idx ON jobs (priority DESC, run_at, id) WHERE status = 'queued';
CREATE INDEX jobs_lease_idx ON jobs (locked_until) WHERE status = 'running';
CREATE UNIQUE INDEX jobs_dedupe_key ON jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running');
CREATE INDEX jobs_failed_idx ON jobs (updated_at DESC) WHERE status = 'failed';

-- Wakes idle workers. Statement-level: one notification per INSERT, however many rows.
CREATE FUNCTION jobs_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('squirrel_jobs', '');
    RETURN NULL;
END;
$$;
CREATE TRIGGER jobs_notify AFTER INSERT ON jobs
    FOR EACH STATEMENT EXECUTE FUNCTION jobs_notify();
```

Notifications are delivered at commit, so a job enqueued inside an upload transaction wakes workers only once the node is visible.

Workspace `Cargo.toml`: `sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "uuid", "time", "migrate", "macros", "json"] }`.

**Step 4: Run** — `cargo test -p squirrel-db` → PASS (existing + 3 new). Also extend `tests/migrations.rs`' expected version list if it enumerates versions (it does for `pending_migrations`; add `20261114000002`).

**Step 5: Commit**
```bash
git add Cargo.toml Cargo.lock crates/squirrel-db/migrations/20261114000002_jobs.sql crates/squirrel-db/tests
git commit -m "feat(db): add the jobs table and its wake-up notification" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `squirrel-jobs` crate — job types and backoff

**Files:**
- Create: `crates/squirrel-jobs/Cargo.toml`, `crates/squirrel-jobs/src/lib.rs`, `crates/squirrel-jobs/src/backoff.rs`, `crates/squirrel-jobs/src/job.rs`
- Modify: `Cargo.toml` (workspace dependency `squirrel-jobs = { path = "crates/squirrel-jobs" }`)

`Cargo.toml`:
```toml
[package]
name = "squirrel-jobs"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
anyhow.workspace = true
rand.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid.workspace = true

[dev-dependencies]
squirrel-db.workspace = true
```

**Step 1: Write the failing tests** — in `backoff.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_fourfold_from_thirty_seconds_and_cap_at_an_hour() {
        let secs = |n| retry_delay(n, 1.0).as_secs();
        assert_eq!([secs(1), secs(2), secs(3), secs(4)], [30, 120, 480, 1920]);
        assert_eq!(secs(5), 3600);
        assert_eq!(secs(40), 3600, "no overflow");
        assert_eq!(secs(0), 30, "attempt 0 is treated as 1");
    }

    #[test]
    fn jitter_scales_the_delay() {
        assert_eq!(retry_delay(1, 0.8).as_secs(), 24);
        assert_eq!(retry_delay(1, 1.2).as_secs(), 36);
    }
}
```

and in `job.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builder_sets_priority_dedupe_and_payload() {
        let job = NewJob::new("thumbnails", json!({"sha256": "ab"}))
            .priority(priority::THUMBNAILS)
            .dedupe("thumbnails:ab");
        assert_eq!(job.kind, "thumbnails");
        assert_eq!(job.priority, 80);
        assert_eq!(job.dedupe_key.as_deref(), Some("thumbnails:ab"));
    }

    #[test]
    fn background_work_runs_after_interactive_work() {
        assert!(priority::background(priority::THUMBNAILS) < priority::GEOCODE);
        assert!(priority::background(priority::INTERACTIVE) > priority::BACKFILL);
    }

    #[test]
    fn payloads_decode_into_their_type_or_fail_permanently() {
        #[derive(serde::Deserialize)]
        struct P { sha256: String }
        let ctx = JobContext::for_tests("thumbnails", json!({"sha256": "ab"}));
        assert_eq!(ctx.payload::<P>().unwrap().sha256, "ab");
        let bad = JobContext::for_tests("thumbnails", json!({"nope": 1}));
        assert!(matches!(bad.payload::<P>(), Err(JobError::Permanent(_))));
    }
}
```

**Step 2: Run** — `cargo test -p squirrel-jobs` → FAIL (does not compile).

**Step 3: Implement.** `backoff.rs`:

```rust
//! Retry delays: 30 s × 4^(attempt − 1), capped at one hour, times a jitter factor.

use std::time::Duration;

const BASE_SECS: f64 = 30.0;
const FACTOR: f64 = 4.0;
const MAX_SECS: f64 = 3600.0;

/// Delay before retrying after failed attempt number `attempt` (1-based).
/// `jitter` is a factor, normally drawn from 0.8..=1.2 (see [`jitter`]).
pub fn retry_delay(attempt: i32, jitter: f64) -> Duration {
    let exponent = attempt.clamp(1, 16) - 1;
    let secs = (BASE_SECS * FACTOR.powi(exponent)).min(MAX_SECS) * jitter;
    Duration::from_secs_f64(secs.round().max(1.0))
}

/// A random factor in 0.8..=1.2, so retries of a burst of failures spread out.
pub fn jitter() -> f64 {
    rand::Rng::gen_range(&mut rand::thread_rng(), 0.8..=1.2)
}
```

`job.rs`:

```rust
//! What handlers see and return, and how jobs are described when enqueued.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Job priorities: higher runs first.
pub mod priority {
    /// Processing a file the user just uploaded.
    pub const INTERACTIVE: i16 = 100;
    pub const THUMBNAILS: i16 = 80;
    pub const METADATA: i16 = 70;
    pub const TEXT: i16 = 40;
    pub const GEOCODE: i16 = 30;
    pub const BACKFILL: i16 = 0;
    /// Subtracted from work that comes from backfill rather than a fresh upload.
    pub const BACKGROUND_PENALTY: i16 = 50;

    pub const fn background(priority: i16) -> i16 {
        priority - BACKGROUND_PENALTY
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub kind: &'static str,
    pub payload: Value,
    pub priority: i16,
    pub dedupe_key: Option<String>,
    /// Not before (default: now).
    pub delay: Option<Duration>,
}

impl NewJob {
    pub fn new(kind: &'static str, payload: Value) -> Self {
        Self { kind, payload, priority: 0, dedupe_key: None, delay: None }
    }

    pub fn priority(mut self, priority: i16) -> Self {
        self.priority = priority;
        self
    }

    pub fn dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

/// How a handler run ended, when it did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Transient: retried with backoff, `failed` after the last attempt.
    #[error("{0:#}")]
    Retry(anyhow::Error),
    /// Retrying cannot help (bad payload, bug): `failed` at once.
    #[error("{0:#}")]
    Permanent(anyhow::Error),
    /// A dependency is not ready (e.g. no geodata): run again after the delay
    /// without using an attempt.
    #[error("not ready yet, rescheduled in {0:?}")]
    Reschedule(Duration),
}

impl JobError {
    pub fn retry(err: impl Into<anyhow::Error>) -> Self {
        Self::Retry(err.into())
    }

    pub fn permanent(err: impl Into<anyhow::Error>) -> Self {
        Self::Permanent(err.into())
    }
}

/// One claimed job, as handed to its handler.
#[derive(Debug, Clone)]
pub struct JobContext {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    /// 1-based number of this attempt.
    pub attempt: i32,
    pub max_attempts: i32,
}

impl JobContext {
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, JobError> {
        serde_json::from_value(self.payload.clone()).map_err(JobError::permanent)
    }

    /// The next `Retry` would mark the job failed: write terminal states now.
    pub fn is_last_attempt(&self) -> bool {
        self.attempt >= self.max_attempts
    }

    #[doc(hidden)]
    pub fn for_tests(kind: &str, payload: Value) -> Self {
        Self { id: 0, kind: kind.to_owned(), payload, attempt: 1, max_attempts: 5 }
    }
}
```

`lib.rs`:
```rust
//! A Postgres job queue (`jobs` table, see migration 20261114000002) and the
//! worker runtime that executes it.

mod backoff;
mod job;
pub mod queue;
mod worker;

pub use backoff::{jitter, retry_delay};
pub use job::{JobContext, JobError, NewJob, priority};
pub use queue::enqueue;
pub use worker::{Registry, Worker, WorkerConfig};
```
(Leave `queue` and `worker` as empty modules with `//!` docs until Tasks 3–5.)

**Step 4: Run** — `cargo test -p squirrel-jobs` → PASS (5 tests).

**Step 5: Commit**
```bash
git add Cargo.toml Cargo.lock crates/squirrel-jobs
git commit -m "feat(jobs): add job types, priorities and retry backoff" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Queue operations — enqueue, claim, complete, retry, lease

**Files:**
- Modify: `crates/squirrel-jobs/src/queue.rs`
- Create: `crates/squirrel-jobs/tests/queue.rs`

**Step 1: Write the failing tests** — `crates/squirrel-jobs/tests/queue.rs`:

```rust
use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::queue::{self, Finished};
use squirrel_jobs::{NewJob, priority};

const LEASE: Duration = Duration::from_secs(300);
const ALL: &[&str] = &["a", "b", "thumbnails"];

async fn add(pool: &PgPool, job: NewJob) -> bool {
    let mut conn = pool.acquire().await.unwrap();
    squirrel_jobs::enqueue(&mut conn, &job).await.unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn claims_by_priority_then_age_and_only_known_kinds(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({"n": 1}))).await;
    add(&pool, NewJob::new("b", json!({"n": 2})).priority(priority::INTERACTIVE)).await;
    add(&pool, NewJob::new("unknown", json!({}))).await;
    let claimed = queue::claim(&pool, ALL, 10, LEASE, "test").await.unwrap();
    let kinds: Vec<_> = claimed.iter().map(|j| j.kind.as_str()).collect();
    assert_eq!(kinds, ["b", "a"]);
    assert!(claimed.iter().all(|j| j.attempts == 1));
    assert!(queue::claim(&pool, ALL, 10, LEASE, "test").await.unwrap().is_empty(), "nothing left");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_claims_never_share_a_job(pool: PgPool) {
    for n in 0..20 {
        add(&pool, NewJob::new("a", json!({"n": n}))).await;
    }
    let (left, right) = tokio::join!(
        queue::claim(&pool, ALL, 15, LEASE, "w1"),
        queue::claim(&pool, ALL, 15, LEASE, "w2")
    );
    let mut ids: Vec<i64> = left.unwrap().into_iter().chain(right.unwrap()).map(|j| j.id).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!((ids.len(), total), (20, 20));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_deletes_the_job_but_only_for_the_current_claim(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
    assert!(!queue::complete(&pool, job.id, uuid::Uuid::now_v7()).await.unwrap(), "stale claim");
    assert!(queue::complete(&pool, job.id, job.claim_id).await.unwrap());
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn retries_back_off_then_fail_after_max_attempts(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    for attempt in 1..=5 {
        sqlx::query("UPDATE jobs SET run_at = now()").execute(&pool).await.unwrap();
        let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
        assert_eq!(job.attempts, attempt);
        let finished = queue::retry(&pool, &job, "boom", Duration::from_secs(60)).await.unwrap();
        let expected = if attempt < 5 { Finished::Requeued } else { Finished::Failed };
        assert_eq!(finished, expected, "attempt {attempt}");
    }
    let (status, error): (String, String) =
        sqlx::query_as("SELECT status, last_error FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!((status.as_str(), error.as_str()), ("failed", "boom"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_retried_job_waits_for_its_backoff(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
    queue::retry(&pool, &job, "boom", Duration::from_secs(60)).await.unwrap();
    assert!(queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn reschedule_and_release_do_not_use_an_attempt(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
    queue::reschedule(&pool, &job, Duration::ZERO).await.unwrap();
    let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
    assert_eq!(job.attempts, 1);
    queue::release(&pool, &job).await.unwrap();
    let job = queue::claim(&pool, ALL, 1, LEASE, "w").await.unwrap().remove(0);
    assert_eq!(job.attempts, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_leases_are_reaped_and_fenced(pool: PgPool) {
    add(&pool, NewJob::new("a", json!({}))).await;
    let first = queue::claim(&pool, ALL, 1, Duration::from_millis(1), "w1").await.unwrap().remove(0);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(queue::reap_expired(&pool).await.unwrap(), 1);
    let second = queue::claim(&pool, ALL, 1, LEASE, "w2").await.unwrap().remove(0);
    assert_eq!(second.attempts, 2);
    assert!(!queue::heartbeat(&pool, &first, LEASE).await.unwrap(), "lost lease");
    assert!(!queue::complete(&pool, first.id, first.claim_id).await.unwrap());
    assert!(queue::heartbeat(&pool, &second, LEASE).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn enqueue_is_transactional_and_deduplicated(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    let job = NewJob::new("thumbnails", json!({})).dedupe("thumbnails:x");
    assert!(squirrel_jobs::enqueue(&mut tx, &job).await.unwrap());
    assert!(!squirrel_jobs::enqueue(&mut tx, &job).await.unwrap());
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "rolled back with the transaction");
}
```

**Step 2: Run** — `cargo test -p squirrel-jobs --test queue` → FAIL.

**Step 3: Implement** — `crates/squirrel-jobs/src/queue.rs`:

```rust
//! SQL for the `jobs` table. Every state change after a claim matches the
//! claim's fencing token (`claim_id`), so a worker that lost its lease is a no-op.

use std::time::Duration;

use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::job::NewJob;

pub const CHANNEL: &str = "squirrel_jobs";
const MAX_ERROR_CHARS: usize = 2000;

#[derive(Debug, Clone, FromRow)]
pub struct ClaimedJob {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    pub priority: i16,
    pub attempts: i32,
    pub max_attempts: i32,
    pub claim_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finished {
    Requeued,
    Failed,
    /// The claim was stale: another worker owns the job now.
    LostLease,
}

fn secs(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

fn truncate(message: &str) -> String {
    message.chars().take(MAX_ERROR_CHARS).collect()
}

/// Enqueues in the caller's connection or transaction: the job exists exactly
/// when the transaction commits. `false`: an equivalent job (same dedupe key)
/// is already queued or running.
pub async fn enqueue(conn: &mut PgConnection, job: &NewJob) -> Result<bool, sqlx::Error> {
    let delay = job.delay.map_or(0.0, secs);
    let result = sqlx::query(
        "INSERT INTO jobs (kind, payload, priority, dedupe_key, run_at) \
         VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5)) \
         ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running') \
         DO NOTHING",
    )
    .bind(job.kind)
    .bind(&job.payload)
    .bind(job.priority)
    .bind(job.dedupe_key.as_deref())
    .bind(delay)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Claims up to `limit` due jobs of `kinds`, highest priority first.
pub async fn claim(
    pool: &PgPool,
    kinds: &[&str],
    limit: i64,
    lease: Duration,
    worker: &str,
) -> Result<Vec<ClaimedJob>, sqlx::Error> {
    sqlx::query_as::<_, ClaimedJob>(
        "WITH picked AS ( \
             SELECT id FROM jobs \
              WHERE status = 'queued' AND run_at <= now() AND kind = ANY($1) \
              ORDER BY priority DESC, run_at, id \
              LIMIT $2 \
                FOR UPDATE SKIP LOCKED) \
         UPDATE jobs j \
            SET status = 'running', attempts = j.attempts + 1, \
                locked_until = now() + make_interval(secs => $3), \
                claim_id = gen_random_uuid(), claimed_by = $4, updated_at = now() \
           FROM picked WHERE j.id = picked.id \
         RETURNING j.id, j.kind, j.payload, j.priority, j.attempts, j.max_attempts, j.claim_id",
    )
    .bind(kinds)
    .bind(limit)
    .bind(secs(lease))
    .bind(worker)
    .fetch_all(pool)
    .await
    .map(|mut jobs| {
        // RETURNING order is unspecified: restore the claim order (deterministic logs and tests).
        jobs.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
        jobs
    })
}
```

```rust
/// Success: the job row is deleted. `false` if the claim was stale.
pub async fn complete(pool: &PgPool, id: i64, claim_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM jobs WHERE id = $1 AND claim_id = $2 AND status = 'running'")
        .bind(id)
        .bind(claim_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// A failed attempt: back to the queue after `delay`, or `failed` if it was the last.
pub async fn retry(pool: &PgPool, job: &ClaimedJob, error: &str, delay: Duration) -> Result<Finished, sqlx::Error> {
    let status: Option<String> = sqlx::query_scalar(
        "UPDATE jobs SET status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END, \
                run_at = now() + make_interval(secs => $3), locked_until = NULL, claim_id = NULL, \
                claimed_by = NULL, last_error = $4, updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running' RETURNING status",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(delay))
    .bind(truncate(error))
    .fetch_optional(pool)
    .await?;
    Ok(match status.as_deref() {
        Some("failed") => Finished::Failed,
        Some(_) => Finished::Requeued,
        None => Finished::LostLease,
    })
}

/// Fails the job now, whatever attempts remain.
pub async fn fail(pool: &PgPool, job: &ClaimedJob, error: &str) -> Result<Finished, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE jobs SET status = 'failed', locked_until = NULL, claim_id = NULL, claimed_by = NULL, \
                last_error = $3, updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(truncate(error))
    .execute(pool)
    .await?;
    Ok(if updated.rows_affected() == 1 { Finished::Failed } else { Finished::LostLease })
}

/// Back to the queue after `delay`, giving the attempt back.
pub async fn reschedule(pool: &PgPool, job: &ClaimedJob, delay: Duration) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE jobs SET status = 'queued', attempts = attempts - 1, \
                run_at = now() + make_interval(secs => $3), locked_until = NULL, claim_id = NULL, \
                claimed_by = NULL, updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(delay))
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Shutdown: hand an unfinished job back at once, attempt not counted.
pub async fn release(pool: &PgPool, job: &ClaimedJob) -> Result<bool, sqlx::Error> {
    reschedule(pool, job, Duration::ZERO).await
}

/// Extends the lease of a job still being worked on. `false`: the lease was lost.
pub async fn heartbeat(pool: &PgPool, job: &ClaimedJob, lease: Duration) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE jobs SET locked_until = now() + make_interval(secs => $3), updated_at = now() \
          WHERE id = $1 AND claim_id = $2 AND status = 'running'",
    )
    .bind(job.id)
    .bind(job.claim_id)
    .bind(secs(lease))
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Running jobs whose lease expired (worker crashed or stalled) go back to the
/// queue, or to `failed` when that was their last attempt.
pub async fn reap_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let reaped = sqlx::query(
        "WITH expired AS ( \
             SELECT id FROM jobs WHERE status = 'running' AND locked_until < now() \
                FOR UPDATE SKIP LOCKED) \
         UPDATE jobs j \
            SET status = CASE WHEN j.attempts >= j.max_attempts THEN 'failed' ELSE 'queued' END, \
                locked_until = NULL, claim_id = NULL, claimed_by = NULL, \
                last_error = 'lease expired: the worker stopped or the job ran too long', \
                run_at = now(), updated_at = now() \
           FROM expired WHERE j.id = expired.id",
    )
    .execute(pool)
    .await?;
    Ok(reaped.rows_affected())
}
```

**Step 4: Run** — `cargo test -p squirrel-jobs` → PASS (13 tests).

**Step 5: Commit**
```bash
git add crates/squirrel-jobs
git commit -m "feat(jobs): claim, complete, retry and lease jobs with fencing" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Queue administration — summary, failures, retry, retention

**Files:**
- Create: `crates/squirrel-jobs/src/admin.rs` (re-exported as `squirrel_jobs::admin`)
- Create: `crates/squirrel-jobs/tests/admin.rs`

**Step 1: Write the failing tests** (`tests/admin.rs`): (1) `summary` groups counts by kind and status (`queued`, `running`, `failed`), kinds sorted; (2) `failed(limit)` lists newest failures first with `last_error`, `attempts`, `updated_at`; (3) `retry_failed(id)` puts a failed job back to `queued` with `attempts = 0`, `run_at = now()`, and returns `RetryOutcome::Requeued`; if an equivalent job (same `dedupe_key`) is already queued, it deletes the failed row instead and returns `RetryOutcome::AlreadyQueued` (unique-violation path); unknown or non-failed id → `NotFound`; (4) `purge_failed_older_than(cutoff)` deletes only failed rows with `updated_at < cutoff`.

**Step 2: Run** → FAIL.

**Step 3: Implement** with these statements:

```rust
pub async fn summary(pool: &PgPool) -> Result<Vec<KindSummary>, sqlx::Error> {
    sqlx::query_as::<_, KindSummary>(
        "SELECT kind, \
                count(*) FILTER (WHERE status = 'queued')  AS queued, \
                count(*) FILTER (WHERE status = 'running') AS running, \
                count(*) FILTER (WHERE status = 'failed')  AS failed \
           FROM jobs GROUP BY kind ORDER BY kind",
    )
    .fetch_all(pool)
    .await
}

pub async fn retry_failed(pool: &PgPool, id: i64) -> Result<RetryOutcome, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE jobs SET status = 'queued', attempts = 0, run_at = now(), updated_at = now() \
          WHERE id = $1 AND status = 'failed'",
    )
    .bind(id)
    .execute(pool)
    .await;
    match result {
        Ok(done) if done.rows_affected() == 1 => Ok(RetryOutcome::Requeued),
        Ok(_) => Ok(RetryOutcome::NotFound),
        Err(sqlx::Error::Database(db)) if db.constraint() == Some("jobs_dedupe_key") => {
            sqlx::query("DELETE FROM jobs WHERE id = $1 AND status = 'failed'")
                .bind(id)
                .execute(pool)
                .await?;
            Ok(RetryOutcome::AlreadyQueued)
        }
        Err(err) => Err(err),
    }
}
```

`KindSummary { kind: String, queued: i64, running: i64, failed: i64 }`, `FailedJob { id, kind, attempts, last_error: Option<String>, updated_at: OffsetDateTime }` (`FromRow`), `RetryOutcome { Requeued, AlreadyQueued, NotFound }`, `purge_failed_older_than(pool, cutoff: OffsetDateTime) -> u64`.

**Step 4: Run** — PASS.

**Step 5: Commit** — `feat(jobs): summarize, list, retry and purge failed jobs`.

---

### Task 5: Worker runtime

**Files:**
- Modify: `crates/squirrel-jobs/src/worker.rs`
- Create: `crates/squirrel-jobs/tests/worker.rs`

**Step 1: Write the failing tests** — `tests/worker.rs` registers test handlers on a `Registry` and runs a `Worker` against the test pool with a short poll interval:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::{JobError, NewJob, Registry, Worker, WorkerConfig};
use tokio::sync::watch;

fn config() -> WorkerConfig {
    WorkerConfig {
        concurrency: 2,
        lease: Duration::from_secs(30),
        poll_interval: Duration::from_millis(50),
        shutdown_grace: Duration::from_secs(2),
        name: "test-worker".into(),
    }
}

async fn add(pool: &PgPool, job: NewJob) {
    let mut conn = pool.acquire().await.unwrap();
    squirrel_jobs::enqueue(&mut conn, &job).await.unwrap();
}

async fn wait_until(pool: &PgPool, sql: &str, expected: i64) {
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar(sql).fetch_one(pool).await.unwrap();
        if n == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {sql} = {expected}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn runs_jobs_and_deletes_them(pool: PgPool) {
    let seen = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    let counter = seen.clone();
    registry.register("count", Duration::from_secs(5), move |_ctx| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(pool.clone(), registry, config()).run(stop_rx));
    for n in 0..5 {
        add(&pool, NewJob::new("count", json!({ "n": n }))).await;
    }
    wait_until(&pool, "SELECT count(*) FROM jobs", 0).await;
    assert_eq!(seen.load(Ordering::SeqCst), 5);
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn errors_retry_panics_too_and_permanent_errors_fail(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("flaky", Duration::from_secs(5), |_ctx| async { Err(JobError::retry(anyhow::anyhow!("later"))) });
    registry.register("broken", Duration::from_secs(5), |_ctx| async { Err(JobError::permanent(anyhow::anyhow!("never"))) });
    registry.register("panics", Duration::from_secs(5), |_ctx| async { panic!("handler bug") });
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(pool.clone(), registry, config()).run(stop_rx));
    add(&pool, NewJob::new("flaky", json!({}))).await;
    add(&pool, NewJob::new("broken", json!({}))).await;
    add(&pool, NewJob::new("panics", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs WHERE kind = 'broken' AND status = 'failed'", 1).await;
    wait_until(&pool, "SELECT count(*) FROM jobs WHERE kind IN ('flaky', 'panics') AND status = 'queued' AND attempts = 1", 2).await;
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn timeouts_count_as_retryable_failures(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("slow", Duration::from_millis(50), |_ctx| async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(())
    });
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(pool.clone(), registry, config()).run(stop_rx));
    add(&pool, NewJob::new("slow", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs WHERE status = 'queued' AND last_error LIKE 'timed out%'", 1).await;
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shutdown_releases_unfinished_jobs(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("forever", Duration::from_secs(600), |_ctx| async {
        tokio::time::sleep(Duration::from_secs(600)).await;
        Ok(())
    });
    let mut cfg = config();
    cfg.shutdown_grace = Duration::from_millis(100);
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(pool.clone(), registry, cfg).run(stop_rx));
    add(&pool, NewJob::new("forever", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs WHERE status = 'running'", 1).await;
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
    let (status, attempts): (String, i32) =
        sqlx::query_as("SELECT status, attempts FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!((status.as_str(), attempts), ("queued", 0));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn reschedule_keeps_the_attempt(pool: PgPool) {
    let mut registry = Registry::new();
    registry.register("later", Duration::from_secs(5), |_ctx| async {
        Err(JobError::Reschedule(Duration::from_secs(3600)))
    });
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn(Worker::new(pool.clone(), registry, config()).run(stop_rx));
    add(&pool, NewJob::new("later", json!({}))).await;
    wait_until(&pool, "SELECT count(*) FROM jobs WHERE status = 'queued' AND attempts = 0 AND run_at > now() + interval '50 minutes'", 1).await;
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
}
```

**Step 2: Run** — `cargo test -p squirrel-jobs --test worker` → FAIL.

**Step 3: Implement** — `crates/squirrel-jobs/src/worker.rs` (UNVERIFIED: not compiled while planning; follow the compiler):

```rust
//! Executes claimed jobs: one claim fills the free slots; LISTEN/NOTIFY wakes
//! the loop, a poll interval catches delayed retries and expired leases.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

use crate::backoff::{jitter, retry_delay};
use crate::job::{JobContext, JobError};
use crate::queue::{self, CHANNEL, ClaimedJob, Finished};

pub type JobFuture = Pin<Box<dyn Future<Output = Result<(), JobError>> + Send>>;
type HandlerFn = Arc<dyn Fn(JobContext) -> JobFuture + Send + Sync>;

struct Handler {
    run: HandlerFn,
    timeout: Duration,
}

/// Job kinds this worker executes, each with its handler and time limit.
#[derive(Default)]
pub struct Registry {
    handlers: HashMap<&'static str, Handler>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// `timeout` must stay below the lease (heartbeats extend the lease, but a
    /// stuck handler should fail rather than hold a slot forever).
    pub fn register<F, Fut>(&mut self, kind: &'static str, timeout: Duration, handler: F)
    where
        F: Fn(JobContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), JobError>> + Send + 'static,
    {
        let run: HandlerFn = Arc::new(move |ctx| Box::pin(handler(ctx)));
        self.handlers.insert(kind, Handler { run, timeout });
    }

    pub fn kinds(&self) -> Vec<&'static str> {
        let mut kinds: Vec<_> = self.handlers.keys().copied().collect();
        kinds.sort_unstable();
        kinds
    }
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub concurrency: usize,
    pub lease: Duration,
    pub poll_interval: Duration,
    pub shutdown_grace: Duration,
    /// Shown to admins as `claimed_by`, e.g. "minipc:4242".
    pub name: String,
}

pub struct Worker {
    pool: PgPool,
    registry: Arc<Registry>,
    config: WorkerConfig,
    /// Claimed and not yet finished: released on shutdown.
    in_flight: Arc<Mutex<HashMap<i64, ClaimedJob>>>,
}

impl Worker {
    pub fn new(pool: PgPool, registry: Registry, config: WorkerConfig) -> Self {
        Self {
            pool,
            registry: Arc::new(registry),
            config,
            in_flight: Arc::default(),
        }
    }

    /// Runs until `stop` turns true, then drains (see [`WorkerConfig::shutdown_grace`]).
    pub async fn run(self, mut stop: watch::Receiver<bool>) -> anyhow::Result<()> {
        let kinds = self.registry.kinds();
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(CHANNEL).await?;
        let slots = Arc::new(Semaphore::new(self.config.concurrency.max(1)));
        let mut running = JoinSet::new();
        tracing::info!(worker = %self.config.name, ?kinds, concurrency = self.config.concurrency, "worker started");
        while !*stop.borrow() {
            if let Err(err) = self.fill(&kinds, &slots, &mut running).await {
                tracing::error!(error = %err, "claiming jobs failed");
            }
            tokio::select! {
                note = listener.recv() => {
                    if let Err(err) = note {
                        tracing::warn!(error = %err, "job notifications interrupted; polling");
                    }
                }
                () = tokio::time::sleep(self.config.poll_interval) => {}
                Some(_) = running.join_next(), if !running.is_empty() => {}
                _ = stop.changed() => {}
            }
        }
        self.drain(running).await;
        Ok(())
    }

    /// Reaps expired leases, then claims as many jobs as there are free slots.
    async fn fill(&self, kinds: &[&str], slots: &Arc<Semaphore>, running: &mut JoinSet<()>) -> anyhow::Result<()> {
        let reaped = queue::reap_expired(&self.pool).await?;
        if reaped > 0 {
            tracing::warn!(reaped, "jobs with expired leases were requeued");
        }
        loop {
            let free = slots.available_permits();
            if free == 0 {
                return Ok(());
            }
            let free = i64::try_from(free).unwrap_or(i64::MAX);
            let jobs = queue::claim(&self.pool, kinds, free, self.config.lease, &self.config.name).await?;
            let claimed = jobs.len();
            for job in jobs {
                let permit = slots.clone().acquire_owned().await?;
                self.in_flight.lock().expect("lock").insert(job.id, job.clone());
                let (pool, registry, config, in_flight) =
                    (self.pool.clone(), self.registry.clone(), self.config.clone(), self.in_flight.clone());
                running.spawn(async move {
                    let _permit = permit;
                    execute(&pool, &registry, &config, &job).await;
                    in_flight.lock().expect("lock").remove(&job.id);
                });
            }
            if i64::try_from(claimed).unwrap_or(0) < free {
                return Ok(());
            }
        }
    }

    async fn drain(&self, mut running: JoinSet<()>) {
        let grace = tokio::time::sleep(self.config.shutdown_grace);
        tokio::pin!(grace);
        loop {
            tokio::select! {
                next = running.join_next() => if next.is_none() { break },
                () = &mut grace => {
                    running.abort_all();
                    break;
                }
            }
        }
        let unfinished: Vec<ClaimedJob> = self.in_flight.lock().expect("lock").drain().map(|(_, j)| j).collect();
        for job in unfinished {
            match queue::release(&self.pool, &job).await {
                Ok(_) => tracing::info!(job_id = job.id, kind = %job.kind, "released an unfinished job"),
                Err(err) => tracing::error!(job_id = job.id, error = %err, "could not release a job; its lease will expire"),
            }
        }
        tracing::info!(worker = %self.config.name, "worker stopped");
    }
}

/// Runs one job with a timeout and heartbeats, then records the outcome.
async fn execute(pool: &PgPool, registry: &Registry, config: &WorkerConfig, job: &ClaimedJob) {
    let Some(handler) = registry.handlers.get(job.kind.as_str()) else {
        let _ = queue::fail(pool, job, "no handler for this job kind").await;
        return;
    };
    let ctx = JobContext {
        id: job.id,
        kind: job.kind.clone(),
        payload: job.payload.clone(),
        attempt: job.attempts,
        max_attempts: job.max_attempts,
    };
    // Its own task, so a panic becomes a JoinError instead of killing the loop.
    let work = tokio::spawn((handler.run)(ctx));
    let heartbeat = async {
        let mut ticker = tokio::time::interval(config.lease / 3);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match queue::heartbeat(pool, job, config.lease).await {
                Ok(true) => {}
                Ok(false) => return,
                Err(err) => tracing::warn!(job_id = job.id, error = %err, "heartbeat failed"),
            }
        }
    };
    let outcome = tokio::select! {
        result = tokio::time::timeout(handler.timeout, work) => match result {
            Ok(Ok(result)) => result,
            Ok(Err(join)) => Err(JobError::retry(anyhow::anyhow!("handler panicked: {join}"))),
            Err(_) => Err(JobError::retry(anyhow::anyhow!("timed out after {:?}", handler.timeout))),
        },
        () = heartbeat => {
            tracing::warn!(job_id = job.id, kind = %job.kind, "lease lost; abandoning the result");
            return;
        }
    };
    record(pool, job, outcome).await;
}

async fn record(pool: &PgPool, job: &ClaimedJob, outcome: Result<(), JobError>) {
    let result = match &outcome {
        Ok(()) => queue::complete(pool, job.id, job.claim_id).await.map(|done| {
            if done { None } else { Some(Finished::LostLease) }
        }),
        Err(JobError::Retry(err)) => {
            let delay = retry_delay(job.attempts, jitter());
            queue::retry(pool, job, &format!("{err:#}"), delay).await.map(Some)
        }
        Err(JobError::Permanent(err)) => queue::fail(pool, job, &format!("{err:#}")).await.map(Some),
        Err(JobError::Reschedule(delay)) => queue::reschedule(pool, job, *delay).await.map(|_| None),
    };
    match (result, &outcome) {
        (Err(err), _) => tracing::error!(job_id = job.id, error = %err, "could not record a job outcome"),
        (Ok(Some(Finished::Failed)), Err(err)) => {
            tracing::error!(job_id = job.id, kind = %job.kind, attempts = job.attempts, error = %err, "job failed")
        }
        (Ok(Some(Finished::Requeued)), Err(err)) => {
            tracing::warn!(job_id = job.id, kind = %job.kind, attempt = job.attempts, error = %err, "job will be retried")
        }
        (Ok(Some(Finished::LostLease)), _) => tracing::warn!(job_id = job.id, "job outcome dropped: lease lost"),
        _ => tracing::debug!(job_id = job.id, kind = %job.kind, "job done"),
    }
}
```

Notes:
- `select!` with `listener.recv()`: `PgListener` reconnects on its own; `recv` errors are logged and the poll interval covers the gap.
- The abort path in `drain` drops handler tasks mid-flight; a handler must therefore never leave partial state that a re-run cannot fix (all Phase 3 handlers write files to `tmp/` and rename, and update rows in one statement).

**Step 4: Run** — `cargo test -p squirrel-jobs` → PASS (all, including the 5 worker tests).

**Step 5: Commit** — `feat(jobs): add the worker runtime with heartbeats, timeouts and graceful shutdown`.

---

### Task 6: `squirrel worker` and `squirrel serve --worker`

**Files:**
- Create: `crates/squirrel/src/worker.rs`
- Modify: `crates/squirrel/src/main.rs`, `crates/squirrel/Cargo.toml` (add `squirrel-jobs`, `serde_json`)

**Step 1: Write the failing tests** — unit tests in `worker.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_concurrency_is_half_the_cores_capped_at_two() {
        assert_eq!(default_concurrency(1), 1);
        assert_eq!(default_concurrency(2), 1);
        assert_eq!(default_concurrency(4), 2);
        assert_eq!(default_concurrency(16), 2);
    }

    #[test]
    fn worker_names_carry_host_and_pid() {
        let name = worker_name();
        assert!(name.contains(':') && name.len() <= 200, "{name}");
    }
}
```

**Step 2: Run** → FAIL.

**Step 3: Implement.**
- `WorkerArgs` (clap `Args`): `#[command(flatten)] database: DatabaseArgs`, `--data-dir` (`SQUIRREL_DATA_DIR`), `--concurrency` (`SQUIRREL_WORKER_CONCURRENCY`, default `default_concurrency(available_parallelism)` = `(min(cores, 4) / 2).max(1)`), plus the media options added in Milestone B (`--pdfium-dir`, `--geodata-file`, `--media-exec`). Milestone A registers no handler except a built-in `noop` kind (used by tests and `squirrel admin jobs ping`), so the subcommand is useful on its own.
- `Command::Worker(WorkerArgs)` in `main.rs` → `init_tracing(args.log_json)`, `connect_waiting_for_migrations(&args.database)`: loop `pending_migrations` every 5 s, logging `waiting for migrations` once per minute, giving up after 10 minutes with the same message as `admin`; then `run_worker(pool, registry, config, shutdown_signal)`.
- `ServeArgs` gains `#[arg(long, env = "SQUIRREL_EMBEDDED_WORKER", default_value_t = false, action = ArgAction::Set)] worker: bool` and the same worker options; when set, `serve` spawns the worker loop on the same pool after migrations, and the existing shutdown path sends `true` on the worker's `watch` channel and awaits it (bounded by `shutdown_grace`).
- `worker_name()` = `format!("{}:{}", hostname, std::process::id())` with the hostname from `HOSTNAME`/`/etc/hostname`, else `"worker"`; truncated to 200 chars.

**Step 4: Run** — `cargo test -p squirrel` → PASS; manually: `cargo run -p squirrel -- worker --help` lists the options; with the dev database, `cargo run -p squirrel -- worker` logs `worker started` and stops cleanly on Ctrl-C.

**Step 5: Commit** — `feat: add the worker subcommand and an embedded worker for serve`.

---

### Task 7: Admin queue API, CLI and failed-job retention

**Files:**
- Create: `crates/squirrel-api/src/routes/jobs.rs`, `crates/squirrel-api/tests/admin_jobs.rs`
- Modify: `crates/squirrel-api/src/routes/mod.rs`, `crates/squirrel-api/src/lib.rs` (routes + `tags(... (name = "jobs"))`), `crates/squirrel-api/Cargo.toml` (add `squirrel-jobs`), `crates/squirrel/src/admin.rs`, `crates/squirrel/src/tasks.rs`, `openapi.json`, `web/src/lib/api/schema.d.ts`

**Step 1: Write the failing tests** (`tests/admin_jobs.rs`, using `common::two_users`): Alice (admin) `GET /api/admin/jobs` → 200 `{kinds: [{kind, queued, running, failed}], failures: [{id, kind, attempts, last_error, updated_at}]}` after inserting a queued and a failed job with raw SQL; Bob → 403 `forbidden`; `POST /api/admin/jobs/{id}/retry` (CSRF) → 204 and the job is queued again; unknown id → 404; missing CSRF → 403 `csrf_invalid`.

**Step 2: Run** → FAIL.

**Step 3: Implement** the two handlers (`current.require_admin()?` first, as `routes/admin.rs` does), DTOs `JobSummaryDto`, `FailedJobDto`, `JobsOverview`. Register in `json_routes`. CLI: `AdminCommand::Jobs { #[command(subcommand)] action: Option<JobsAction> }` with `JobsAction::Retry { id: i64 }`; plain `squirrel admin jobs` prints the summary table and the last 20 failures. `tasks.rs`: the 6-hourly storage job also calls `squirrel_jobs::admin::purge_failed_older_than(now − 30 days)`. Regenerate `openapi.json` and `schema.d.ts`.

**Step 4: Run** — `cargo test --workspace` → PASS; `pnpm check` in `web/` → PASS (schema only).

**Step 5: Commit** — `feat(api): expose the job queue to administrators`.

**Milestone A review checkpoint:** `cargo test --workspace` green; run `squirrel serve --worker` against the dev DB, insert `INSERT INTO jobs (kind) VALUES ('noop')` with `docker compose -f deploy/docker-compose.dev.yml exec postgres psql -U squirrel -c …`, and see it disappear within a second (LISTEN/NOTIFY) in the logs.

---

## Milestone B — Media extraction & thumbnails

### Task 8: `squirrel-media` crate, test fixtures and EXIF metadata

**Files:**
- Create: `crates/squirrel-media/Cargo.toml`, `src/lib.rs`, `src/metadata.rs`, `src/testutil.rs`
- Create: `crates/squirrel-media/tests/fixtures/` (copy `exif.heic`, `exif.png`, `exif.webp` from the `kamadak-exif` 0.6.2 crate's `tests/` directory — BSD-2-Clause, add a `README.md` there saying so)
- Modify: `Cargo.toml` (workspace deps `kamadak-exif = "0.6.2"`, `image = { version = "0.25.10", default-features = false, features = ["jpeg", "png", "gif", "webp", "tiff", "bmp"] }`, `squirrel-media = { path = "crates/squirrel-media" }`)

`Cargo.toml`:
```toml
[package]
name = "squirrel-media"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[features]
# Exposes `testutil` (fixture generators) to other crates' tests and the fixtures example.
test-util = []
# HEIC/HEIF (and AVIF) decoding through the system libheif (>= 1.17).
heic = ["dep:libheif-rs"]

[dependencies]
kamadak-exif.workspace = true
image.workspace = true
serde.workspace = true
thiserror.workspace = true
time.workspace = true

[dev-dependencies]
```
(`fast_image_resize`, `webp`, `blurhash`, `pdfium-render`, `libheif-rs` are added by the tasks that use them.)

`lib.rs`:
```rust
//! Media understanding without I/O policy: EXIF, capture times, decoding,
//! thumbnails, blurhash, PDFs and reverse geocoding. Callers decide where bytes
//! come from and where results go.

pub mod metadata;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;
```

**Step 1: Write the failing tests** — `metadata.rs` with only its test module (below) and `testutil.rs` in full (it is test infrastructure, written first).

`src/testutil.rs` (verified):

```rust
//! Test fixtures generated in code: no binary files with unclear licences.

use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::{ImageBuffer, Rgb};

type Dms = (u32, u32, u32);

#[derive(Debug, Default, Clone)]
pub struct ExifSpec {
    pub date_time_original: Option<&'static str>,
    pub offset_time_original: Option<&'static str>,
    /// (latitude d/m/centiseconds, 'N'|'S', longitude d/m/centiseconds, 'E'|'W').
    pub gps: Option<(Dms, u8, Dms, u8)>,
    pub make: Option<&'static str>,
    pub model: Option<&'static str>,
    pub orientation: Option<u16>,
    pub user_comment: Option<&'static str>,
}

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn dms((d, m, cs): Dms) -> Value {
    Value::Rational(vec![
        Rational { num: d, denom: 1 },
        Rational { num: m, denom: 1 },
        Rational {
            num: cs,
            denom: 100,
        },
    ])
}

/// The TIFF-structured EXIF payload for `spec`.
/// `None` when `spec` sets no field (the writer refuses an empty IFD).
pub fn exif_payload(spec: &ExifSpec) -> Option<Vec<u8>> {
    let mut fields = Vec::new();
    if let Some(v) = spec.date_time_original {
        fields.push(ascii(Tag::DateTimeOriginal, v));
    }
    if let Some(v) = spec.offset_time_original {
        fields.push(ascii(Tag::OffsetTimeOriginal, v));
    }
    if let Some(v) = spec.make {
        fields.push(ascii(Tag::Make, v));
    }
    if let Some(v) = spec.model {
        fields.push(ascii(Tag::Model, v));
    }
    if let Some(v) = spec.orientation {
        fields.push(Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![v]),
        });
    }
    if let Some(v) = spec.user_comment {
        let mut bytes = b"ASCII\0\0\0".to_vec();
        bytes.extend_from_slice(v.as_bytes());
        fields.push(Field {
            tag: Tag::UserComment,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(bytes, 0),
        });
    }
    if let Some((lat, lat_ref, lon, lon_ref)) = spec.gps {
        fields.push(Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: dms(lat),
        });
        fields.push(ascii(Tag::GPSLatitudeRef, &(lat_ref as char).to_string()));
        fields.push(Field {
            tag: Tag::GPSLongitude,
            ifd_num: In::PRIMARY,
            value: dms(lon),
        });
        fields.push(ascii(Tag::GPSLongitudeRef, &(lon_ref as char).to_string()));
    }
    if fields.is_empty() {
        return None;
    }
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out, false).expect("exif payload");
    Some(out.into_inner())
}

/// A small RGB JPEG with an APP1 EXIF segment built from `spec` (no segment when empty).
pub fn jpeg_with_exif(spec: &ExifSpec) -> Vec<u8> {
    jpeg_sized_with_exif(32, 24, spec)
}

pub fn jpeg_sized_with_exif(width: u32, height: u32, spec: &ExifSpec) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x * 7) as u8, (y * 5) as u8, 128])
    });
    let mut jpeg = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .expect("jpeg");
    let Some(payload) = exif_payload(spec) else {
        return jpeg;
    };
    let mut segment = Vec::with_capacity(payload.len() + 10);
    segment.extend_from_slice(&[0xFF, 0xE1]);
    let len = u16::try_from(payload.len() + 8).expect("small exif");
    segment.extend_from_slice(&len.to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&payload);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&segment);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// A valid one-page PDF (A4 portrait) whose page shows `text` in Helvetica.
pub fn pdf_with_text(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] \
         /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[test]
fn generated_pdfs_have_a_consistent_xref() {
    let pdf = pdf_with_text("Hello");
    let text = String::from_utf8(pdf).unwrap();
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(text[start..].starts_with("xref\n0 6\n"));
    let obj5 = text.find("5 0 obj").unwrap();
    assert!(text.contains(&format!("{obj5:010} 00000 n ")));
}
```

**Step 2: Run** — `cargo test -p squirrel-media` → FAIL (`read_metadata` not found).

**Step 3: Implement** — `src/metadata.rs` (verified; tests included):

```rust
//! EXIF metadata of an image file (JPEG, HEIC/HEIF, PNG, WebP, TIFF).

use std::io::{BufRead, Seek};

use exif::{Exif, Field, In, Tag, Value};
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

/// When the photo was taken, as the camera recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureTime {
    /// Wall-clock time where the photo was taken.
    pub local: PrimitiveDateTime,
    /// UTC offset, when the file records one (`OffsetTimeOriginal`, EXIF 2.31).
    pub offset: Option<UtcOffset>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsPosition {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres above sea level.
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageMetadata {
    pub taken: Option<CaptureTime>,
    pub gps: Option<GpsPosition>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    /// EXIF orientation 1..=8 (1 = upright).
    pub orientation: Option<u16>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    pub focal_length_mm: Option<f64>,
    /// `UserComment` or `ImageDescription` says "Screenshot" (iOS, some Androids).
    pub says_screenshot: bool,
}

/// Longest text value kept (make, model, lens); EXIF strings are untrusted.
const MAX_TEXT: usize = 128;

/// Parses EXIF from `reader`. A file without EXIF gives the default (empty)
/// metadata; only I/O errors are errors.
pub fn read_metadata<R: BufRead + Seek>(reader: &mut R) -> std::io::Result<ImageMetadata> {
    match exif::Reader::new().read_from_container(reader) {
        Ok(exif) => Ok(from_exif(&exif)),
        Err(exif::Error::Io(err)) => Err(err),
        Err(_) => Ok(ImageMetadata::default()),
    }
}

fn from_exif(exif: &Exif) -> ImageMetadata {
    let field = |tag| exif.get_field(tag, In::PRIMARY);
    ImageMetadata {
        taken: capture_time(exif),
        gps: gps(exif),
        camera_make: field(Tag::Make).and_then(text),
        camera_model: field(Tag::Model).and_then(text),
        lens_model: field(Tag::LensModel).and_then(text),
        orientation: field(Tag::Orientation)
            .and_then(|f| f.value.get_uint(0))
            .and_then(|o| u16::try_from(o).ok())
            .filter(|o| (1..=8).contains(o)),
        exposure_time: field(Tag::ExposureTime)
            .and_then(|f| rational(&f.value))
            .filter(|s| *s > 0.0)
            .map(format_exposure),
        f_number: field(Tag::FNumber).and_then(|f| rational(&f.value)),
        iso: field(Tag::PhotographicSensitivity).and_then(|f| f.value.get_uint(0)),
        focal_length_mm: field(Tag::FocalLength).and_then(|f| rational(&f.value)),
        says_screenshot: [Tag::UserComment, Tag::ImageDescription]
            .into_iter()
            .filter_map(field)
            .any(|f| raw_text(f).to_ascii_lowercase().contains("screenshot")),
    }
}

/// `DateTimeOriginal`, else `DateTimeDigitized`, else `DateTime`, with the
/// matching sub-seconds and offset tags when present.
fn capture_time(exif: &Exif) -> Option<CaptureTime> {
    const CANDIDATES: [(Tag, Tag, Tag); 3] = [
        (
            Tag::DateTimeOriginal,
            Tag::SubSecTimeOriginal,
            Tag::OffsetTimeOriginal,
        ),
        (
            Tag::DateTimeDigitized,
            Tag::SubSecTimeDigitized,
            Tag::OffsetTimeDigitized,
        ),
        (Tag::DateTime, Tag::SubSecTime, Tag::OffsetTime),
    ];
    CANDIDATES.iter().find_map(|&(main, subsec, offset)| {
        let mut dt = exif::DateTime::from_ascii(first_ascii(exif, main)?).ok()?;
        if let Some(raw) = first_ascii(exif, subsec) {
            let _ = dt.parse_subsec(raw);
        }
        if let Some(raw) = first_ascii(exif, offset) {
            let _ = dt.parse_offset(raw);
        }
        to_capture_time(&dt)
    })
}

/// The first string of an ASCII field of the primary image.
fn first_ascii(exif: &Exif, tag: Tag) -> Option<&[u8]> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(parts) => parts.first().map(Vec::as_slice),
        _ => None,
    }
}

fn to_capture_time(dt: &exif::DateTime) -> Option<CaptureTime> {
    // Cameras without a set clock write 0000:00:00 or 1970/2000 defaults.
    if dt.year < 1900 {
        return None;
    }
    let date =
        Date::from_calendar_date(i32::from(dt.year), Month::try_from(dt.month).ok()?, dt.day)
            .ok()?;
    let time =
        Time::from_hms_nano(dt.hour, dt.minute, dt.second, dt.nanosecond.unwrap_or(0)).ok()?;
    let offset = dt
        .offset
        .and_then(|minutes| UtcOffset::from_whole_seconds(i32::from(minutes) * 60).ok());
    Some(CaptureTime {
        local: PrimitiveDateTime::new(date, time),
        offset,
    })
}

fn gps(exif: &Exif) -> Option<GpsPosition> {
    let field = |tag| exif.get_field(tag, In::PRIMARY);
    let latitude = signed(
        dms(field(Tag::GPSLatitude)?)?,
        field(Tag::GPSLatitudeRef),
        b'S',
    )?;
    let longitude = signed(
        dms(field(Tag::GPSLongitude)?)?,
        field(Tag::GPSLongitudeRef),
        b'W',
    )?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    // (0, 0) is what phones write when they had no fix.
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }
    let altitude = field(Tag::GPSAltitude)
        .and_then(|f| rational(&f.value))
        .map(|alt| {
            let below = field(Tag::GPSAltitudeRef).and_then(|f| f.value.get_uint(0)) == Some(1);
            if below { -alt } else { alt }
        });
    Some(GpsPosition {
        latitude,
        longitude,
        altitude,
    })
}

/// Degrees, minutes, seconds → decimal degrees.
fn dms(field: &Field) -> Option<f64> {
    let Value::Rational(ref parts) = field.value else {
        return None;
    };
    let value = |i: usize| parts.get(i).filter(|r| r.denom != 0).map(|r| r.to_f64());
    let degrees = value(0)? + value(1).unwrap_or(0.0) / 60.0 + value(2).unwrap_or(0.0) / 3600.0;
    degrees.is_finite().then_some(degrees)
}

fn signed(value: f64, reference: Option<&Field>, negative: u8) -> Option<f64> {
    let is_negative = match reference.map(|f| &f.value) {
        Some(Value::Ascii(parts)) => parts.first().and_then(|p| p.first()) == Some(&negative),
        _ => false,
    };
    Some(if is_negative { -value } else { value })
}

fn rational(value: &Value) -> Option<f64> {
    match value {
        Value::Rational(parts) => parts.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        Value::SRational(parts) => parts.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// `1/250` for fast shutters, `2.5 s` for slow ones.
fn format_exposure(seconds: f64) -> String {
    if seconds >= 1.0 {
        format!("{} s", (seconds * 10.0).round() / 10.0)
    } else {
        format!("1/{}", (1.0 / seconds).round())
    }
}

/// Printable text of an ASCII or UNDEFINED field, trimmed and length-capped.
fn text(field: &Field) -> Option<String> {
    let value = raw_text(field);
    let cleaned: String = value
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn raw_text(field: &Field) -> String {
    match &field.value {
        Value::Ascii(parts) => parts
            .first()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .unwrap_or_default(),
        // UserComment: 8-byte character code, then the text.
        Value::Undefined(bytes, _) => {
            String::from_utf8_lossy(bytes.get(8..).unwrap_or_default()).into_owned()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_with_exif};
    use std::io::Cursor;
    use time::macros::{datetime, offset};

    #[test]
    fn reads_capture_time_with_offset_and_gps() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            date_time_original: Some("2024:07:14 18:30:05"),
            offset_time_original: Some("+02:00"),
            gps: Some(((48, 51, 2400), b'N', (2, 21, 300), b'E')),
            make: Some("Google"),
            model: Some("Pixel 8"),
            orientation: Some(6),
            ..ExifSpec::default()
        });
        let meta = read_metadata(&mut Cursor::new(jpeg)).unwrap();
        assert_eq!(
            meta.taken,
            Some(CaptureTime {
                local: datetime!(2024-07-14 18:30:05),
                offset: Some(offset!(+2)),
            })
        );
        let gps = meta.gps.unwrap();
        assert!((gps.latitude - 48.8566).abs() < 1e-3, "{gps:?}");
        assert!((gps.longitude - 2.3508).abs() < 1e-3, "{gps:?}");
        assert_eq!(meta.camera_make.as_deref(), Some("Google"));
        assert_eq!(meta.camera_model.as_deref(), Some("Pixel 8"));
        assert_eq!(meta.orientation, Some(6));
    }

    #[test]
    fn southern_and_western_coordinates_are_negative() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            gps: Some(((33, 52, 0), b'S', (151, 12, 0), b'W')),
            ..ExifSpec::default()
        });
        let gps = read_metadata(&mut Cursor::new(jpeg)).unwrap().gps.unwrap();
        assert!(gps.latitude < 0.0 && gps.longitude < 0.0, "{gps:?}");
    }

    #[test]
    fn null_island_and_unset_clocks_are_ignored() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            date_time_original: Some("0000:00:00 00:00:00"),
            gps: Some(((0, 0, 0), b'N', (0, 0, 0), b'E')),
            ..ExifSpec::default()
        });
        let meta = read_metadata(&mut Cursor::new(jpeg)).unwrap();
        assert_eq!((meta.taken, meta.gps), (None, None));
    }

    #[test]
    fn files_without_exif_give_empty_metadata() {
        let jpeg = jpeg_with_exif(&ExifSpec::default());
        assert_eq!(
            read_metadata(&mut Cursor::new(b"not an image".to_vec())).unwrap(),
            ImageMetadata::default()
        );
        assert!(
            read_metadata(&mut Cursor::new(jpeg))
                .unwrap()
                .taken
                .is_none()
        );
    }

    #[test]
    fn screenshot_comments_are_detected() {
        let jpeg = jpeg_with_exif(&ExifSpec {
            user_comment: Some("Screenshot"),
            ..ExifSpec::default()
        });
        assert!(
            read_metadata(&mut Cursor::new(jpeg))
                .unwrap()
                .says_screenshot
        );
    }

    #[test]
    fn heic_png_and_webp_containers_are_read() {
        for name in ["exif.heic", "exif.png", "exif.webp"] {
            let bytes = std::fs::read(format!("tests/fixtures/{name}")).unwrap();
            // The kamadak fixtures carry a few fields but no capture time: parsing must succeed.
            let parsed = exif::Reader::new().read_from_container(&mut Cursor::new(bytes));
            assert!(parsed.is_ok(), "{name}: {:?}", parsed.err());
        }
    }

    #[test]
    fn exposure_is_formatted_like_cameras_do() {
        assert_eq!(format_exposure(0.004), "1/250");
        assert_eq!(format_exposure(2.5), "2.5 s");
    }
}
```

The HEIC/PNG/WebP fixture test reads `tests/fixtures/…` relative to the crate root (cargo runs tests there).

**Step 4: Run** — `cargo test -p squirrel-media` → PASS (8 tests).

**Step 5: Commit**
```bash
git add Cargo.toml Cargo.lock crates/squirrel-media
git commit -m "feat(media): read capture time, GPS and camera details from EXIF" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Capture time chain, screenshots and `MediaKind`

**Files:**
- Create: `crates/squirrel-media/src/capture.rs` (verified code below)
- Create: `crates/squirrel-core/src/media_kind.rs`; Modify: `crates/squirrel-core/src/lib.rs`

**Step 1: Write the failing tests.** `capture.rs` tests (part of the verified file below). `media_kind.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_types() {
        assert_eq!(MediaKind::for_mime("image/jpeg"), Some(MediaKind::Image));
        assert_eq!(MediaKind::for_mime("image/heic"), Some(MediaKind::Image));
        assert_eq!(MediaKind::for_mime("application/pdf"), Some(MediaKind::Pdf));
        assert_eq!(MediaKind::for_mime("text/markdown"), Some(MediaKind::Text));
        assert_eq!(MediaKind::for_mime("image/svg+xml"), None, "active content is never decoded");
        assert_eq!(MediaKind::for_mime("video/mp4"), None, "Phase 4");
        assert_eq!(MediaKind::for_mime("application/zip"), None);
    }

    #[test]
    fn heif_family_needs_the_heic_feature() {
        assert!(MediaKind::is_heif_family("image/heic"));
        assert!(MediaKind::is_heif_family("image/avif"));
        assert!(!MediaKind::is_heif_family("image/jpeg"));
    }

    #[test]
    fn round_trips_through_its_database_name() {
        for kind in [MediaKind::Image, MediaKind::Pdf, MediaKind::Text] {
            assert_eq!(kind.as_str().parse::<MediaKind>(), Ok(kind));
        }
    }
}
```

**Step 2: Run** — `cargo test -p squirrel-core -p squirrel-media` → FAIL.

**Step 3: Implement.** `media_kind.rs`:

```rust
//! Which stored files the media pipeline processes, by their sniffed MIME type.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Pdf,
    Text,
}

const IMAGES: &[&str] = &[
    "image/jpeg", "image/png", "image/gif", "image/webp", "image/tiff", "image/bmp",
    "image/heic", "image/heif", "image/avif",
];
const HEIF_FAMILY: &[&str] = &["image/heic", "image/heif", "image/avif"];
const TEXTS: &[&str] = &["text/plain", "text/markdown"];

impl MediaKind {
    pub fn for_mime(mime: &str) -> Option<Self> {
        let essence = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
        if IMAGES.contains(&essence.as_str()) {
            Some(Self::Image)
        } else if essence == "application/pdf" {
            Some(Self::Pdf)
        } else if TEXTS.contains(&essence.as_str()) {
            Some(Self::Text)
        } else {
            None
        }
    }

    /// Decodable only when the server is built with the `heic` feature.
    pub fn is_heif_family(mime: &str) -> bool {
        HEIF_FAMILY.contains(&mime)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }
}

impl FromStr for MediaKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "image" => Ok(Self::Image),
            "pdf" => Ok(Self::Pdf),
            "text" => Ok(Self::Text),
            _ => Err(()),
        }
    }
}
```

`crates/squirrel-media/src/capture.rs` (verified, 5 tests):

```rust
//! Which capture time and kind a photo gets: the `taken_at` fallback chain and
//! screenshot detection. Pure functions over what extraction found.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use crate::metadata::{CaptureTime, ImageMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenSource {
    Exif,
    Filename,
    Upload,
}

impl TakenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exif => "exif",
            Self::Filename => "filename",
            Self::Upload => "upload",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    pub local: PrimitiveDateTime,
    pub offset: Option<UtcOffset>,
    pub source: TakenSource,
}

/// EXIF, else a date in the file name, else the upload time (UTC). Candidates
/// more than a day in the future (wrong camera clock) are skipped.
pub fn resolve_taken(
    exif: Option<CaptureTime>,
    file_name: &str,
    uploaded_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Taken {
    let limit = now + time::Duration::days(1);
    let plausible = |local: PrimitiveDateTime, offset: Option<UtcOffset>| {
        local.assume_offset(offset.unwrap_or(UtcOffset::UTC)) <= limit
    };
    if let Some(capture) = exif.filter(|c| plausible(c.local, c.offset)) {
        return Taken {
            local: capture.local,
            offset: capture.offset,
            source: TakenSource::Exif,
        };
    }
    if let Some(local) = date_in_name(file_name).filter(|local| plausible(*local, None)) {
        return Taken {
            local,
            offset: None,
            source: TakenSource::Filename,
        };
    }
    let utc = uploaded_at.to_offset(UtcOffset::UTC);
    Taken {
        local: PrimitiveDateTime::new(utc.date(), utc.time()),
        offset: Some(UtcOffset::UTC),
        source: TakenSource::Upload,
    }
}

/// A capture date embedded in common phone and app file names:
/// `IMG_20240714_183005`, `PXL_20240714_183005123`, `VID_…`, `Screenshot_2024-07-14-18-30-05`,
/// `Screenshot 2024-07-14 at 18.30.05`, `2024-07-14 18.30.05`, `IMG-20240714-WA0001` (date only: noon).
pub fn date_in_name(name: &str) -> Option<PrimitiveDateTime> {
    let digits: Vec<(usize, u32)> = name
        .char_indices()
        .filter_map(|(i, c)| c.to_digit(10).map(|d| (i, d)))
        .collect();
    // Find the first run that starts with a plausible year and yields a date.
    (0..digits.len()).find_map(|start| parse_from(&digits[start..], name))
}

/// Reads YYYY MM DD [hh mm ss] from consecutive digit groups separated by at
/// most one non-digit character each (`-`, `_`, `.`, ` `, `:`), as in names.
fn parse_from(digits: &[(usize, u32)], name: &str) -> Option<PrimitiveDateTime> {
    let bytes = name.as_bytes();
    let mut values = Vec::with_capacity(14);
    let mut last: Option<usize> = None;
    for &(index, digit) in digits {
        if let Some(prev) = last {
            let gap = index - prev - 1;
            if gap > 1 || (gap == 1 && !b"-_. :T".contains(&bytes[prev + 1])) {
                // " at " between date and time (macOS screenshots).
                if !(gap == 4 && &bytes[prev + 1..index] == b" at ") {
                    break;
                }
            }
        }
        values.push(digit);
        last = Some(index);
        if values.len() == 14 {
            break;
        }
    }
    if values.len() < 8 {
        return None;
    }
    let number = |range: std::ops::Range<usize>| range.fold(0u32, |acc, i| acc * 10 + values[i]);
    let year = i32::try_from(number(0..4)).ok()?;
    if !(1990..=2100).contains(&year) {
        return None;
    }
    let date = Date::from_calendar_date(
        year,
        Month::try_from(u8::try_from(number(4..6)).ok()?).ok()?,
        u8::try_from(number(6..8)).ok()?,
    )
    .ok()?;
    let time = if values.len() >= 14 {
        Time::from_hms(
            u8::try_from(number(8..10)).ok()?,
            u8::try_from(number(10..12)).ok()?,
            u8::try_from(number(12..14)).ok()?,
        )
        .ok()?
    } else {
        Time::from_hms(12, 0, 0).ok()?
    };
    Some(PrimitiveDateTime::new(date, time))
}

const SCREENSHOT_PREFIXES: [&str; 6] = [
    "screenshot",
    "screen shot",
    "capture d’écran",
    "capture d'écran",
    "bildschirmfoto",
    "schermafbeelding",
];
/// Phone screens are at least 19.5:9 ≈ 2.17; 1.9 leaves room for status bars cropped off.
const SCREEN_ASPECT: f64 = 1.9;

/// Screenshot: the EXIF says so, the name says so, or a PNG without a camera
/// that has a phone screen's shape.
pub fn is_screenshot(
    meta: &ImageMetadata,
    file_name: &str,
    mime: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> bool {
    let lower = file_name.to_lowercase();
    if meta.says_screenshot
        || SCREENSHOT_PREFIXES.iter().any(|p| lower.starts_with(p))
        || lower.starts_with("captura de pantalla")
    {
        return true;
    }
    let no_camera = meta.camera_make.is_none() && meta.camera_model.is_none();
    match (mime, width, height) {
        ("image/png", Some(w), Some(h)) if no_camera && w > 0 && h > 0 => {
            let ratio = f64::from(w.max(h)) / f64::from(w.min(h));
            ratio >= SCREEN_ASPECT
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{datetime, offset};

    const NOW: OffsetDateTime = datetime!(2026-11-14 12:00 UTC);

    #[test]
    fn names_from_phones_and_apps() {
        let cases = [
            ("IMG_20240714_183005.jpg", datetime!(2024-07-14 18:30:05)),
            ("PXL_20240714_183005123.jpg", datetime!(2024-07-14 18:30:05)),
            (
                "Screenshot_2024-07-14-18-30-05.png",
                datetime!(2024-07-14 18:30:05),
            ),
            (
                "Screenshot 2024-07-14 at 18.30.05.png",
                datetime!(2024-07-14 18:30:05),
            ),
            ("2024-07-14 18.30.05.jpg", datetime!(2024-07-14 18:30:05)),
            ("IMG-20240714-WA0001.jpg", datetime!(2024-07-14 12:00:00)),
        ];
        for (name, expected) in cases {
            assert_eq!(date_in_name(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn names_without_dates_give_none() {
        for name in [
            "holiday.jpg",
            "DSC_0042.JPG",
            "IMG_1234.HEIC",
            "20241399_000000.jpg",
            "18000101_101010.jpg",
        ] {
            assert_eq!(date_in_name(name), None, "{name}");
        }
    }

    #[test]
    fn exif_wins_then_name_then_upload() {
        let exif = CaptureTime {
            local: datetime!(2023-01-02 03:04:05),
            offset: Some(offset!(+1)),
        };
        let uploaded = datetime!(2026-11-01 10:00 UTC);
        let taken = resolve_taken(Some(exif), "IMG_20240714_183005.jpg", uploaded, NOW);
        assert_eq!((taken.local, taken.source), (exif.local, TakenSource::Exif));
        let taken = resolve_taken(None, "IMG_20240714_183005.jpg", uploaded, NOW);
        assert_eq!(
            (taken.local, taken.offset, taken.source),
            (datetime!(2024-07-14 18:30:05), None, TakenSource::Filename)
        );
        let taken = resolve_taken(None, "holiday.jpg", uploaded, NOW);
        assert_eq!(
            (taken.local, taken.source),
            (datetime!(2026-11-01 10:00), TakenSource::Upload)
        );
    }

    #[test]
    fn future_dates_are_skipped() {
        let future = CaptureTime {
            local: datetime!(2030-01-01 00:00),
            offset: None,
        };
        let taken = resolve_taken(Some(future), "x.jpg", datetime!(2026-11-01 10:00 UTC), NOW);
        assert_eq!(taken.source, TakenSource::Upload);
    }

    #[test]
    fn screenshots_by_comment_name_or_shape() {
        let plain = ImageMetadata::default();
        let commented = ImageMetadata {
            says_screenshot: true,
            ..ImageMetadata::default()
        };
        assert!(is_screenshot(
            &commented,
            "IMG_1.jpg",
            "image/jpeg",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "Screenshot_20240714.png",
            "image/png",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "Capture d’écran 2024-07-14.png",
            "image/png",
            None,
            None
        ));
        assert!(is_screenshot(
            &plain,
            "x.png",
            "image/png",
            Some(1080),
            Some(2400)
        ));
        assert!(!is_screenshot(
            &plain,
            "x.png",
            "image/png",
            Some(1200),
            Some(900)
        ));
        let camera = ImageMetadata {
            camera_make: Some("Apple".into()),
            ..ImageMetadata::default()
        };
        assert!(!is_screenshot(
            &camera,
            "pano.png",
            "image/png",
            Some(4000),
            Some(1000)
        ));
        assert!(!is_screenshot(
            &plain,
            "pano.jpg",
            "image/jpeg",
            Some(4000),
            Some(1000)
        ));
    }
}
```

Add `pub mod capture;` to `lib.rs`; add `time = { workspace = true }` with the `macros` feature already enabled in the workspace.

**Step 4: Run** → PASS.

**Step 5: Commit** — `feat(media): resolve capture times and detect screenshots`.

---

### Task 10: Bounded decoding, variants, WebP and blurhash (+ optional HEIC)

**Files:**
- Create: `crates/squirrel-media/src/thumbs.rs` (verified), `crates/squirrel-media/src/render.rs`, `crates/squirrel-media/src/heic.rs`
- Modify: `crates/squirrel-media/Cargo.toml`, `Cargo.toml` (workspace deps `fast_image_resize = { version = "6.1.0", features = ["image"] }`, `webp = { version = "0.3.1", default-features = false }`, `blurhash = { version = "0.2.3", default-features = false }`, `libheif-rs = { version = "3.0.0", default-features = false, features = ["v1_17", "image"] }`)

`squirrel-media/Cargo.toml` additions: `fast_image_resize.workspace = true`, `webp.workspace = true`, `blurhash.workspace = true`, `libheif-rs = { workspace = true, optional = true }`, `serde_json.workspace = true` (protocol, Task 12), dev: `tempfile.workspace = true`.

**Step 1: Write the failing tests** — the test module of `thumbs.rs` (verified file below), plus `render.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};

    #[test]
    fn writes_both_variants_and_reports_displayed_size() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(2000, 1000, &ExifSpec { orientation: Some(6), ..ExifSpec::default() })).unwrap();
        let out = tempfile::tempdir().unwrap();
        let rendered = render_image(&input, out.path(), DecodeLimits::default()).unwrap();
        assert_eq!((rendered.width, rendered.height), (1000, 2000), "rotated");
        let thumb = image::open(out.path().join(THUMB_FILE)).unwrap();
        let preview = image::open(out.path().join(PREVIEW_FILE)).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (180, 360));
        assert_eq!((preview.width(), preview.height()), (720, 1440));
        assert!(!rendered.blurhash.is_empty());
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.jpg");
        std::fs::write(&input, jpeg_sized_with_exif(120, 80, &ExifSpec::default())).unwrap();
        let rendered = render_image(&input, dir.path(), DecodeLimits::default()).unwrap();
        assert_eq!((rendered.width, rendered.height), (120, 80));
        assert_eq!(image::open(dir.path().join(THUMB_FILE)).unwrap().height(), 80);
    }
}
```

**Step 2: Run** → FAIL.

**Step 3: Implement.** `thumbs.rs` (verified; add `#[derive(serde::Serialize, serde::Deserialize)]` to `DecodeLimits`, which the sandbox protocol sends):

```rust
//! Decoding (bounded), orientation, resizing, WebP encoding and blurhash.

use std::io::{BufRead, Seek};

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

/// Decoding limits for untrusted images.
#[derive(Debug, Clone, Copy)]
pub struct DecodeLimits {
    /// Width × height above which an image is refused before decoding.
    pub max_pixels: u64,
    /// Largest single allocation the decoder may make.
    pub max_alloc: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_pixels: 100_000_000,       // 100 MP: every phone and most cameras.
            max_alloc: 1024 * 1024 * 1024, // 1 GiB.
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    #[error("image of {0}×{1} pixels exceeds the limit")]
    TooLarge(u32, u32),
    #[error("unsupported or corrupt image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("resize failed: {0}")]
    Resize(String),
    #[error("blurhash failed: {0}")]
    Blurhash(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// A decoded image, upright (EXIF orientation applied), with its stored size.
pub struct Decoded {
    pub image: DynamicImage,
    /// Dimensions as displayed (after orientation).
    pub width: u32,
    pub height: u32,
}

/// Decodes JPEG/PNG/GIF/WebP/TIFF/BMP with limits checked before any pixel
/// buffer is allocated.
pub fn decode<R: BufRead + Seek>(reader: R, limits: DecodeLimits) -> Result<Decoded, ThumbError> {
    let mut reader = ImageReader::new(reader).with_guessed_format()?;
    let mut image_limits = Limits::default();
    image_limits.max_alloc = Some(limits.max_alloc);
    reader.limits(image_limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > limits.max_pixels {
        return Err(ThumbError::TooLarge(w, h));
    }
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let (width, height) = (image.width(), image.height());
    Ok(Decoded {
        image,
        width,
        height,
    })
}

/// Size of a variant that fits `max_w` × `max_h` keeping the aspect ratio,
/// never upscaling, never 0.
pub fn fit(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_w) / f64::from(width),
            f64::from(max_h) / f64::from(height),
        ),
    );
    let w = ((f64::from(width) * scale).round() as u32).max(1);
    let h = ((f64::from(height) * scale).round() as u32).max(1);
    (w, h)
}

/// Resized copy (Lanczos3) as 8-bit RGB or RGBA (kept only when the source has alpha).
pub fn resize(src: &DynamicImage, width: u32, height: u32) -> Result<DynamicImage, ThumbError> {
    let src = if src.color().has_alpha() {
        DynamicImage::ImageRgba8(src.to_rgba8())
    } else {
        DynamicImage::ImageRgb8(src.to_rgb8())
    };
    let mut dst = DynamicImage::new(width, height, src.color());
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|err| ThumbError::Resize(err.to_string()))?;
    Ok(dst)
}

/// Lossy WebP (libwebp) at `quality` 0–100.
pub fn encode_webp(image: &DynamicImage, quality: f32) -> Vec<u8> {
    let (w, h) = (image.width(), image.height());
    match image {
        DynamicImage::ImageRgba8(buf) => webp::Encoder::from_rgba(buf.as_raw(), w, h)
            .encode(quality)
            .to_vec(),
        other => {
            let rgb = other.to_rgb8();
            webp::Encoder::from_rgb(rgb.as_raw(), w, h)
                .encode(quality)
                .to_vec()
        }
    }
}

/// 4×3-component blurhash computed on a ≤ 32 px copy (fast, same result visually).
pub fn blurhash_of(image: &DynamicImage) -> Result<String, ThumbError> {
    let (w, h) = fit(image.width(), image.height(), 32, 32);
    let small = resize(image, w, h)?.to_rgba8();
    blurhash::encode(4, 3, w, h, small.as_raw())
        .map_err(|err| ThumbError::Blurhash(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ExifSpec, jpeg_sized_with_exif};
    use std::io::Cursor;

    #[test]
    fn fit_keeps_the_ratio_and_never_upscales() {
        assert_eq!(fit(4000, 3000, 1440, 1440), (1440, 1080));
        assert_eq!(fit(3000, 4000, 1080, 360), (270, 360));
        assert_eq!(fit(100, 50, 1440, 1440), (100, 50));
        assert_eq!(fit(10_000, 1, 1080, 360), (1080, 1));
    }

    #[test]
    fn orientation_is_applied_when_decoding() {
        let jpeg = jpeg_sized_with_exif(
            64,
            32,
            &ExifSpec {
                orientation: Some(6),
                ..ExifSpec::default()
            },
        );
        let decoded = decode(Cursor::new(jpeg), DecodeLimits::default()).unwrap();
        assert_eq!((decoded.width, decoded.height), (32, 64));
    }

    #[test]
    fn images_over_the_pixel_limit_are_refused_before_decoding() {
        let jpeg = jpeg_sized_with_exif(64, 64, &ExifSpec::default());
        let limits = DecodeLimits {
            max_pixels: 64 * 63,
            ..DecodeLimits::default()
        };
        assert!(matches!(
            decode(Cursor::new(jpeg), limits),
            Err(ThumbError::TooLarge(64, 64))
        ));
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            decode(
                Cursor::new(b"definitely not an image".to_vec()),
                DecodeLimits::default()
            ),
            Err(ThumbError::Decode(_))
        ));
    }

    #[test]
    fn variants_are_webp_and_blurhash_is_short() {
        let jpeg = jpeg_sized_with_exif(400, 300, &ExifSpec::default());
        let decoded = decode(Cursor::new(jpeg), DecodeLimits::default()).unwrap();
        let (w, h) = fit(decoded.width, decoded.height, 1080, 120);
        let small = resize(&decoded.image, w, h).unwrap();
        let webp = encode_webp(&small, 75.0);
        assert_eq!(&webp[..4], b"RIFF");
        assert_eq!(&webp[8..12], b"WEBP");
        let back = image::load_from_memory(&webp).unwrap();
        assert_eq!((back.width(), back.height()), (160, 120));
        let hash = blurhash_of(&decoded.image).unwrap();
        assert!((20..=40).contains(&hash.len()), "{hash}");
    }
}
```

> HEIF note (UNVERIFIED, see "What was verified"): libheif applies the container's rotation/mirroring itself, so for HEIF-family input `decode` must **not** apply EXIF orientation again. Before calling `decoder.orientation()`, check the first 12 bytes: if bytes 4..8 are `ftyp`, use `image::metadata::Orientation::NoTransforms`. Add a `heic`-gated test with a fixture generated by `heif-enc` from a rotated JPEG (`crates/squirrel-media/tests/fixtures/rotated.heic`, created in this task with `heif-enc -q 50 rotated.jpg -o rotated.heic` from `jpeg_sized_with_exif(64, 32, orientation 6)`; it is our own content).

`render.rs`:

```rust
//! The two stored variants of an image (grid thumbnail and lightbox preview).

use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::thumbs::{DecodeLimits, ThumbError, blurhash_of, decode, encode_webp, fit, resize};

/// Bump to regenerate every thumbnail (backfill compares `media.thumbs_version`).
pub const THUMBS_VERSION: i16 = 1;
pub const THUMB_HEIGHT: u32 = 360;
pub const THUMB_MAX_WIDTH: u32 = 1080;
pub const PREVIEW_SIDE: u32 = 1440;
const THUMB_QUALITY: f32 = 72.0;
const PREVIEW_QUALITY: f32 = 80.0;
pub const THUMB_FILE: &str = "thumb.webp";
pub const PREVIEW_FILE: &str = "preview.webp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rendered {
    /// The source as displayed (after orientation).
    pub width: u32,
    pub height: u32,
    pub blurhash: String,
}

/// Writes `thumb.webp` and `preview.webp` into `out_dir`.
pub fn render_variants(image: &DynamicImage, out_dir: &Path) -> Result<Rendered, ThumbError> {
    let (width, height) = (image.width(), image.height());
    let (pw, ph) = fit(width, height, PREVIEW_SIDE, PREVIEW_SIDE);
    let preview = resize(image, pw, ph)?;
    // From the preview: 4–16× fewer pixels to filter, no visible difference at 360 px.
    let (tw, th) = fit(pw, ph, THUMB_MAX_WIDTH, THUMB_HEIGHT);
    let thumb = resize(&preview, tw, th)?;
    fs::write(out_dir.join(PREVIEW_FILE), encode_webp(&preview, PREVIEW_QUALITY))?;
    fs::write(out_dir.join(THUMB_FILE), encode_webp(&thumb, THUMB_QUALITY))?;
    Ok(Rendered { width, height, blurhash: blurhash_of(&thumb)? })
}

pub fn render_image(input: &Path, out_dir: &Path, limits: DecodeLimits) -> Result<Rendered, ThumbError> {
    let decoded = decode(BufReader::new(File::open(input)?), limits)?;
    render_variants(&decoded.image, out_dir)
}
```

`heic.rs` (only compiled with `--features heic`; UNVERIFIED):

```rust
//! HEIC/HEIF/AVIF through libheif, registered as `image` decoding hooks so
//! `thumbs::decode` handles them like any other format.

/// Call once per process before decoding (the sandbox does it at start).
pub fn register() {
    libheif_rs::integration::image::register_all_decoding_hooks();
}
```

`lib.rs` gains `pub mod render; pub mod thumbs; #[cfg(feature = "heic")] pub mod heic;` and `pub const HEIC_SUPPORTED: bool = cfg!(feature = "heic");`.

**Step 4: Run** — `cargo test -p squirrel-media` → PASS. If libheif is installed locally: `cargo test -p squirrel-media --features heic` → PASS (adds the rotated-HEIC test).

**Step 5: Commit** — `feat(media): render WebP thumbnails, previews and blurhashes`.

---

### Task 11: Derived files store, garbage collection and orphan sweep

**Files:**
- Create: `crates/squirrel-storage/src/derived.rs`
- Modify: `crates/squirrel-storage/src/lib.rs`, `src/store.rs` (`BlobStore` owns a `DerivedStore`), `crates/squirrel-api/src/maintenance.rs`, `crates/squirrel/src/tasks.rs`, `crates/squirrel-api/tests/maintenance.rs`

**Step 1: Write the failing tests** — `derived.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn store() -> (tempfile::TempDir, DerivedStore) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tmp")).unwrap();
        let store = DerivedStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn scratch_with(store: &DerivedStore, files: &[&str]) -> Scratch {
        let scratch = store.scratch().unwrap();
        for name in files {
            fs::write(scratch.path().join(name), name.as_bytes()).unwrap();
        }
        scratch
    }

    #[test]
    fn installs_versioned_variants_read_only_under_the_hash() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store.install(&hash, 1, &scratch_with(&store, &["thumb.webp", "preview.webp"])).unwrap();
        let path = store.variant_path(&hash, Variant::Thumb, 1);
        assert!(path.ends_with(format!("2c/f2/{HASH}/thumb-v1.webp")));
        assert_eq!(store.read(&hash, Variant::Preview, 1).unwrap(), b"preview.webp");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn a_new_version_replaces_the_old_one() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store.install(&hash, 1, &scratch_with(&store, &["thumb.webp"])).unwrap();
        store.install(&hash, 2, &scratch_with(&store, &["thumb.webp"])).unwrap();
        assert!(matches!(store.read(&hash, Variant::Thumb, 1), Err(StorageError::NotFound)));
        assert!(store.read(&hash, Variant::Thumb, 2).is_ok());
    }

    #[test]
    fn scratch_directories_vanish_when_dropped() {
        let (dir, store) = store();
        let path = store.scratch().unwrap().path().to_owned();
        assert!(!path.exists());
        assert!(path.starts_with(dir.path().join("tmp")));
    }

    #[test]
    fn remove_all_is_idempotent_and_lists_follow() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store.install(&hash, 1, &scratch_with(&store, &["thumb.webp"])).unwrap();
        assert_eq!(store.list_older_than(SystemTime::now() + Duration::from_secs(60)).unwrap(), vec![hash.clone()]);
        store.remove_all(&hash).unwrap();
        store.remove_all(&hash).unwrap();
        assert!(store.list_older_than(SystemTime::now() + Duration::from_secs(60)).unwrap().is_empty());
    }
}
```

In `crates/squirrel-api/tests/maintenance.rs` add: GC of an unreferenced blob also removes `<data>/derived/ab/cd/<sha>/`; the orphan sweep removes a derived directory whose blob row does not exist (back-date it with `filetime`-free `File::set_modified`, as the existing orphan-blob test does) and keeps one whose blob row exists.

**Step 2: Run** → FAIL.

**Step 3: Implement** `DerivedStore`:

```rust
//! Files computed from a blob (thumbnails now; posters and HLS in Phase 4):
//!
//! ```text
//! <data_dir>/derived/ab/cd/<sha256>/thumb-v1.webp
//! <data_dir>/derived/ab/cd/<sha256>/preview-v1.webp
//! ```
//!
//! Written into a scratch directory under `<data_dir>/tmp/` and renamed into
//! place (same filesystem, checked at open). Blocking: call from `spawn_blocking`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Thumb,
    Preview,
}

impl Variant {
    pub fn stem(self) -> &'static str {
        match self {
            Self::Thumb => "thumb",
            Self::Preview => "preview",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DerivedStore {
    root: PathBuf,
    tmp: PathBuf,
}

/// A private temporary directory, removed (with its content) when dropped.
pub struct Scratch(PathBuf);
```

Methods: `open(data_dir)` (create `derived/` with `create_dir_all_durable`, check `device_of(derived) == device_of(tmp)` → `StorageError::CrossDevice`), `dir(hash)`, `variant_path(hash, variant, version)` (`<stem>-v<version>.webp`), `scratch()` (`tmp/derived-<uuid v7>`, mode 0700), `install(hash, version, scratch)` (for each `Variant` whose `<stem>.webp` exists in the scratch: `make_read_only`, `rename` to `variant_path`; then remove files of the same stem with another version; `sync_dir`), `read(hash, variant, version) -> Vec<u8>` (`NotFound` mapped), `remove_all(hash)` (`remove_dir_all` if present, then try to remove the now-empty `cd/` and `ab/` parents, ignoring `DirectoryNotEmpty`), `list_older_than(cutoff) -> Vec<ContentHash>` (walk two shard levels, parse names with `ContentHash::parse`, skip anything else, compare the directory's mtime). `Drop for Scratch` → `let _ = fs::remove_dir_all(&self.0)`.

`BlobStore::open` also opens `DerivedStore` and exposes `pub fn derived(&self) -> &DerivedStore`; `BlobStore::remove_blob` stays unchanged. In `maintenance.rs`, `delete_row_then_file` removes the derived directory right after the blob file (still under the blob's session advisory lock), and a new `sweep_orphan_derived(state) -> OrphanSweep` mirrors `sweep_orphan_blobs` (list derived hashes older than `blob_gc_grace`, check `blobs::existing` in batches of 1000, re-check `blobs::exists` under `blobs::lock_session` before `remove_all`). `tasks.rs` runs it after the blob sweep.

**Step 4: Run** — `cargo test -p squirrel-storage -p squirrel-api` → PASS.

**Step 5: Commit** — `feat(storage): store derived files per blob and collect them with it`.

---

### Task 12: Sandboxed media execution (`squirrel media-exec`) and the `squirrel-worker` crate

**Files:**
- Create: `crates/squirrel-media/src/exec.rs` (request/response protocol + `execute`)
- Create: `crates/squirrel-worker/Cargo.toml`, `src/lib.rs`, `src/exec.rs` (the `Executor`)
- Create: `crates/squirrel/src/media_exec.rs`, `crates/squirrel/tests/media_exec.rs`
- Modify: `crates/squirrel/src/main.rs` (hidden `MediaExec` subcommand), `crates/squirrel/Cargo.toml` (features `heic = ["squirrel-media/heic"]`), `Cargo.toml` (`rustix` features `["fs", "process"]`, workspace dep `squirrel-worker`)

**Step 1: Write the failing tests.**

`squirrel-media/src/exec.rs` tests: an `Image` request on a generated JPEG → `ExecResponse::Image { rendered }` and both files in `out_dir`; on `b"not an image"` → `Failed { retryable: false, .. }`; a 32 × 24 JPEG with `limits.max_pixels = 10` → `Unsupported { reason }` containing "exceeds"; a missing input file → `Failed { retryable: true, .. }`.

`crates/squirrel/tests/media_exec.rs` (runs the real binary, as production does):

```rust
use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif};
use squirrel_media::thumbs::DecodeLimits;
use squirrel_worker::exec::{ExecError, Executor, SubprocessConfig};

fn executor(timeout: Duration) -> Executor {
    Executor::Subprocess(SubprocessConfig {
        program: env!("CARGO_BIN_EXE_squirrel").into(),
        max_memory_mb: 2048,
        max_cpu_secs: 60,
        timeout,
    })
}

#[tokio::test]
async fn renders_in_a_child_process() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    std::fs::write(&input, jpeg_sized_with_exif(400, 300, &ExifSpec::default())).unwrap();
    let out = tempfile::tempdir().unwrap();
    let request = ExecRequest::Image { input, out_dir: out.path().into(), limits: DecodeLimits::default() };
    match executor(Duration::from_secs(60)).run(request).await.unwrap() {
        ExecResponse::Image { rendered } => assert_eq!((rendered.width, rendered.height), (400, 300)),
        other => panic!("unexpected {other:?}"),
    }
    assert!(out.path().join("thumb.webp").is_file());
}

#[tokio::test]
async fn a_child_that_overruns_its_time_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    std::fs::write(&input, jpeg_sized_with_exif(4000, 3000, &ExifSpec::default())).unwrap();
    let request = ExecRequest::Image { input, out_dir: dir.path().into(), limits: DecodeLimits::default() };
    let err = executor(Duration::from_millis(1)).run(request).await.unwrap_err();
    assert!(matches!(err, ExecError::TimedOut(_)), "{err}");
}
```

(`squirrel` gets dev-dependencies `squirrel-media = { workspace = true, features = ["test-util"] }`, `squirrel-worker`, `tempfile`, `tokio`.)

**Step 2: Run** → FAIL.

**Step 3: Implement.**

`squirrel-media/src/exec.rs`:

```rust
//! One unit of untrusted decoding, described as data so it can run in a child
//! process (`squirrel media-exec`) or in-process (tests). Outputs go to files in
//! `out_dir`, which the caller created and owns; names are fixed here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::render::{Rendered, render_image};
use crate::thumbs::{DecodeLimits, ThumbError};

/// Inputs larger than this are not decoded at all.
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ExecRequest {
    Image { input: PathBuf, out_dir: PathBuf, limits: DecodeLimits },
    // `Pdf` is added in Task 28.
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExecResponse {
    Image { rendered: Rendered },
    /// Nothing to do for this input (format not compiled in, too large, encrypted).
    Unsupported { reason: String },
    /// `retryable: false` — the input is broken; `true` — an I/O problem.
    Failed { reason: String, retryable: bool },
}

pub fn execute(request: &ExecRequest) -> ExecResponse {
    match request {
        ExecRequest::Image { input, out_dir, limits } => {
            match std::fs::metadata(input) {
                Ok(meta) if meta.len() > MAX_INPUT_BYTES => {
                    return ExecResponse::Unsupported { reason: format!("{} bytes is too large to decode", meta.len()) };
                }
                Ok(_) => {}
                Err(err) => return ExecResponse::Failed { reason: err.to_string(), retryable: true },
            }
            match render_image(input, out_dir, *limits) {
                Ok(rendered) => ExecResponse::Image { rendered },
                Err(err) => classify(err),
            }
        }
    }
}

fn classify(err: ThumbError) -> ExecResponse {
    use image::ImageError;
    let reason = err.to_string();
    match err {
        ThumbError::TooLarge(..) => ExecResponse::Unsupported { reason },
        ThumbError::Decode(ImageError::Unsupported(_) | ImageError::Limits(_)) => ExecResponse::Unsupported { reason },
        ThumbError::Decode(ImageError::IoError(_)) | ThumbError::Io(_) => ExecResponse::Failed { reason, retryable: true },
        _ => ExecResponse::Failed { reason, retryable: false },
    }
}
```

`crates/squirrel/src/media_exec.rs`:

```rust
//! `squirrel media-exec`: decode one untrusted input under resource limits.
//! Reads one JSON `ExecRequest` on stdin, writes one JSON `ExecResponse` on
//! stdout. Logs go to stderr (the parent keeps the tail for error messages).

use std::io::{Read, Write};

use rustix::process::{Resource, Rlimit, setrlimit};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(clap::Args)]
pub struct MediaExecArgs {
    #[arg(long, default_value_t = 2048)]
    max_memory_mb: u64,
    #[arg(long, default_value_t = 120)]
    max_cpu_secs: u64,
}

pub fn run(args: &MediaExecArgs) -> anyhow::Result<()> {
    // Before reading anything untrusted. Best effort on macOS (RLIMIT_AS is not enforced there).
    limit(Resource::As, args.max_memory_mb * 1024 * 1024);
    limit(Resource::Cpu, args.max_cpu_secs);
    limit(Resource::Nofile, 64);
    #[cfg(feature = "heic")]
    squirrel_media::heic::register();
    let mut raw = Vec::new();
    std::io::stdin().take(MAX_REQUEST_BYTES).read_to_end(&mut raw)?;
    let request: squirrel_media::exec::ExecRequest = serde_json::from_slice(&raw)?;
    let response = squirrel_media::exec::execute(&request);
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &response)?;
    out.flush()?;
    Ok(())
}

fn limit(resource: Resource, value: u64) {
    if let Err(err) = setrlimit(resource, Rlimit { current: Some(value), maximum: Some(value) }) {
        eprintln!("media-exec: could not set {resource:?} limit: {err}");
    }
}
```

`main.rs`: `#[command(hide = true)] MediaExec(media_exec::MediaExecArgs)` → `media_exec::run(&args)` **without** initialising tracing on stdout.

`squirrel-worker` crate (`Cargo.toml`: `anyhow`, `serde`, `serde_json`, `sqlx`, `squirrel-core`, `squirrel-db`, `squirrel-jobs`, `squirrel-media`, `squirrel-storage`, `thiserror`, `time`, `tokio`, `tracing`, `uuid`; dev: `squirrel-media` with `test-util`, `tempfile`), `src/exec.rs`:

```rust
//! Runs `ExecRequest`s in a sandboxed child process (production) or on the
//! blocking pool (tests, `SQUIRREL_MEDIA_EXEC=in-process`).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse, execute};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const STDERR_TAIL: usize = 2000;

#[derive(Debug, Clone)]
pub struct SubprocessConfig {
    /// The `squirrel` executable (`std::env::current_exe()` in production).
    pub program: PathBuf,
    pub max_memory_mb: u64,
    pub max_cpu_secs: u64,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum Executor {
    InProcess,
    Subprocess(SubprocessConfig),
}

/// Every variant is worth retrying (the job's attempt budget bounds it).
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("could not start the media sandbox: {0}")]
    Spawn(std::io::Error),
    #[error("media sandbox timed out after {0:?}")]
    TimedOut(Duration),
    #[error("media sandbox crashed ({status}): {stderr}")]
    Crashed { status: String, stderr: String },
    #[error("media sandbox protocol error: {0}")]
    Protocol(String),
}

impl Executor {
    pub async fn run(&self, request: ExecRequest) -> Result<ExecResponse, ExecError> {
        match self {
            Self::InProcess => tokio::task::spawn_blocking(move || execute(&request))
                .await
                .map_err(|err| ExecError::Crashed { status: "panic".into(), stderr: err.to_string() }),
            Self::Subprocess(config) => run_child(config, &request).await,
        }
    }
}

async fn run_child(config: &SubprocessConfig, request: &ExecRequest) -> Result<ExecResponse, ExecError> {
    let body = serde_json::to_vec(request).map_err(|err| ExecError::Protocol(err.to_string()))?;
    let mut child = Command::new(&config.program)
        .arg("media-exec")
        .arg("--max-memory-mb")
        .arg(config.max_memory_mb.to_string())
        .arg("--max-cpu-secs")
        .arg(config.max_cpu_secs.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(ExecError::Spawn)?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin.write_all(&body).await.map_err(ExecError::Spawn)?;
    drop(stdin);
    // Dropping the future on timeout drops the child, which kills it.
    let output = tokio::time::timeout(config.timeout, child.wait_with_output())
        .await
        .map_err(|_| ExecError::TimedOut(config.timeout))?
        .map_err(ExecError::Spawn)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.chars().rev().take(STDERR_TAIL).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(ExecError::Crashed { status: output.status.to_string(), stderr: tail });
    }
    serde_json::from_slice(&output.stdout).map_err(|err| ExecError::Protocol(err.to_string()))
}
```

**Step 4: Run** — `cargo test -p squirrel-media -p squirrel-worker -p squirrel` → PASS. On Linux additionally check by hand that `squirrel media-exec --max-memory-mb 64` fails a 4000 × 3000 decode with a non-zero exit (allocation failure) rather than succeeding.

**Step 5: Commit** — `feat(worker): decode untrusted media in a resource-limited child process`.

---

### Task 13: Migration `20261114000003_media.sql` and the media/asset repositories

**Files:**
- Create: `crates/squirrel-db/migrations/20261114000003_media.sql`
- Create: `crates/squirrel-db/src/media.rs`, `crates/squirrel-db/src/assets.rs`
- Create: `crates/squirrel-db/tests/media.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `crates/squirrel-db/src/nodes/read.rs` (`find_any`)

**Step 1: Write the failing tests** (`tests/media.rs`, using `common::{admin, invited, root, raw_file}`):
1. `media::ensure(&mut conn, &sha, MediaKind::Image)` inserts `metadata_state = 'pending'`, `thumbs_state = 'pending'`, `text_state = 'unsupported'`; `Pdf` → `unsupported`/`pending`/`pending`; `Text` → `unsupported`/`unsupported`/`pending`; a second call returns the existing row unchanged.
2. `assets::upsert` creates then updates (same `node_id`) `taken_at`, `taken_offset_min`, `taken_source`, `kind`.
3. Constraints: an asset whose `(node_id, owner_id)` does not match the node's owner is rejected (`assets_node_fkey`); deleting the node row deletes the asset; trashing it keeps it (timeline filters on `nodes.deleted_at`).
4. Deleting the blob row (after removing its nodes) cascades to `media`.
5. `assets::for_blob(pool, sha)` returns `(node_id, owner_id, name, mime, created_at)` for every node of the blob, of every owner (worker use only; documented as internal).
6. `nodes::find_any(pool, id)` returns live and trashed nodes regardless of owner (worker use only).

**Step 2: Run** → FAIL.

**Step 3: Implement.** The migration (verified on Postgres 17):

```sql
-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: what the worker learns from a blob's bytes (`media`), and the
-- per-user timeline entries (`assets`), one per image file node.

-- Content-derived facts. Shared by every node, of any user, that references
-- the blob; only ever read through a node the caller owns.
CREATE TABLE media (
    sha256           TEXT PRIMARY KEY REFERENCES blobs (sha256) ON DELETE CASCADE,
    kind             TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text')),
    -- Each step: pending → ready | failed | unsupported (nothing to do for this format).
    metadata_state   TEXT NOT NULL DEFAULT 'pending'
                     CHECK (metadata_state IN ('pending', 'ready', 'failed', 'unsupported')),
    thumbs_state     TEXT NOT NULL DEFAULT 'pending'
                     CHECK (thumbs_state IN ('pending', 'ready', 'failed', 'unsupported')),
    text_state       TEXT NOT NULL DEFAULT 'unsupported'
                     CHECK (text_state IN ('pending', 'ready', 'failed', 'unsupported')),
    -- Bumped when thumbnails are regenerated with new settings; part of their URL.
    thumbs_version   SMALLINT NOT NULL DEFAULT 0 CHECK (thumbs_version >= 0),
    -- As displayed (EXIF orientation applied).
    width            INTEGER CHECK (width > 0),
    height           INTEGER CHECK (height > 0),
    blurhash         TEXT CHECK (char_length(blurhash) <= 64),
    -- Wall-clock capture time and, when the file records it, its UTC offset.
    exif_taken_at    TIMESTAMP,
    exif_offset_min  SMALLINT CHECK (exif_offset_min BETWEEN -1080 AND 1080),
    latitude         DOUBLE PRECISION CHECK (latitude BETWEEN -90 AND 90),
    longitude        DOUBLE PRECISION CHECK (longitude BETWEEN -180 AND 180),
    altitude         DOUBLE PRECISION,
    camera_make      TEXT CHECK (char_length(camera_make) <= 128),
    camera_model     TEXT CHECK (char_length(camera_model) <= 128),
    lens_model       TEXT CHECK (char_length(lens_model) <= 128),
    exposure_time    TEXT CHECK (char_length(exposure_time) <= 32),
    f_number         REAL,
    iso              INTEGER,
    focal_length_mm  REAL,
    -- The EXIF says "Screenshot" (names and shapes are judged per node, in `assets.kind`).
    is_screenshot    BOOLEAN NOT NULL DEFAULT FALSE,
    page_count       INTEGER CHECK (page_count >= 0),
    -- Reverse geocoding (GeoNames); `geocoded_with` names the dataset used.
    place_name       TEXT,
    place_admin1     TEXT,
    place_country    TEXT,
    place_country_code TEXT CHECK (place_country_code ~ '^[A-Z]{2}$'),
    geocoded_with    TEXT,
    -- Last failure of any step, for admins.
    last_error       TEXT CHECK (char_length(last_error) <= 2000),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((latitude IS NULL) = (longitude IS NULL))
);

-- One timeline entry per image file node. `taken_at` is the wall-clock time
-- shown to the user and the sort key (days and months group on it).
CREATE TABLE assets (
    node_id       UUID PRIMARY KEY,
    owner_id      UUID NOT NULL,
    sha256        TEXT NOT NULL REFERENCES media (sha256),
    kind          TEXT NOT NULL CHECK (kind IN ('photo', 'screenshot')),
    taken_at      TIMESTAMP NOT NULL,
    -- UTC offset of `taken_at` in minutes, when known.
    taken_offset_min SMALLINT CHECK (taken_offset_min BETWEEN -1080 AND 1080),
    taken_source  TEXT NOT NULL CHECK (taken_source IN ('exif', 'filename', 'upload')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT assets_node_owner_key UNIQUE (node_id, owner_id),
    CONSTRAINT assets_node_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE
);
-- Timeline keyset pagination: ORDER BY taken_at DESC, node_id DESC.
CREATE INDEX assets_timeline_idx ON assets (owner_id, taken_at DESC, node_id DESC);
CREATE INDEX assets_sha256_idx ON assets (sha256);
```

Repositories (runtime `query_as`, as elsewhere): `MediaRecord` (`FromRow`, every column; `kind()` parses `MediaKind`), `MediaState` enum (`Pending`/`Ready`/`Failed`/`Unsupported` with `as_str`), `ensure`, `find`, `set_metadata(pool, sha, &MetadataUpdate)` (EXIF columns + `width`/`height` only if still NULL + `metadata_state='ready'`), `set_thumbnails(pool, sha, version, width, height, blurhash)` (`thumbs_state='ready'`, overwrites `width`/`height` with the decoded truth), `set_state(pool, sha, Step, MediaState, error: Option<&str>)` (`Step` = `Metadata`/`Thumbs`/`Text` picks the column from a fixed match — never interpolated from input), `thumbnails_for(pool, &[String]) -> HashMap<String, ThumbRef{version, blurhash}>` (ready only). `assets`: `NewAsset`, `upsert(conn, &NewAsset)` (`ON CONFLICT (node_id) DO UPDATE SET kind, taken_at, taken_offset_min, taken_source`), `for_blob`. `nodes::find_any` is `#[doc(hidden)]` with the same warning as `blobs::find`.

**Step 4: Run** — `cargo test -p squirrel-db` → PASS.

**Step 5: Commit** — `feat(db): add media facts per blob and timeline assets per node`.

---

### Task 14: `process_media` handler

**Files:**
- Create: `crates/squirrel-worker/src/context.rs`, `src/process.rs`, `src/jobs.rs`
- Create: `crates/squirrel-worker/tests/common/mod.rs`, `tests/process.rs`
- Modify: `crates/squirrel-jobs/src/lib.rs` (+ `pub mod kinds`)

`squirrel_jobs::kinds` (light, so `squirrel-api` can enqueue without depending on media crates):

```rust
//! Job kinds enqueued across crates.

use serde_json::json;
use uuid::Uuid;

use crate::{NewJob, priority};

pub const PROCESS_MEDIA: &str = "process_media";

/// Classify a stored file and queue its media work. `background`: from backfill.
pub fn process_media(node_id: Uuid, background: bool) -> NewJob {
    let base = if background { priority::background(priority::INTERACTIVE) } else { priority::INTERACTIVE };
    NewJob::new(PROCESS_MEDIA, json!({ "node_id": node_id, "background": background }))
        .priority(base)
        .dedupe(format!("{PROCESS_MEDIA}:{node_id}"))
}
```

`squirrel-worker/src/context.rs`:

```rust
pub struct MediaContext {
    pub pool: PgPool,
    pub storage: BlobStore,
    pub executor: Executor,
    /// Directory holding libpdfium; `None`: PDFs are `unsupported`.
    pub pdfium_dir: Option<PathBuf>,
    pub geo: GeoSource, // Task 21; `GeoSource::Disabled` until then
    pub decode_limits: DecodeLimits,
}
```

`src/jobs.rs` builds the per-blob follow-up jobs (`extract_metadata`, `thumbnails`, `geocode`, `extract_text`), each `dedupe("<kind>:<sha256>")`, priority from the key decisions, lowered by `BACKGROUND_PENALTY` when `background`.

**Step 1: Write the failing tests** — `tests/process.rs` (`common` builds a `MediaContext` over the test pool, a temp data dir with `BlobStore::open`, `Executor::InProcess`, and offers `store_file(owner, parent, name, bytes) -> NodeRecord` that puts the bytes with `BlobStore::put_reader` and inserts blob + node rows with the sniffed MIME):

1. A JPEG node → after `process::run(&ctx, job_ctx(json!({"node_id": id})))`: a `media` row (`kind = 'image'`, `metadata_state = 'pending'`), an `assets` row with `taken_source = 'upload'` (or `'filename'` for `IMG_20240714_183005.jpg`) and jobs `extract_metadata:<sha>` and `thumbnails:<sha>` queued at priorities 70 / 80.
2. Same with `"background": true` → priorities 20 / 30.
3. A second JPEG node with the same bytes (another user) → a second asset, **no** new jobs (dedupe; media already pending), and once `metadata_state = 'ready'`, its `taken_at` comes straight from EXIF.
4. A PDF node → `media.kind = 'pdf'`, no asset, jobs `thumbnails` + `extract_text`; a `.txt` → only `extract_text`; a ZIP → nothing at all.
5. A node deleted for good before the job runs → `Ok(())`, nothing written.
6. A malformed payload → `JobError::Permanent`.

**Step 2: Run** → FAIL.

**Step 3: Implement** `process.rs`:

```rust
#[derive(Deserialize)]
struct Payload {
    node_id: Uuid,
    #[serde(default)]
    background: bool,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let payload: Payload = job.payload()?;
    let Some(node) = nodes::find_any(&ctx.pool, payload.node_id).await.map_err(JobError::retry)? else {
        return Ok(()); // deleted for good meanwhile
    };
    let (Some(sha), Some(mime)) = (node.blob_sha256.clone(), node.mime.clone()) else {
        return Ok(()); // a folder
    };
    let Some(kind) = MediaKind::for_mime(&mime) else {
        return Ok(());
    };
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    let media = media::ensure(&mut tx, &sha, kind).await.map_err(JobError::retry)?;
    if kind == MediaKind::Image {
        let asset = asset_for(&node, &media, OffsetDateTime::now_utc());
        assets::upsert(&mut tx, &asset).await.map_err(JobError::retry)?;
    }
    for follow_up in jobs::follow_ups(&media, payload.background) {
        squirrel_jobs::enqueue(&mut tx, &follow_up).await.map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)?;
    Ok(())
}

/// Provisional until metadata is ready; `extract_metadata` recomputes it.
pub(crate) fn asset_for(node: &NodeRecord, media: &MediaRecord, now: OffsetDateTime) -> NewAsset {
    let exif = media.capture_time(); // Some only when metadata_state = 'ready'
    let taken = resolve_taken(exif, &node.name, node.created_at, now);
    let meta = media.as_image_metadata(); // says_screenshot + camera fields
    let mime = node.mime.as_deref().unwrap_or_default();
    let screenshot = is_screenshot(&meta, &node.name, mime, media.width(), media.height());
    NewAsset::new(node, media, taken, if screenshot { AssetKind::Screenshot } else { AssetKind::Photo })
}
```

`jobs::follow_ups` enqueues a step only when its state is `pending`. `lib.rs` exposes `pub fn registry(ctx: Arc<MediaContext>) -> Registry` registering `process_media` (timeout 60 s) via a closure that clones the `Arc`.

**Step 4: Run** — `cargo test -p squirrel-worker` → PASS.

**Step 5: Commit** — `feat(worker): classify stored files and queue their media work`.

---

### Task 15: `extract_metadata` handler

**Files:**
- Create: `crates/squirrel-worker/src/metadata.rs`, `tests/metadata.rs`

**Step 1: Write the failing tests:**
1. A JPEG with EXIF (2024-07-14 18:30:05 +02:00, Paris GPS, Google Pixel 8, orientation 6, 64 × 32) → `media`: `metadata_state = 'ready'`, `exif_taken_at`, `exif_offset_min = 120`, lat/lon, make/model, `width = 32, height = 64` (rotated, from the header); every asset of the blob: `taken_at = 2024-07-14 18:30:05`, `taken_offset_min = 120`, `taken_source = 'exif'`; a `geocode:<sha>` job queued (priority 30).
2. A JPEG without EXIF named `Screenshot_2024-07-14-18-30-05.png`-like → `taken_source = 'filename'`, `kind = 'screenshot'`; no geocode job.
3. Bytes that are not an image under an `image/jpeg` blob → `metadata_state = 'ready'` with empty fields (EXIF absent is not an error), dimensions NULL.
4. A TIFF larger than 64 MiB (sparse temp file with a TIFF header) → EXIF skipped, `metadata_state = 'ready'`.
5. The blob row is gone → `Ok(())`.

**Step 2: Run** → FAIL.

**Step 3: Implement:**

```rust
/// EXIF of TIFF files (read whole by kamadak-exif) is skipped above this size.
const MAX_TIFF_EXIF_BYTES: u64 = 64 * 1024 * 1024;

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256).await.map_err(JobError::retry)? else {
        return Ok(());
    };
    if media.kind() != MediaKind::Image {
        return Ok(());
    }
    let path = ctx.storage.blob_path(&hash);
    let extracted = tokio::task::spawn_blocking(move || extract(&path))
        .await
        .map_err(JobError::retry)?;
    let extracted = match extracted {
        Ok(found) => found,
        Err(err) if job.is_last_attempt() => {
            media::set_state(&ctx.pool, &sha256, Step::Metadata, MediaState::Failed, Some(&err.to_string()))
                .await
                .map_err(JobError::retry)?;
            return Err(JobError::retry(err));
        }
        Err(err) => return Err(JobError::retry(err)),
    };
    media::set_metadata(&ctx.pool, &sha256, &extracted.update()).await.map_err(JobError::retry)?;
    recompute_assets(ctx, &sha256).await?;
    if extracted.meta.gps.is_some() {
        let mut conn = ctx.pool.acquire().await.map_err(JobError::retry)?;
        squirrel_jobs::enqueue(&mut conn, &jobs::geocode(&sha256, false)).await.map_err(JobError::retry)?;
    }
    Ok(())
}

struct Extracted {
    meta: ImageMetadata,
    /// Displayed size from the header, if the format's header is readable here.
    size: Option<(u32, u32)>,
}

fn extract(path: &Path) -> std::io::Result<Extracted> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let head = reader.fill_buf()?.get(..4).map(<[u8]>::to_vec).unwrap_or_default();
    let is_tiff = head == *b"II*\0" || head == *b"MM\0*";
    let meta = if is_tiff && len > MAX_TIFF_EXIF_BYTES {
        ImageMetadata::default()
    } else {
        read_metadata(&mut reader)?
    };
    let size = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map(|(w, h)| if matches!(meta.orientation, Some(5..=8)) { (h, w) } else { (w, h) });
    Ok(Extracted { meta, size })
}
```

`recompute_assets` loads `assets::for_blob` + the updated `media` row and upserts each asset with `process::asset_for` (one transaction).

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(worker): extract EXIF metadata and settle capture times`.

---

### Task 16: `thumbnails` handler

**Files:**
- Create: `crates/squirrel-worker/src/thumbnails.rs`, `tests/thumbnails.rs`

**Step 1: Write the failing tests:**
1. A 400 × 300 JPEG → `thumbs_state = 'ready'`, `thumbs_version = THUMBS_VERSION`, `width/height = 400/300`, a blurhash, and `derived/…/<sha>/thumb-v1.webp` + `preview-v1.webp` exist.
2. Already ready at the current version → no work (file mtimes unchanged).
3. Garbage bytes → `thumbs_state = 'failed'`, `last_error` set, job `Ok` (not retried).
4. `image/heic` without the `heic` feature → `unsupported` with reason "built without HEIC support" (decided in the worker, no subprocess spawned). With the feature (cfg-gated test) → ready.
5. A blob larger than 512 MiB (sparse file + a `blobs.size` row to match) → `unsupported`.
6. Executor failure (use an `Executor::Subprocess` pointing at `/nonexistent`) → `JobError::Retry`; on the last attempt `thumbs_state = 'failed'` too.

**Step 2: Run** → FAIL.

**Step 3: Implement:**

```rust
pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256).await.map_err(JobError::retry)? else {
        return Ok(());
    };
    if media.thumbs_state() == MediaState::Ready && media.thumbs_version == THUMBS_VERSION {
        return Ok(());
    }
    let blob = blobs::find(&ctx.pool, &hash).await.map_err(JobError::retry)?;
    let Some(blob) = blob else { return Ok(()) };
    if let Some(reason) = unsupported_reason(&media, &blob, ctx.pdfium_dir.is_some()) {
        return mark(ctx, &sha256, MediaState::Unsupported, &reason).await;
    }
    let derived = ctx.storage.derived().clone();
    let scratch = tokio::task::spawn_blocking(move || derived.scratch())
        .await
        .map_err(JobError::retry)?
        .map_err(JobError::retry)?;
    let request = request_for(ctx, &media, ctx.storage.blob_path(&hash), scratch.path().to_owned());
    match ctx.executor.run(request).await {
        Ok(ExecResponse::Image { rendered }) => install(ctx, &hash, scratch, &rendered).await,
        Ok(ExecResponse::Unsupported { reason }) => mark(ctx, &sha256, MediaState::Unsupported, &reason).await,
        Ok(ExecResponse::Failed { reason, retryable: false }) => mark(ctx, &sha256, MediaState::Failed, &reason).await,
        Ok(ExecResponse::Failed { reason, retryable: true }) => give_up_or_retry(ctx, &job, &sha256, reason).await,
        Err(err) => give_up_or_retry(ctx, &job, &sha256, err.to_string()).await,
        Ok(other) => Err(JobError::permanent(anyhow::anyhow!("unexpected sandbox answer {other:?}"))),
    }
}

/// Moves the variants into place, then records them (row after files: a
/// crash in between only leaves files the next run overwrites).
async fn install(ctx: &MediaContext, hash: &ContentHash, scratch: Scratch, rendered: &Rendered) -> Result<(), JobError> {
    let (derived, blob) = (ctx.storage.derived().clone(), hash.clone());
    tokio::task::spawn_blocking(move || derived.install(&blob, THUMBS_VERSION, &scratch))
        .await
        .map_err(JobError::retry)?
        .map_err(JobError::retry)?;
    media::set_thumbnails(&ctx.pool, hash.as_str(), THUMBS_VERSION, rendered.width, rendered.height, &rendered.blurhash)
        .await
        .map_err(JobError::retry)
}
```

`unsupported_reason`: `blob.size > MAX_INPUT_BYTES` → "too large"; `MediaKind::is_heif_family(&blob.mime) && !squirrel_media::HEIC_SUPPORTED` → "built without HEIC support"; `kind == Pdf && !has_pdfium` → "pdfium is not configured" (logged once per process at `warn`); `kind == Text` → "no preview for text". `give_up_or_retry` marks `failed` on the last attempt then returns `JobError::retry`. Register with a 180 s handler timeout: the sandbox's wall clock is 150 s and the lease 300 s, so the sandbox always gives up first.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(worker): generate thumbnails in the sandbox and record them`.

---

### Task 17: Enqueue on upload, backfill, and the full worker wiring

**Files:**
- Modify: `crates/squirrel-api/src/routes/uploads/complete.rs`, `src/routes/uploads/create.rs`, `crates/squirrel-api/Cargo.toml`
- Create: `crates/squirrel-worker/src/backfill.rs`, `tests/backfill.rs`
- Modify: `crates/squirrel/src/worker.rs`, `crates/squirrel/src/admin.rs`
- Create: `crates/squirrel-api/tests/media_jobs.rs`

**Step 1: Write the failing tests.**

`crates/squirrel-api/tests/media_jobs.rs`: uploading `photo.jpg` (complete path) leaves exactly one `process_media` job whose payload `node_id` is the new node, at priority 100; the dedup shortcut (same bytes again into another folder) enqueues one for the second node too; a failed completion (hash mismatch) enqueues nothing; a ZIP upload enqueues nothing.

`crates/squirrel-worker/tests/backfill.rs`: with three pre-existing nodes (a JPEG with no asset, a PDF with no media, a ZIP) and a cursor page size of 2: running `backfill` twice enqueues `process_media` (background priority 50) for the JPEG and the PDF only, re-enqueues `backfill` with the next cursor after the first page and not after the last; media at `thumbs_version = 0` with `thumbs_state = 'ready'` gets a `thumbnails` job; a trashed JPEG without asset is included.

**Step 2: Run** → FAIL.

**Step 3: Implement.**
- `complete.rs` `store_and_link`: right after `nodes::insert_file`, `if MediaKind::for_mime(mime).is_some() { squirrel_jobs::enqueue(&mut tx, &kinds::process_media(node.id, false)).await.map_err(|e| discard(DbError::from(e)))?; }` — inside the same transaction, before `uploads::delete`. Same in `create.rs` `link_existing` (with `?` mapping to `ApiError` via `DbError`).
- `backfill.rs`:

```rust
const PAGE: i64 = 500;

#[derive(Deserialize, Default)]
struct Payload {
    after: Option<Uuid>,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let Payload { after } = job.payload()?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n \
          WHERE n.kind = 'file' AND n.id > $1 AND n.mime = ANY($2) \
            AND NOT EXISTS (SELECT 1 FROM assets a WHERE a.node_id = n.id) \
            AND NOT (n.mime <> ALL($3) AND EXISTS (SELECT 1 FROM media m WHERE m.sha256 = n.blob_sha256)) \
          ORDER BY n.id LIMIT $4",
    )
    .bind(after.unwrap_or(Uuid::nil()))
    .bind(SUPPORTED_MIMES)
    .bind(IMAGE_MIMES)
    .bind(PAGE)
    .fetch_all(&ctx.pool)
    .await
    .map_err(JobError::retry)?;
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    for id in &ids {
        squirrel_jobs::enqueue(&mut tx, &kinds::process_media(*id, true)).await.map_err(JobError::retry)?;
    }
    if after.is_none() {
        requeue_stale_steps(&mut tx).await?; // thumbs_version < THUMBS_VERSION; geocode (Task 21)
    }
    if i64::try_from(ids.len()).unwrap_or(0) == PAGE {
        let next = jobs::backfill(ids.last().copied()).delay(Duration::from_secs(2));
        squirrel_jobs::enqueue(&mut tx, &next).await.map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)
}
```

  (The dedupe key `backfill` is held by the running job itself, so the follow-up page uses the key `backfill:<after>`.) `SUPPORTED_MIMES` / `IMAGE_MIMES` come from `squirrel_core::MediaKind` (expose `MediaKind::mimes(kind)`).
- `crates/squirrel/src/worker.rs`: build `MediaContext` (`BlobStore::open(data_dir)`, executor from `--media-exec subprocess|in-process` (`SQUIRREL_MEDIA_EXEC`, default `subprocess`, program `std::env::current_exe()`), `--pdfium-dir` (`SQUIRREL_PDFIUM_DIR`), decode limits), register all handlers, and at start enqueue `jobs::backfill(None)` (dedupe `backfill`).
- `squirrel admin media backfill` enqueues the same job and prints whether one was already queued.

**Step 4: Run** — `cargo test --workspace` → PASS. Manual: upload a photo in the dev UI with `serve --worker`; within seconds `derived/…/thumb-v1.webp` exists.

**Step 5: Commit** — `feat: queue media processing on upload and backfill existing files`.

---

### Task 18: Thumbnail endpoint, thumbnails in node listings, cache clearing on logout

**Files:**
- Create: `crates/squirrel-api/src/routes/thumbnails.rs`, `crates/squirrel-api/tests/thumbnails.rs`
- Modify: `crates/squirrel-api/src/error.rs` (`NotReady`), `src/files_dto.rs` (`ThumbnailRef`, `NodeDto.thumbnail`), `src/routes/nodes.rs`, `src/routes/trash.rs` (stays `null`), `src/routes/auth.rs`, `src/lib.rs`, `openapi.json`, `web/src/lib/api/schema.d.ts`

**Step 1: Write the failing tests** (`tests/thumbnails.rs`; the test installs variants directly with `DerivedStore::install` and `media::set_thumbnails` — no worker needed):
1. Alice's JPEG with ready thumbnails: `GET /api/files/{id}/thumbnail?size=thumb&v=1` → 200, `content-type: image/webp`, body = the installed bytes, `cache-control: private, max-age=31536000, immutable`, `x-content-type-options: nosniff`, `cross-origin-resource-policy: same-origin`, CSP = `SANDBOX_CSP`, strong ETag `"<sha>-thumb-v1"`; `If-None-Match` with it → 304 with the same policy headers.
2. `size=preview` → the preview bytes; `size=huge` → 400 `bad_request`.
3. Pending thumbnails → 404 `not_ready`; a folder or a ZIP → 404 `not_found`.
4. **Isolation:** Bob requesting Alice's node → 404 `not_found` (even though the blob's thumbnails exist); Bob's own node of the **same bytes** → 200 (shared derived files).
5. A trashed node → 404.
6. `GET /api/nodes?parent_id=<Photos>` lists the JPEG with `thumbnail: {version: 1, blurhash: "…"}`, a pending one with `thumbnail: null`.
7. `POST /api/auth/logout` → `clear-site-data: "cache"`.

**Step 2: Run** → FAIL.

**Step 3: Implement** — `routes/thumbnails.rs`:

```rust
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ThumbnailQuery {
    /// `thumb` (grid, 360 px tall) or `preview` (lightbox, 1440 px).
    pub size: ThumbnailSize,
    /// The `thumbnail.version` from the listing; only busts caches.
    pub v: Option<i16>,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    Thumb,
    Preview,
}

const CACHE: &str = "private, max-age=31536000, immutable";

#[utoipa::path(get, path = "/api/files/{id}/thumbnail", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), ThumbnailQuery),
    responses(
        (status = 200, body = Vec<u8>, content_type = "image/webp"),
        (status = 304, description = "Unchanged"),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "not_found, or not_ready while the worker has not produced it"),
    ))]
pub async fn thumbnail(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let node = nodes::find_live(&state.pool, current.user.id, id)
        .await?
        .filter(|node| node.kind() == NodeKind::File)
        .ok_or(ApiError::NotFound)?;
    let hash = blob_hash(&node)?; // shared with routes/content.rs (move it to content/mod.rs)
    let media = media::find(&state.pool, hash.as_str()).await?.ok_or(ApiError::NotFound)?;
    if media.thumbs_state() != MediaState::Ready {
        return Err(ApiError::NotReady);
    }
    let variant = match query.size {
        ThumbnailSize::Thumb => Variant::Thumb,
        ThumbnailSize::Preview => Variant::Preview,
    };
    let etag = format!("\"{}-{}-v{}\"", hash.as_str(), variant.stem(), media.thumbs_version);
    let mut response_headers = to_header_map(vec![
        (CACHE_CONTROL, CACHE.to_owned()),
        (ETAG, etag.clone()),
        (CONTENT_SECURITY_POLICY, SANDBOX_CSP.to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (X_FRAME_OPTIONS, "SAMEORIGIN".to_owned()),
        (CROSS_ORIGIN_RESOURCE_POLICY, "same-origin".to_owned()),
    ])?;
    if etag_matches(header_str(&headers, IF_NONE_MATCH), &etag) {
        return Ok((StatusCode::NOT_MODIFIED, response_headers).into_response());
    }
    let (derived, version) = (state.storage.derived().clone(), media.thumbs_version);
    let bytes = run_storage(move || derived.read(&hash, variant, version))
        .await
        .map_err(|err| match err {
            // Row says ready but the file is gone (restored backup, manual delete): the
            // next backfill regenerates it; meanwhile behave as not ready.
            ApiError::NotFound => ApiError::NotReady,
            other => other,
        })?;
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    Ok((StatusCode::OK, response_headers, bytes).into_response())
}
```

Move `header_str`, `to_header_map`, `blob_hash` and `CROSS_ORIGIN_RESOURCE_POLICY` from `routes/content.rs` to `content/mod.rs` so both handlers share them. `ApiError::NotReady` → `(404, "not_ready")`, message "this preview is still being prepared". Register `thumbnail` in `json_routes`. `NodeDto` gains `#[serde(default)] pub thumbnail: Option<ThumbnailRef>` (`ThumbnailRef { version: i16, blurhash: Option<String> }`), set by `list_nodes` and `get_node` from one `media::thumbnails_for` call over the page's blob hashes. Logout handlers add `(HeaderName::from_static("clear-site-data"), "\"cache\"")`. Regenerate the contract.

**Step 4: Run** — `cargo test --workspace` → PASS; `pnpm check` → PASS.

**Step 5: Commit** — `feat(api): serve cached thumbnails and list them with files`.

**Milestone B review checkpoint:** a fresh dev instance with `serve --worker`: upload 20 phone photos and a PNG screenshot; `GET /api/nodes` shows `thumbnail` for each within seconds; `squirrel admin jobs` shows an empty queue; kill the worker mid-way (`kill -9`) and restart: leases expire after 5 minutes and processing completes.

---

## Milestone C — Offline geocoding

### Task 19: Reverse geocoder and the `geodata build` tool

**Files:**
- Create: `crates/squirrel-media/src/geo.rs` (verified), `crates/squirrel-media/src/geo_build.rs`
- Create: `crates/squirrel-media/tests/fixtures/geo-sample.tsv`
- Modify: `crates/squirrel-media/src/lib.rs`, `crates/squirrel/src/admin.rs`

**Step 1: Write the failing tests** — `geo.rs`'s test module (in the verified file below) and `geo_build.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const CITIES: &str = "2988507\tParis\tParis\tParis,Pariz\t48.85341\t2.3488\tP\tPPLC\tFR\t\t11\t75\t751\t75056\t2138551\t\t42\tEurope/Paris\t2024-06-05\n\
        2147714\tSydney\tSydney\t\t-33.86785\t151.20732\tP\tPPLA\tAU\t\t02\t\t\t\t4627345\t\t58\tAustralia/Sydney\t2024-01-01\n";
    const ADMIN1: &str = "FR.11\tÎle-de-France\tIle-de-France\t3012874\nAU.02\tNew South Wales\tNew South Wales\t2155400\n";
    const COUNTRIES: &str = "# ISO\tISO3\tISO-Numeric\tfips\tCountry\n\
        FR\tFRA\t250\tFR\tFrance\tParis\n\
        AU\tAUS\t036\tAS\tAustralia\tCanberra\n";

    #[test]
    fn joins_cities_with_region_and_country_names() {
        let mut out = Vec::new();
        let count = build(CITIES.as_bytes(), ADMIN1.as_bytes(), COUNTRIES.as_bytes(), "2026-11-14", &mut out).unwrap();
        assert_eq!(count, 2);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("# GeoNames cities500"), "{text}");
        assert!(text.contains("CC BY 4.0"));
        assert!(text.contains("48.85341\t2.3488\tParis\tÎle-de-France\tFR\tFrance\tEurope/Paris\n"));
        let index = GeoIndex::load(text.as_bytes()).unwrap();
        assert_eq!(index.nearest(-33.9, 151.2).unwrap().country, "Australia");
    }

    #[test]
    fn tabs_and_newlines_in_names_cannot_break_the_format() {
        let cities = CITIES.replace("\tParis\tParis\t", "\tPa\u{2028}ris\tParis\t");
        let mut out = Vec::new();
        build(cities.as_bytes(), ADMIN1.as_bytes(), COUNTRIES.as_bytes(), "x", &mut out).unwrap();
        assert!(GeoIndex::load(out.as_slice()).is_ok());
    }
}
```

**Step 2: Run** → FAIL.

**Step 3: Implement.** `geo.rs` (verified, 5 tests):

```rust
//! Offline reverse geocoding over a GeoNames cities500 extract.
//!
//! Data file (UTF-8 TSV, one place per line, `#` lines are comments):
//! `latitude \t longitude \t name \t admin1 name \t country code \t country name \t timezone`

use std::collections::HashMap;
use std::io::BufRead;

/// Matches farther than this are not a place (sea, wilderness).
pub const MAX_DISTANCE_KM: f64 = 50.0;
const EARTH_RADIUS_KM: f64 = 6371.0;
const KM_PER_DEGREE: f64 = 111.19;

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub admin1: String,
    pub country_code: String,
    pub country: String,
    pub timezone: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    #[error("line {0}: malformed place")]
    Malformed(usize),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

/// Places bucketed in 1° × 1° cells.
pub struct GeoIndex {
    places: Vec<Place>,
    cells: HashMap<(i16, i16), Vec<u32>>,
}

fn cell(latitude: f64, longitude: f64) -> (i16, i16) {
    (latitude.floor() as i16, longitude.floor() as i16)
}

/// Great-circle distance in km (haversine).
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().min(1.0).asin()
}

impl GeoIndex {
    pub fn load(reader: impl BufRead) -> Result<Self, GeoError> {
        let mut places = Vec::new();
        for (number, line) in reader.lines().enumerate() {
            let line = line?;
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            places.push(parse_line(&line).ok_or(GeoError::Malformed(number + 1))?);
        }
        Ok(Self::from_places(places))
    }

    pub fn from_places(places: Vec<Place>) -> Self {
        let mut cells: HashMap<(i16, i16), Vec<u32>> = HashMap::new();
        for (i, place) in places.iter().enumerate() {
            let index = u32::try_from(i).expect("fewer than 4 billion places");
            cells
                .entry(cell(place.latitude, place.longitude))
                .or_default()
                .push(index);
        }
        Self { places, cells }
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// The nearest place within [`MAX_DISTANCE_KM`], if any.
    pub fn nearest(&self, latitude: f64, longitude: f64) -> Option<&Place> {
        let (row, col) = cell(latitude, longitude);
        // 50 km is < 1° of latitude; in longitude a degree shrinks with cos(latitude).
        let shrink = latitude.abs().min(89.0).to_radians().cos();
        let span = ((MAX_DISTANCE_KM / (KM_PER_DEGREE * shrink)).ceil() as i16 + 1).min(180);
        let mut best: Option<(f64, &Place)> = None;
        for r in row - 1..=row + 1 {
            for c in col - span..=col + span {
                let wrapped = (c + 180).rem_euclid(360) - 180;
                for &i in self.cells.get(&(r, wrapped)).into_iter().flatten() {
                    let place = &self.places[i as usize];
                    let d = distance_km(latitude, longitude, place.latitude, place.longitude);
                    if d <= MAX_DISTANCE_KM && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, place));
                    }
                }
            }
        }
        best.map(|(_, place)| place)
    }
}

fn parse_line(line: &str) -> Option<Place> {
    let mut parts = line.split('\t');
    let latitude: f64 = parts.next()?.parse().ok()?;
    let longitude: f64 = parts.next()?.parse().ok()?;
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    let mut text = || parts.next().map(str::to_owned);
    Some(Place {
        name: text()?,
        admin1: text()?,
        country_code: text()?,
        country: text()?,
        timezone: text()?,
        latitude,
        longitude,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# GeoNames cities500 extract, CC BY 4.0\n\
        48.85341\t2.3488\tParis\tÎle-de-France\tFR\tFrance\tEurope/Paris\n\
        48.80359\t2.13424\tVersailles\tÎle-de-France\tFR\tFrance\tEurope/Paris\n\
        -33.86785\t151.20732\tSydney\tNew South Wales\tAU\tAustralia\tAustralia/Sydney\n\
        -16.5\t179.9\tLabasa\tNorthern\tFJ\tFiji\tPacific/Fiji\n";

    fn index() -> GeoIndex {
        GeoIndex::load(SAMPLE.as_bytes()).unwrap()
    }

    #[test]
    fn finds_the_nearest_place() {
        let geo = index();
        assert_eq!(geo.len(), 4);
        assert_eq!(geo.nearest(48.8606, 2.3376).unwrap().name, "Paris");
        assert_eq!(geo.nearest(48.79, 2.12).unwrap().name, "Versailles");
        assert_eq!(
            geo.nearest(-33.85, 151.21).unwrap().admin1,
            "New South Wales"
        );
    }

    #[test]
    fn far_from_everything_is_no_place() {
        assert!(index().nearest(0.0, -30.0).is_none());
    }

    #[test]
    fn searches_across_the_antimeridian() {
        assert_eq!(index().nearest(-16.5, -179.9).unwrap().name, "Labasa");
    }

    #[test]
    fn malformed_lines_are_reported_with_their_number() {
        let err = GeoIndex::load("# c\n1\t2\tonly-three\n".as_bytes())
            .err()
            .unwrap();
        assert!(matches!(err, GeoError::Malformed(2)), "{err}");
    }

    #[test]
    fn haversine_is_right_for_a_known_pair() {
        let d = distance_km(48.85341, 2.3488, 51.50853, -0.12574); // Paris → London
        assert!((d - 343.5).abs() < 2.0, "{d}");
    }
}
```

`geo_build.rs`: reads GeoNames' `cities500.txt` (19 tab-separated columns: 0 id, 1 name, 4 latitude, 5 longitude, 8 country code, 10 admin1 code, 17 timezone), `admin1CodesASCII.txt` (`CC.code \t name \t ascii \t id`) and `countryInfo.txt` (`#` comments; column 0 ISO, 4 name); writes the header

```text
# GeoNames cities500 extract for Squirrel, source date <date>.
# Data: GeoNames (https://www.geonames.org), licensed CC BY 4.0. Columns:
# latitude	longitude	name	admin1	country_code	country	timezone
```

then one line per place, replacing control characters (and U+2028/U+2029) in names with spaces, sorted by `(country, name)` for stable diffs. `pub fn build(cities, admin1, countries: impl BufRead, source_date: &str, out: impl Write) -> Result<usize, GeoError>`.

CLI: `squirrel admin geodata build --cities cities500.txt --admin1 admin1CodesASCII.txt --countries countryInfo.txt --date 2026-11-14 --out geonames-cities500-20261114.tsv` (no database needed: make `Admin` dispatch this subcommand before connecting).

`tests/fixtures/geo-sample.tsv`: the header plus ~10 real places produced by the tool (Paris, Versailles, Lyon, London, New York, Sydney, Tokyo, Labasa, Reykjavik, Cape Town) — our extract of CC BY data, attribution in its header.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(media): add an offline reverse geocoder over GeoNames cities500`.

---

### Task 20: Dataset pinning, verification and the fetch script

**Files:**
- Create: `crates/squirrel-media/src/geo_data.rs`, `scripts/fetch-geodata.sh`
- Modify: `crates/squirrel-media/src/lib.rs`, `crates/squirrel-media/Cargo.toml` (`sha2.workspace = true`)

**Step 1: Write the failing tests** (`geo_data.rs`): `load_verified(path, &Pinned { file_name, sha256 })` loads a file whose SHA-256 matches; a modified file → `GeoDataError::Checksum { expected, actual }`; `GeoSource::resolve(explicit: Option<PathBuf>, data_dir)` returns `Explicit(path)` when set, else `Pinned(data_dir/geo/<PINNED.file_name>)`; `dataset_id()` is the pinned file name for the pinned source and `custom:<file name>` for an explicit file.

**Step 2: Run** → FAIL.

**Step 3: Implement:**

```rust
/// The published snapshot (Open question 4). Update both fields together when
/// publishing a new snapshot; backfill then re-geocodes every geotagged photo.
pub const PINNED: Pinned = Pinned {
    file_name: "geonames-cities500-20261114.tsv",
    // Filled in when the release asset is published (Task 20, step "Publish").
    sha256: "0000000000000000000000000000000000000000000000000000000000000000",
    url: "https://github.com/<owner>/squirrel/releases/download/geodata-2026-11/geonames-cities500-20261114.tsv",
};
```

A unit test asserts `PINNED.sha256` is 64 lowercase hex characters and **is not all zeros** — marked `#[ignore = "set PINNED.sha256 after publishing the snapshot"]` until the maintainer publishes it, then un-ignored in the same commit that sets the hash.

`GeoSource` (shared by the worker): `Disabled` (tests), `Explicit(PathBuf)`, `Pinned(PathBuf)`; `load(&self) -> Result<Option<Arc<GeoIndex>>, GeoDataError>` → `Ok(None)` when the pinned file is absent (caller reschedules), loads and verifies otherwise, caching the index in a `tokio::sync::OnceCell` owned by the worker's `MediaContext`.

`scripts/fetch-geodata.sh <data_dir>`: `set -euo pipefail`; refuses an empty argument; `mkdir -p "$1/geo"`; reads the URL and hash from `squirrel admin geodata pinned` (which prints `PINNED`, so they live in one place), downloads to a temp file, `shasum -a 256 -c` against the pinned hash, moves it into place with mode 0400.

**Publish (maintainer, once, documented in README "Geodata"):** download `cities500.zip`, `admin1CodesASCII.txt`, `countryInfo.txt` from `https://download.geonames.org/export/dump/`, unzip, run `squirrel admin geodata build …`, create the GitHub release `geodata-2026-11` with the TSV as an asset, put its SHA-256 into `PINNED` and the repository's real owner into `PINNED.url` (the repo has no remote yet, hence `<owner>`), un-ignore the test.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(media): pin and verify the geodata snapshot`.

---

### Task 21: `geocode` handler

**Files:**
- Create: `crates/squirrel-worker/src/geocode.rs`, `tests/geocode.rs`
- Modify: `crates/squirrel-worker/src/backfill.rs` (re-geocode), `src/context.rs`, `crates/squirrel/src/worker.rs` (`--geodata-file` / `SQUIRREL_GEODATA_FILE`)

**Step 1: Write the failing tests** (with `GeoSource::Explicit(fixtures/geo-sample.tsv)`): a media row at 48.8606, 2.3376 → `place_name = 'Paris'`, `place_admin1 = 'Île-de-France'`, `place_country = 'France'`, `place_country_code = 'FR'`, `geocoded_with = 'custom:geo-sample.tsv'`; mid-Atlantic → places stay NULL but `geocoded_with` is set (so backfill does not retry it); no GPS → `Ok`, untouched; `GeoSource::Pinned` with no file → `JobError::Reschedule(1 h)`; a corrupt pinned file → `JobError::Retry` mentioning the checksum; backfill enqueues `geocode` for rows with GPS whose `geocoded_with` differs from the current dataset id.

**Step 2: Run** → FAIL.

**Step 3: Implement:** `run` loads the media row, returns early without GPS, gets the index (`Ok(None)` → `Reschedule(Duration::from_secs(3600))`), looks up `nearest` on the blocking pool (cheap, but loading the index on first use is not), and writes the place with `media::set_place(pool, sha, Option<&Place>, dataset_id)`.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(worker): name the places of geotagged photos offline`.

---

## Milestone D — Timeline & albums API

### Task 22: Timeline repository and cursors

**Files:**
- Create: `crates/squirrel-db/src/timeline.rs`, `crates/squirrel-db/tests/timeline.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `crates/squirrel-api/src/cursor.rs`

**Step 1: Write the failing tests** (`tests/timeline.rs`; helper `photo(pool, owner, parent, name, taken_at)` inserts blob, node, `media` (thumbs ready) and asset rows with raw SQL):
1. `timeline::page(pool, owner, Filter::All, None, 3)` returns the newest three by `taken_at`, ties broken by `node_id DESC`; `has_more`; the next page from the last item's `TimelineCursor { taken_at, node_id }` continues without gaps or repeats (insert 7 photos, two with identical `taken_at`).
2. `Filter::Month(2024, 7)` returns only July 2024 (bounds: `>= 2024-07-01 00:00` and `< 2024-08-01 00:00`), December rolls to the next year.
3. Trashed nodes are excluded; another user's assets never appear.
4. `timeline::months(pool, owner)` → `[("2024-07", 2), ("2024-06", 1)]`, newest first, trash excluded.
5. `timeline::detail(pool, owner, node_id)` returns the joined row (node name/size/mime, media EXIF/place/thumb fields, asset fields) or `None` for another user's node.

**Step 2: Run** → FAIL.

**Step 3: Implement** (columns shared by page, album items and detail):

```rust
const SUMMARY_COLUMNS: &str = "a.node_id, n.name, n.mime, n.size, a.kind, a.taken_at, a.taken_offset_min, \
     m.width, m.height, m.blurhash, m.thumbs_state, m.thumbs_version";
const FROM: &str = "FROM assets a \
     JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL \
     JOIN media m ON m.sha256 = a.sha256";

pub enum Filter {
    All,
    Month(i32, u8),
}

pub async fn page(
    pool: &PgPool,
    owner_id: Uuid,
    filter: Filter,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<AssetSummaryRecord>, DbError> {
    // QueryBuilder numbers only the binds actually used; the SQL text is made of
    // fixed fragments, so the row comparison stays an Index Cond on assets_timeline_idx.
    let mut query = QueryBuilder::<Postgres>::new(format!("SELECT {SUMMARY_COLUMNS} {FROM} WHERE a.owner_id = "));
    query.push_bind(owner_id);
    if let Some(cursor) = after {
        query.push(" AND (a.taken_at, a.node_id) < (").push_bind(cursor.taken_at);
        query.push(", ").push_bind(cursor.node_id).push(")");
    }
    if let Filter::Month(year, month) = filter {
        let (from, to) = month_bounds(year, month).ok_or(DbError::InvalidInput("month out of range"))?;
        query.push(" AND a.taken_at >= ").push_bind(from).push(" AND a.taken_at < ").push_bind(to);
    }
    query.push(" ORDER BY a.taken_at DESC, a.node_id DESC LIMIT ").push_bind(i64::from(limit) + 1);
    let rows = query.build_query_as::<AssetSummaryRecord>().fetch_all(pool).await?;
    Ok(Page::from_overfetch(rows, limit))
}
```

The test also checks the plan: `EXPLAIN` of the generated SQL with a cursor contains `Index Cond` (as Phase 2's keyset test does). `month_bounds(2024, 12)` → `(2024-12-01 00:00, 2025-01-01 00:00)`; check `DbError::InvalidInput`'s payload type matches (`&'static str` today).

`months`: `SELECT to_char(a.taken_at, 'YYYY-MM') AS month, count(*) AS count {FROM} WHERE a.owner_id = $1 GROUP BY 1 ORDER BY 1 DESC`. `TimelineCursor { taken_at: PrimitiveDateTime, node_id: Uuid }`; `cursor.rs` gains `encode_timeline`/`decode_timeline` (first part: `taken_at.assume_utc().unix_timestamp_nanos()`), with round-trip and garbage tests like the trash cursor's.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(db): page through a user's timeline by capture time`.

---

### Task 23: `GET /api/photos` and `GET /api/photos/months`

**Files:**
- Create: `crates/squirrel-api/src/routes/photos.rs`, `crates/squirrel-api/src/photos_dto.rs`, `crates/squirrel-api/tests/photos.rs`
- Modify: `crates/squirrel-api/src/lib.rs` (routes, tag `photos`), `openapi.json`, `web/src/lib/api/schema.d.ts`

DTOs:

```rust
#[derive(Debug, Serialize, ToSchema)]
pub struct AssetSummary {
    /// The file node's id (also used by `/api/files/{id}/…`).
    pub id: Uuid,
    pub name: String,
    pub mime: Option<String>,
    pub kind: AssetKindDto, // "photo" | "screenshot"
    /// Wall-clock capture time, no offset: `2024-07-14T18:30:05`.
    #[schema(example = "2024-07-14T18:30:05")]
    pub taken_at: String,
    /// `+02:00` when the camera recorded it.
    pub taken_offset: Option<String>,
    /// Displayed size, once known. The grid treats unknown as square.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub blurhash: Option<String>,
    pub thumbnail: ThumbnailState, // {state: "pending"|"ready"|"failed"|"unsupported", version}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TimelinePage {
    pub items: Vec<AssetSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MonthCount {
    /// `YYYY-MM`.
    pub month: String,
    pub count: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TimelineQuery {
    /// Only this month (`YYYY-MM`, wall-clock capture time).
    pub month: Option<String>,
    pub cursor: Option<String>,
    /// Default 200, at most 500.
    pub limit: Option<u32>,
}
```

`taken_at` is formatted with a fixed format description `[year]-[month]-[day]T[hour]:[minute]:[second]` (no sub-seconds); offsets as `+HH:MM`.

**Step 1: Write the failing tests** (`tests/photos.rs`): seeding through raw SQL as in Task 22 via `common::files`: paging (`limit=2`, cursor round-trip, `next_cursor: null` at the end), `month=2024-07` filter, `month=2024-13` / `month=x` → 422 `validation_error`, a bad cursor → 400, months list, items carry `thumbnail.state` and `blurhash`; unauthenticated → 401; Bob sees none of Alice's; trashed excluded.

**Step 2: Run** → FAIL. **Step 3: Implement** (page size default 200; month parsed with a small `parse_month(&str) -> Option<(i32, u8)>` unit-tested for `2024-07`, `1999-12`, `2024-7`, `2024-00`, `abcd-ef`). Regenerate the contract.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(api): list the photo timeline by month`.

---

### Task 24: `GET /api/photos/{id}` and `GET /api/media/status`

**Files:**
- Modify: `crates/squirrel-api/src/routes/photos.rs`, `src/photos_dto.rs`, `crates/squirrel-db/src/timeline.rs` (`status_counts`), `tests/photos.rs`, contract files

`AssetDetails { asset: AssetSummary, size: i64, parent_id: Uuid, place: Option<PlaceDto{name, admin1, country, country_code}>, location: Option<LocationDto{latitude, longitude, altitude}>, camera: Option<CameraDto{make, model, lens, exposure_time, f_number, iso, focal_length_mm}>, albums: Vec<AlbumRef{id, name}> }` (`albums` is empty until Task 26 wires it). `MediaStatus { queued: i64, processing: i64, failed: i64 }` where — for the caller's live files — `queued` = image/PDF/text files with no `media` row yet, `processing` = media with any step `pending`, `failed` = media with any step `failed`.

**Step 1: Tests:** detail of a geotagged photo has all three objects; a photo without EXIF has `place: null, location: null, camera: null`; Bob → 404 for Alice's id; a PDF node id → 404 (not an asset); status counts follow seeded states, scoped to the owner.

**Steps 2–5** as usual. Commit — `feat(api): describe a photo and report media processing status`.

---

### Task 25: Migration `20261114000004_albums.sql` and the albums repository

**Files:**
- Create: `crates/squirrel-db/migrations/20261114000004_albums.sql`, `crates/squirrel-db/src/albums.rs`, `crates/squirrel-db/tests/albums.rs`
- Create: `crates/squirrel-core/src/album_name.rs` (`AlbumName::parse`: trimmed, NFC, 1–100 chars, no control characters; tests like `DisplayName`'s)

**Step 1: Write the failing tests:** create/list (newest `updated_at` first, with `item_count` and `cover_node_id` = chosen cover else the newest live item); rename; delete (items gone, photos untouched); `add_items(album, owner, &[node_ids])` is idempotent (`ON CONFLICT DO NOTHING`), skips ids that are not the owner's live assets and returns how many were added; `remove_items`; **isolation at the database level:** inserting an `album_items` row pairing Alice's album with Bob's asset fails on `album_items_asset_fkey`; deleting a node clears it from albums and nulls the cover; trashed items are hidden from `items(…)` but come back after restore; `items` pages by `(taken_at DESC, node_id DESC)` like the timeline; `albums_for_node(owner, node)` for the detail endpoint. Adding or removing items bumps `updated_at`.

**Step 2: Run** → FAIL.

**Step 3: Implement.** Migration (verified on Postgres 17):

```sql
-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: albums, per user, holding timeline assets.

CREATE TABLE albums (
    id             UUID PRIMARY KEY,
    owner_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- Chosen cover; NULL means "the most recent item".
    cover_node_id  UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT albums_id_owner_key UNIQUE (id, owner_id),
    -- Postgres ≥ 15: SET NULL on the cover column only (owner_id stays).
    CONSTRAINT albums_cover_fkey FOREIGN KEY (cover_node_id, owner_id)
        REFERENCES assets (node_id, owner_id) ON DELETE SET NULL (cover_node_id)
);
CREATE INDEX albums_owner_idx ON albums (owner_id, updated_at DESC, id DESC);

CREATE TABLE album_items (
    album_id  UUID NOT NULL,
    node_id   UUID NOT NULL,
    owner_id  UUID NOT NULL,
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (album_id, node_id),
    -- Both sides belong to the same owner: an album never holds another user's photo.
    CONSTRAINT album_items_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT album_items_asset_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES assets (node_id, owner_id) ON DELETE CASCADE
);
CREATE INDEX album_items_node_idx ON album_items (node_id);
```

`add_items` inserts with `INSERT … SELECT $1, a.node_id, $2 FROM assets a JOIN nodes n ON n.id = a.node_id AND n.deleted_at IS NULL WHERE a.owner_id = $2 AND a.node_id = ANY($3) ON CONFLICT DO NOTHING`, after locking the album row (`SELECT … FROM albums WHERE id = $1 AND owner_id = $2 FOR UPDATE`, `NotFound` otherwise). At most 500 ids per call (checked in the API).

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(db): add albums of timeline photos`.

---

### Task 26: Albums API

**Files:**
- Create: `crates/squirrel-api/src/routes/albums.rs`, `crates/squirrel-api/src/albums_dto.rs`, `crates/squirrel-api/tests/albums.rs`
- Modify: `src/lib.rs` (routes, tag `albums`), `src/routes/photos.rs` (detail `albums`), contract files

Endpoints (all `json_routes`, `CurrentUser`, CSRF on writes): `GET /api/albums` → `[AlbumDto{id, name, item_count, cover: Option<AssetSummary>, updated_at}]` (all albums, ≤ 1000); `POST /api/albums {name, node_ids?}` → 201 `AlbumDto`; `GET /api/albums/{id}` → `AlbumDto`; `PATCH /api/albums/{id} {name?, cover_node_id?}` → 200 (`cover_node_id` must be an item of the album, else 422); `DELETE /api/albums/{id}` → 204; `GET /api/albums/{id}/items?cursor&limit` → `TimelinePage`; `POST /api/albums/{id}/items {node_ids}` → 200 `{changed}`; `POST /api/albums/{id}/items/remove {node_ids}` → 200 `{changed}`. `node_ids`: 1–500 (422 otherwise).

**Step 1: Tests:** the full lifecycle; names validated (empty, 101 chars, control characters → 422); Bob gets 404 on every Alice album route (GET, PATCH, DELETE, items, add, remove) and adding Alice's photo ids to Bob's own album changes nothing (`changed: 0`); CSRF required on writes; detail endpoint lists album memberships.

**Steps 2–5** as usual. Commit — `feat(api): create albums and add photos to them`.

**Milestone D review checkpoint:** with the dev UI from Milestone B, `curl` (with the session cookie) `/api/photos/months`, `/api/photos?month=…`, create an album and add items; everything owner-scoped; `openapi.json` regenerated.

---

## Milestone E — Documents: PDF thumbnails, text and search

### Task 27: Migration `20261114000005_documents.sql` and the document repository

**Files:**
- Create: `crates/squirrel-db/migrations/20261114000005_documents.sql`, `crates/squirrel-db/src/documents.rs`, `crates/squirrel-db/tests/documents.rs`

**Step 1: Write the failing tests:**
1. `documents::store_text(pool, sha, body)` stores the body and its `tsvector`; storing again replaces it; `media.text_state` becomes `ready` in the same transaction.
2. `documents::search(pool, owner, "ecole elec", 0, 20)` finds a document whose text is "Facture École Électricité 2026" (accents folded, prefixes), returns a snippet with `\u{2}École\u{3}`-style markers, and also finds a file **named** `ecole-inscription.pdf` with no text at all; a name match ranks above a body-only match.
3. Queries with no usable word (`""`, `"' & | !"`, `"a"`) → `Ok(vec![])` without touching the database (the API turns empty input into 422 first).
4. Trashed files and other users' files are never returned, even when they share the same blob (dedup) as one of the caller's matching files — the caller's own node is returned once.
5. `offset`/`limit` page through results; `has_more` from overfetch.

**Step 2: Run** → FAIL.

**Step 3: Implement.** Migration (verified on Postgres 17, including as a non-superuser owner):

```sql
-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: full-text search over PDF and plain-text documents, and file names.

-- `unaccent` is a trusted extension (Postgres ≥ 13): the database owner may create it.
CREATE EXTENSION IF NOT EXISTS unaccent;

-- Language-agnostic: no stemming (French and English mix in one home), accents
-- folded, so "ecole" finds "École". Queries add prefix matching.
CREATE TEXT SEARCH CONFIGURATION squirrel_text (COPY = pg_catalog.simple);
ALTER TEXT SEARCH CONFIGURATION squirrel_text
    ALTER MAPPING FOR hword, hword_part, word WITH unaccent, simple;

CREATE TABLE document_text (
    sha256      TEXT PRIMARY KEY REFERENCES media (sha256) ON DELETE CASCADE,
    -- Extracted text, capped at 256 KiB (kept for result snippets).
    body        TEXT NOT NULL CHECK (octet_length(body) <= 262144),
    content     TSVECTOR NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX document_text_content_idx ON document_text USING GIN (content);

-- Name search over live files.
CREATE INDEX nodes_name_search_idx ON nodes USING GIN (to_tsvector('squirrel_text', name))
    WHERE kind = 'file' AND deleted_at IS NULL;
```

The search statement:

```sql
WITH q AS (
    SELECT to_tsquery('squirrel_text', (
        SELECT string_agg(quote_literal(u.lexeme) || ':*', ' & ')
          FROM unnest(to_tsvector('squirrel_text', $2)) AS u
         WHERE length(u.lexeme) >= 2)) AS query)
SELECT n.id, n.owner_id, n.parent_id, n.name, n.kind, n.blob_sha256, n.size, n.mime,
       n.created_at, n.modified_at, n.deleted_at,
       m.page_count,
       CASE WHEN d.content @@ q.query THEN
           ts_headline('squirrel_text', d.body, q.query,
               'StartSel=' || chr(2) || ', StopSel=' || chr(3) || ', MaxFragments=2, MaxWords=18, MinWords=6')
       END AS snippet,
       CASE WHEN to_tsvector('squirrel_text', n.name) @@ q.query THEN 1.0 ELSE 0.0 END
         + COALESCE(ts_rank(d.content, q.query), 0) AS rank
  FROM q
  JOIN nodes n ON n.owner_id = $1 AND n.kind = 'file' AND n.deleted_at IS NULL
  LEFT JOIN media m ON m.sha256 = n.blob_sha256
  LEFT JOIN document_text d ON d.sha256 = n.blob_sha256
 WHERE q.query IS NOT NULL
   AND (to_tsvector('squirrel_text', n.name) @@ q.query OR d.content @@ q.query)
 ORDER BY rank DESC, n.modified_at DESC, n.id
 LIMIT $3 OFFSET $4
```

`store_text` strips NUL characters (Postgres `text` refuses them) and truncates to 256 KiB on a char boundary before binding.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(db): index document text and search it with file names`.

---

### Task 28: PDF rendering and text through pdfium

**Files:**
- Create: `crates/squirrel-media/src/pdf.rs`, `scripts/fetch-pdfium.sh`
- Modify: `crates/squirrel-media/src/exec.rs` (`ExecRequest::Pdf`, `ExecResponse::Pdf`), `crates/squirrel-media/Cargo.toml`, `Cargo.toml` (`pdfium-render = { version = "0.9.5", default-features = false, features = ["pdfium_latest", "image_latest", "thread_safe"] }`), `.gitignore` (`/target` already covers `target/pdfium`)

**Step 1: Write the failing tests** — in `pdf.rs` (they need pdfium; without `SQUIRREL_PDFIUM_DIR` they print `skipped` and pass):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::pdf_with_text;

    /// `scripts/fetch-pdfium.sh` prints the value to export.
    fn pdfium() -> Option<Pdfium> {
        let dir = std::env::var_os("SQUIRREL_PDFIUM_DIR")?;
        Some(bind(Path::new(&dir)).expect("pdfium loads"))
    }

    #[test]
    fn renders_the_first_page_and_extracts_text() {
        let Some(pdfium) = pdfium() else {
            eprintln!("skipped: SQUIRREL_PDFIUM_DIR is not set");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        std::fs::write(&input, pdf_with_text("Squirrel invoice 2026")).unwrap();
        let out = process(&pdfium, &input, &PdfWork { render: true, text: true }).unwrap();
        assert_eq!(out.page_count, 1);
        let page = out.first_page.unwrap();
        assert_eq!(page.height(), 1440, "A4 portrait fitted to 1440 px");
        assert!(out.text.unwrap().contains("Squirrel invoice 2026"));
    }

    #[test]
    fn garbage_is_unreadable() {
        let Some(pdfium) = pdfium() else { return };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("bad.pdf");
        std::fs::write(&input, b"%PDF-1.4 garbage").unwrap();
        assert!(matches!(process(&pdfium, &input, &PdfWork { render: true, text: false }), Err(PdfError::Unreadable(_))));
    }
}
```

and in `exec.rs`: a `Pdf` request with a `pdfium_dir` that does not contain the library → `Unsupported { reason }` mentioning pdfium (runs everywhere).

**Step 2: Run** → FAIL.

**Step 3: Implement** `pdf.rs` (the byte-slice variant of this code compiled against `pdfium-render` 0.9.5 while planning; this file-path variant is the same API with `load_pdf_from_file`):

```rust
//! First-page render and text of a PDF, through a pdfium library loaded at
//! runtime (`SQUIRREL_PDFIUM_DIR`). Runs inside the media sandbox.

use std::path::Path;

use image::DynamicImage;
use pdfium_render::prelude::*;

pub const RENDER_SIDE: i32 = 1440;
/// Text kept per document (UTF-8 bytes) and pages read for it.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
pub const MAX_TEXT_PAGES: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("pdfium is not available: {0}")]
    Unavailable(String),
    #[error("unreadable PDF: {0}")]
    Unreadable(String),
}

pub struct PdfWork {
    pub render: bool,
    pub text: bool,
}

pub struct PdfOutput {
    pub page_count: u32,
    pub first_page: Option<DynamicImage>,
    pub text: Option<String>,
}

pub fn bind(library_dir: &Path) -> Result<Pdfium, PdfError> {
    let path = Pdfium::pdfium_platform_library_name_at_path(library_dir);
    let bindings = Pdfium::bind_to_library(&path).map_err(|e| PdfError::Unavailable(e.to_string()))?;
    Ok(Pdfium::new(bindings))
}

pub fn process(pdfium: &Pdfium, input: &Path, work: &PdfWork) -> Result<PdfOutput, PdfError> {
    let unreadable = |e: PdfiumError| PdfError::Unreadable(e.to_string());
    // No password: encrypted documents fail here and are reported unsupported.
    let document = pdfium.load_pdf_from_file(input, None).map_err(unreadable)?;
    let pages = document.pages();
    let page_count = u32::try_from(pages.len()).unwrap_or(0);
    let first_page = if work.render {
        let config = PdfRenderConfig::new().set_maximum_width(RENDER_SIDE).set_maximum_height(RENDER_SIDE);
        let first = pages.first().map_err(unreadable)?;
        Some(first.render_with_config(&config).map_err(unreadable)?.as_image().map_err(unreadable)?)
    } else {
        None
    };
    let text = work.text.then(|| {
        let mut text = String::new();
        for page in pages.iter().take(MAX_TEXT_PAGES) {
            if text.len() >= MAX_TEXT_BYTES {
                break;
            }
            if let Ok(page_text) = page.text() {
                text.push_str(&page_text.all());
                text.push('\n');
            }
        }
        text.truncate(text.floor_char_boundary(MAX_TEXT_BYTES));
        text.replace('\0', "")
    });
    Ok(PdfOutput { page_count, first_page, text })
}
```

`exec.rs` additions:

```rust
// in ExecRequest:
Pdf { input: PathBuf, out_dir: PathBuf, pdfium_dir: PathBuf, render: bool, text: bool },
// in ExecResponse (text, when asked, is written to out_dir/TEXT_FILE):
Pdf { rendered: Option<Rendered>, page_count: u32 },
```

with `pub const TEXT_FILE: &str = "text.txt";`; `execute` binds pdfium (`Unavailable` → `Unsupported`), checks `MAX_INPUT_BYTES`, calls `pdf::process`, renders variants from `first_page` with `render_variants`, writes the text file; `Unreadable` whose message mentions a password → `Unsupported { reason: "encrypted PDF" }`, other `Unreadable` → `Failed { retryable: false }`.

`scripts/fetch-pdfium.sh` (hashes verified by downloading the archives while planning):

```bash
#!/usr/bin/env bash
# Downloads the pinned pdfium build (bblanchon/pdfium-binaries, chromium/8125,
# the version pdfium-render 0.9.5's `pdfium_latest` feature targets) into
# target/pdfium (or $1) and prints the SQUIRREL_PDFIUM_DIR to export.
set -euo pipefail
cd "$(dirname "$0")/.."

RELEASE="chromium/8125"
case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) ASSET=pdfium-linux-x64.tgz; SHA=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659 ;;
	Linux-aarch64) ASSET=pdfium-linux-arm64.tgz; SHA=8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa ;;
	Darwin-arm64) ASSET=pdfium-mac-arm64.tgz; SHA=4939bd935ac83ddc09a3a4f038bc43a19cea1660316a3667f26cb07a070e139d ;;
	*) echo "fetch-pdfium: no pinned pdfium build for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

DEST="${1:-${PWD}/target/pdfium}"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT
curl -fsSL -o "${TMP}/${ASSET}" "https://github.com/bblanchon/pdfium-binaries/releases/download/${RELEASE}/${ASSET}"
echo "${SHA}  ${TMP}/${ASSET}" | shasum -a 256 -c - >/dev/null
rm -rf "${DEST}"
mkdir -p "${DEST}"
tar -xzf "${TMP}/${ASSET}" -C "${DEST}"
echo "SQUIRREL_PDFIUM_DIR=${DEST}/lib"
```

The archive's `LICENSE` and `licenses/` stay next to the library (Phase 7's image copies them too).

**Step 4: Run** — `cargo test -p squirrel-media` → PASS (PDF tests skip); then `eval "export $(scripts/fetch-pdfium.sh | tail -1)" && cargo test -p squirrel-media pdf` → PASS with pdfium.

**Step 5: Commit** — `feat(media): render PDF first pages and extract their text with pdfium`.

---

### Task 29: PDF thumbnails and the `extract_text` handler

**Files:**
- Modify: `crates/squirrel-worker/src/thumbnails.rs` (PDF requests)
- Create: `crates/squirrel-worker/src/text.rs`, `tests/text.rs`

**Step 1: Write the failing tests:**
1. `extract_text` on a `text/plain` blob "Quarterly squirrel budget\n" → `document_text` row, `text_state = 'ready'`; a 1 MiB text file keeps 256 KiB; invalid UTF-8 is replaced, NULs dropped.
2. With pdfium (skip otherwise): a generated PDF → `text_state = 'ready'`, `page_count = 1`, searchable; `thumbnails` on the PDF → ready thumbnails (A4 → `width/height` 1018 × 1440).
3. Without pdfium configured → `text_state = 'unsupported'`, `thumbs_state = 'unsupported'`, jobs `Ok`.
4. An image blob reaching `extract_text` (should not happen) → `Ok`, untouched.

**Step 2: Run** → FAIL.

**Step 3: Implement.** `text.rs`: `Text` kind → read the first `MAX_TEXT_BYTES` of the blob on the blocking pool (`File::open(path)?.take(MAX_TEXT_BYTES as u64).read_to_end`), `String::from_utf8_lossy`, `documents::store_text`; `Pdf` kind → sandbox `ExecRequest::Pdf { render: false, text: true, … }`, then read `scratch/text.txt`, `documents::store_text`, `media::set_page_count`. Same error mapping as thumbnails (`unsupported` / `failed` / retry with last-attempt marking). Register with a 3-minute timeout. `thumbnails.rs` sends `ExecRequest::Pdf { render: true, text: false }` for PDFs and installs `rendered` like images.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(worker): extract document text and thumbnail PDFs`.

---

### Task 30: `GET /api/search/documents`

**Files:**
- Create: `crates/squirrel-api/src/routes/search.rs`, `crates/squirrel-api/src/search_dto.rs`, `crates/squirrel-api/tests/search.rs`
- Modify: `src/lib.rs` (tag `search`), contract files

DTOs: `DocumentHit { node: NodeDto, page_count: Option<i32>, snippet: Vec<SnippetPart> }`, `SnippetPart { text: String, highlight: bool }`, `DocumentResults { items: Vec<DocumentHit>, has_more: bool }`; query `q` (1–200 chars after trim; 422 otherwise), `offset` (≤ 200), `limit` (default 20, ≤ 50).

**Step 1: Write the failing tests:** seeded through `documents::store_text` + raw nodes: hits by content and by name; snippet parts alternate plain/highlight and never contain control characters; `q=""` → 422; `q` of 201 chars → 422; `offset=500` → 422; Bob's search never returns Alice's file even for the same text (dedup blob); thumbnails appear on hits whose PDFs have them (`node.thumbnail`).

`snippet_parts` unit tests (in `search_dto.rs`): `"a \u{2}b\u{3} c"` → `[("a ", false), ("b", true), (" c", false)]`; unbalanced markers and stray control characters are dropped; empty input → `[]`.

**Steps 2–5** as usual. Commit — `feat(api): search documents by name and content`.

**Milestone E review checkpoint:** with pdfium fetched and `serve --worker`: upload a real multi-page PDF (bank statement, invoice) and a `.md` note; the Files list shows the PDF's first page (after Task 39) and `curl '/api/search/documents?q=…'` returns both with snippets.

---

## Milestone F — Web: Photos timeline & lightbox

UI rules for every task below (from MASTER.md; reviewers check them):
- Only `tokens.css` custom properties and the `ui/` components; no literal colour, size, radius, shadow or duration (1 px borders and breakpoints excepted). New values go into MASTER.md and `tokens.css` first (Task 31).
- Phone first (375–412 px), then 768 / 1024 / 1440. Touch targets ≥ 44 px (36 px visuals carry a 44 px hit area).
- Motion: only `transform`, `opacity`, `box-shadow`, `background-color`, `border-color`, `color`. Hover changes a background or shadow only. **No arrival animation on touch devices** (`pointer: coarse`: content appears at once); reduced motion → instant. Photo tiles never animate in (the blurhash → image swap is instant); rows of the grid are not "arriving rows" either. Sheets/dialogs/lightbox keep their `--dur-slow` arrival.
- The accent appears only on the main action, progress, the active item, focus and selection.
- Real data only: counts, dates, places come from the API; nothing invented while loading (a blurhash or a tinted tile, never a fake photo or number).
- Every icon button has an accessible name that includes its subject ("Open photo taken 14 July 2024 18:30", "Select IMG_2041.jpg").

### Task 31: Design additions (MASTER.md, tokens, icons)

**Prerequisite:** the user validated Open question 8 (or chose other values; use theirs).

**Files:**
- Modify: `MASTER.md` (new "Photos" subsection under Components; tokens table rows), `web/src/lib/styles/tokens.css`, `web/src/lib/components/icons/paths.ts`

**Step 1:** Add to MASTER.md → Components:

```markdown
- **Photo tile**: the image fills the tile (`object-fit: cover`), `--radius-tile` (6 px), no shadow;
  while loading, the tile shows the photo's blurhash (or its average colour), else `--tint` with a
  20 px `image` icon in `--ink-muted`. Hover (hover devices): a 24 px round selection circle appears
  top-left (white 70 % ring). Selected: `--select-ring` and a filled `--accent` check circle with an
  `--on-accent` check. Focus: the global ring. Unsupported previews: `--tint` with the `image-off` icon.
- **Justified grid**: rows `--photo-row` high (112 px phones, 168 px from 768 px, 208 px from 1440 px),
  gaps `--space-1`; day labels `--text-sm` 500 `--ink-muted` tabular above each day; month headings
  `--text-lg` `--weight-heading` above each month.
- **Scrubber** (from 768 px): a slim column on the panel's right edge listing years, with months as
  ticks; the current month is a `--card` pill with `--shadow-knob` and `--ink` label; dragging shows the
  month label in that pill. Phones: a 44 px round `--card` handle on the right edge appears while
  scrolling (no persistent column).
- **Lightbox**: a full-viewport graphite stage (`--rail` gradient) with the photo centred; controls
  are `--rail-well` circles with `--rail-ink` icons (as rail items); the info panel is a `--card` sheet
  (bottom sheet on phones, right-side panel from 1024 px).
- **Selection bar**: a `--card` pill with `--shadow-pop` floating above the phone rail (top of the
  panel from 1024 px): count (tabular), then actions.
```

and the tokens rows: `--radius-tile` 6 px; `--photo-row` 112 / 168 / 208 px; `--select-check` 24 px; `--album-min` 160 px (album card minimum width). Add the matching custom properties to `tokens.css` (with the media queries for `--photo-row`). Add Lucide shapes `images` (nav), `image-off`, `map-pin`, `info`, `camera`, `search`, `plus`, `check-circle`, `library` (albums) to `paths.ts` (Lucide is ISC, already vendored with its `LICENSE`).

**Step 2:** `pnpm check && pnpm test:unit && pnpm build` → PASS.

**Step 3: Commit** — `docs: record photo grid, scrubber and lightbox in MASTER.md` (MASTER.md + tokens + icons together).

---

### Task 32: Client plumbing — API, blurhash, justified layout, timeline model

**Files:**
- Create: `web/src/lib/photos/api.ts`, `web/src/lib/photos/blurhash.ts` (+ `.test.ts`), `web/src/lib/photos/justified.ts` (+ `.test.ts`), `web/src/lib/photos/model.ts` (+ `.test.ts`), `web/src/lib/photos/dates.ts` (+ `.test.ts`)
- Modify: `web/src/lib/api/schema.d.ts` (regenerated)

**Step 1: Write the failing tests.**

`blurhash.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { averageColor, decodeBlurhash, isBlurhash } from '#lib/photos/blurhash.ts';

const HASH = 'LEHV6nWB2yk8pyo0adR*.7kCMdnj';

describe('blurhash', () => {
	it('validates length and alphabet', () => {
		expect(isBlurhash(HASH)).toBe(true);
		expect(isBlurhash('LEHV6nWB2yk8')).toBe(false);
		expect(isBlurhash('L!!!!!!!!!!!!!!!!!!!!!!!!!!')).toBe(false);
		expect(isBlurhash(null)).toBe(false);
	});

	it('gives the average colour without decoding', () => {
		expect(averageColor(HASH)).toBe('rgb(151 150 149)');
	});

	it('decodes to opaque pixels whose mean is the average colour', () => {
		const px = decodeBlurhash(HASH, 32, 32);
		expect(px.length).toBe(32 * 32 * 4);
		const sum = [0, 0, 0];
		for (let i = 0; i < px.length; i += 4) for (let c = 0; c < 3; c++) sum[c] += px[i + c];
		expect(sum.map((s) => Math.round(s / 1024))).toEqual([150, 150, 148]);
		expect(px[3]).toBe(255);
	});
});
```

`justified.test.ts`: full rows end exactly at the container width and are no taller than the target; the last row keeps the target height and is left-aligned; an empty list → `{rows: [], height: 0}`; a 20:1 panorama is clamped to aspect 3 (width 600 at height 200); `stack` blocks never overlap and `visible` returns only blocks intersecting the window (the values in the verified check below: rows `[{top 0, height 165, 4 tiles of 247 px}, {top 169, height 200}]` for four 3:2 and one 3:4 photo in 1000 px at target 200 with gap 4).

`dates.test.ts`: `dayKey('2024-07-14T18:30:05')` → `'2024-07-14'`; `monthKey` → `'2024-07'`; `dayLabel('2024-07-14', 'en-GB')` → `'Sun 14 Jul 2024'` (built with `new Date(2024, 6, 14)`, never parsed as UTC, so no day shift); `monthLabel('2024-07', 'en-GB')` → `'July 2024'`.

`model.test.ts`: `monthPlaceholders(months, width, opts)` gives each month an estimated height `heading + ceil(count × 1.4 × rowHeight / width) × (rowHeight + gap)`; `layoutMonth(items, …)` groups by day and stacks; `replaceEstimate(sections, month, realHeight)` returns the new total height and the **delta to add to `scrollTop` when the month is above the viewport** (scroll anchoring, no jump).

**Step 2: Run** — `pnpm test:unit` → FAIL.

**Step 3: Implement.** `blurhash.ts` (verified in Node 22):

```ts
/**
 * Blurhash decoding (https://blurha.sh, MIT reference algorithm), so a photo's
 * blurred colour preview can be painted while its thumbnail loads. No dependency.
 */
const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~';

function decode83(text: string): number {
	let value = 0;
	for (const char of text) {
		const digit = DIGITS.indexOf(char);
		if (digit < 0) throw new Error('invalid blurhash');
		value = value * 83 + digit;
	}
	return value;
}

function srgbToLinear(value: number): number {
	const v = value / 255;
	return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(value: number): number {
	const v = Math.max(0, Math.min(1, value));
	return Math.trunc(v <= 0.0031308 ? v * 12.92 * 255 + 0.5 : (1.055 * v ** (1 / 2.4) - 0.055) * 255 + 0.5);
}

const signPow = (value: number, exp: number) => Math.sign(value) * Math.abs(value) ** exp;

/** Whether `hash` is a well-formed blurhash (length matches its component count). */
export function isBlurhash(hash: string | null | undefined): hash is string {
	if (!hash || hash.length < 6) return false;
	try {
		const size = decode83(hash[0]);
		const nx = (size % 9) + 1;
		const ny = Math.floor(size / 9) + 1;
		return hash.length === 4 + 2 * nx * ny && decode83(hash) >= 0;
	} catch {
		return false;
	}
}

/** The average colour of a blurhash as `rgb(r g b)`: instant, before any pixel is decoded. */
export function averageColor(hash: string): string {
	const value = decode83(hash.slice(2, 6));
	return `rgb(${value >> 16} ${(value >> 8) & 255} ${value & 255})`;
}

/** RGBA pixels (`width × height × 4`) of the blurred preview. */
export function decodeBlurhash(hash: string, width: number, height: number): Uint8ClampedArray {
	if (!isBlurhash(hash)) throw new Error('invalid blurhash');
	const size = decode83(hash[0]);
	const nx = (size % 9) + 1;
	const ny = Math.floor(size / 9) + 1;
	const maxValue = (decode83(hash[1]) + 1) / 166;
	const colors: [number, number, number][] = [];
	for (let i = 0; i < nx * ny; i++) {
		if (i === 0) {
			const value = decode83(hash.slice(2, 6));
			colors.push([srgbToLinear(value >> 16), srgbToLinear((value >> 8) & 255), srgbToLinear(value & 255)]);
		} else {
			const value = decode83(hash.slice(4 + i * 2, 6 + i * 2));
			const quant = [Math.floor(value / (19 * 19)), Math.floor(value / 19) % 19, value % 19];
			colors.push(quant.map((q) => signPow((q - 9) / 9, 2) * maxValue) as [number, number, number]);
		}
	}
	const pixels = new Uint8ClampedArray(width * height * 4);
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			let r = 0;
			let g = 0;
			let b = 0;
			for (let j = 0; j < ny; j++) {
				for (let i = 0; i < nx; i++) {
					const basis = Math.cos((Math.PI * x * i) / width) * Math.cos((Math.PI * y * j) / height);
					const color = colors[i + j * nx];
					r += color[0] * basis;
					g += color[1] * basis;
					b += color[2] * basis;
				}
			}
			const at = 4 * (x + y * width);
			pixels[at] = linearToSrgb(r);
			pixels[at + 1] = linearToSrgb(g);
			pixels[at + 2] = linearToSrgb(b);
			pixels[at + 3] = 255;
		}
	}
	return pixels;
}
```

`justified.ts` (verified in Node 22):

```ts
/** Justified rows ("Google Photos" layout) and the stacked blocks the timeline virtualizes. */

export interface Tile {
	/** Width / height as displayed; anything non-finite or ≤ 0 counts as square. */
	aspect: number;
}

export interface PlacedTile {
	index: number;
	left: number;
	width: number;
}

export interface Row {
	top: number;
	height: number;
	tiles: PlacedTile[];
}

export interface LayoutOptions {
	width: number;
	targetHeight: number;
	gap: number;
}

/** Panoramas and slivers are cropped to this range so one photo never owns a row. */
const MIN_ASPECT = 0.5;
const MAX_ASPECT = 3;

function clampAspect(aspect: number): number {
	if (!Number.isFinite(aspect) || aspect <= 0) return 1;
	return Math.min(MAX_ASPECT, Math.max(MIN_ASPECT, aspect));
}

/**
 * Greedy justified layout: tiles fill a row until its height, scaled to fill the
 * width exactly, drops to `targetHeight`. The last row keeps `targetHeight` and is
 * left-aligned. Widths are rounded so each full row ends exactly at `width`.
 */
export function justify(tiles: readonly Tile[], { width, targetHeight, gap }: LayoutOptions): { rows: Row[]; height: number } {
	const rows: Row[] = [];
	let top = 0;
	let start = 0;
	let aspectSum = 0;
	const close = (end: number, height: number, fill: boolean) => {
		const placed: PlacedTile[] = [];
		let left = 0;
		for (let i = start; i < end; i++) {
			const last = i === end - 1;
			const exact = clampAspect(tiles[i].aspect) * height;
			const w = fill && last ? width - left : Math.round(exact);
			placed.push({ index: i, left, width: Math.max(1, w) });
			left += w + gap;
		}
		const rounded = Math.round(height);
		rows.push({ top, height: rounded, tiles: placed });
		top += rounded + gap;
	};
	for (let i = 0; i < tiles.length; i++) {
		aspectSum += clampAspect(tiles[i].aspect);
		const count = i - start + 1;
		const height = (width - gap * (count - 1)) / aspectSum;
		if (height <= targetHeight) {
			close(i + 1, height, true);
			start = i + 1;
			aspectSum = 0;
		}
	}
	if (start < tiles.length) close(tiles.length, targetHeight, false);
	return { rows, height: Math.max(0, top - gap) };
}

export interface Section<T> {
	key: string;
	label: string;
	items: T[];
}

export type Block<T> =
	| { kind: 'header'; key: string; label: string; top: number; height: number }
	| { kind: 'row'; key: string; top: number; height: number; items: { item: T; left: number; width: number }[] };

/** Sections (days) stacked as header + rows, with absolute tops for virtualization. */
export function stack<T extends Tile>(
	sections: readonly Section<T>[],
	options: LayoutOptions & { headerHeight: number; sectionGap: number }
): { blocks: Block<T>[]; height: number } {
	const blocks: Block<T>[] = [];
	let top = 0;
	for (const section of sections) {
		blocks.push({ kind: 'header', key: `h:${section.key}`, label: section.label, top, height: options.headerHeight });
		top += options.headerHeight;
		const { rows, height } = justify(section.items, options);
		rows.forEach((row, n) =>
			blocks.push({
				kind: 'row',
				key: `r:${section.key}:${n}`,
				top: top + row.top,
				height: row.height,
				items: row.tiles.map((t) => ({ item: section.items[t.index], left: t.left, width: t.width }))
			})
		);
		top += height + options.sectionGap;
	}
	return { blocks, height: Math.max(0, top - options.sectionGap) };
}

/** Blocks intersecting [scrollTop - overscan, scrollTop + viewport + overscan] (binary search on sorted tops). */
export function visible<T>(blocks: readonly Block<T>[], scrollTop: number, viewport: number, overscan: number): Block<T>[] {
	const from = scrollTop - overscan;
	const to = scrollTop + viewport + overscan;
	let lo = 0;
	let hi = blocks.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (blocks[mid].top + blocks[mid].height < from) lo = mid + 1;
		else hi = mid;
	}
	const out: Block<T>[] = [];
	for (let i = lo; i < blocks.length && blocks[i].top <= to; i++) out.push(blocks[i]);
	return out;
}
```

`api.ts` (same `call` / `Outcome` helpers as `files/api.ts`; export the types from `components['schemas']`): `listMonths()`, `listMonth(month)` (follows `next_cursor` until done, 500 per page — a month rarely holds more than a few hundred photos), `photoDetails(id)`, `mediaStatus()`, `thumbnailUrl(id, size, version)` → `/api/files/${encodeURIComponent(id)}/thumbnail?size=${size}&v=${version}`, albums functions (Task 38). `dates.ts` and `model.ts` as specified by their tests.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(web): add photo API helpers, blurhash decoding and justified layout`.

---

### Task 33: Navigation entry and the Photos page shell

**Files:**
- Modify: `web/src/lib/components/shell/AppNav.svelte` (item `{ href: '/photos', label: 'Photos', icon: 'images' }` after Home)
- Create: `web/src/routes/photos/+page.svelte`, `web/src/routes/photos/albums/+page.svelte` (placeholder until Task 38), `web/src/lib/components/photos/PhotosTabs.svelte`

**Spec:**
- Phone bottom bar: Home · Photos · Files · Trash (· Invitations): five items fit at 375 px (each ≥ 64 px wide with a 36 px well and a 12 px label). Check at 375 px that labels do not truncate; if they do, the label falls back to `--text-xs` (already the phone size).
- `/photos`: `Page` with title "Photos", description = "{n} photos" (sum of month counts, tabular) — or nothing until loaded (no placeholder number); actions: **Upload** (primary, `upload` icon; uploads into the `Photos` root through the existing upload queue and `UploadPanel`).
- Under the header, `PhotosTabs`: a pill segmented control (MASTER's segmented control: `--tint` track, chosen segment `--card` pill + `--shadow-knob`) with two links, "Timeline" (`/photos`) and "Albums" (`/photos/albums`), `aria-current="page"` on the active one. 44 px hit area.
- Empty library: `EmptyState` icon `images`, title "No photos yet", text "Upload photos, or add images anywhere in your drive: they appear here by date."

**Steps:** a unit test for nav item order (extract the item list to `lib/nav.ts` with a test, keeping `AppNav` rendering it); `pnpm check` → PASS; commit — `feat(web): add the Photos section to the navigation`.

---

### Task 34: Photo tile with placeholders and states

**Files:**
- Create: `web/src/lib/components/photos/PhotoTile.svelte`, `web/src/lib/photos/placeholder.ts` (+ test)

**Spec:**
- Props: `asset: AssetSummary`, `width`, `height` (px from the layout), `selected`, `selecting` (selection mode), `onOpen`, `onToggle`.
- Markup: one `<button>` (opens the lightbox; in selection mode toggles) with `aria-label` "Open photo taken {date time}" / "Select {name}", `aria-pressed` in selection mode; inside, absolutely positioned layers: placeholder, `<img>` (`loading="lazy"`, `decoding="async"`, `alt=""` because the button carries the name, `src` = thumb URL only when `thumbnail.state === 'ready'`), selection circle.
- Placeholder: `background-color: averageColor(blurhash)` at once; then a 32 × 32 canvas painted from `decodeBlurhash` inside `requestIdleCallback` (fallback `setTimeout`), stretched with CSS. `placeholder.ts` caches decoded data URLs per hash (`canvas.toDataURL()` → `background-image`), bounded LRU of 500 entries.
- States: `pending` → placeholder + sr-only "Preparing preview"; `failed`/`unsupported` → `--tint` well with `image-off` icon and `title` "Preview unavailable" (the photo still opens: the lightbox offers the original); `ready` → image over placeholder, the swap is instant (no fade).
- Hover (hover devices only): selection circle appears (`opacity` change at `--dur-fast`); selected: `--select-ring` + accent check.
- Touch: long-press (500 ms, cancelled by > 10 px movement) calls `onToggle` and enters selection mode; it must not trigger the context menu or text selection (`-webkit-touch-callout: none`, `user-select: none`).

**Tests:** `placeholder.test.ts` (LRU eviction, invalid hash → `null`). Component behaviour is covered by E2E. Commit — `feat(web): render photo tiles with blurhash placeholders`.

---

### Task 35: Virtualized timeline grouped by month and day

**Files:**
- Create: `web/src/lib/components/photos/Timeline.svelte`, `web/src/lib/components/photos/MonthSection.svelte`
- Modify: `web/src/routes/photos/+page.svelte`

**Spec:**
- Data: `listMonths()` once; every month gets an estimated height (`model.monthPlaceholders`). A month's items load when its estimated block comes within one viewport of the visible window (`IntersectionObserver` on per-month sentinels with `rootMargin: 100% 0px`), at most 2 month requests in flight; loaded months are laid out with `stack` (day sections) at the current width; their real height replaces the estimate with scroll anchoring (`model.replaceEstimate`).
- Rendering: the page scrolls normally (window scroll; the panel is not a scroll container); the timeline is one `position: relative` box of the total height; only blocks returned by `visible(…, scrollY − offsetTop, innerHeight, overscan = innerHeight)` are rendered, absolutely positioned with `transform: translateY(top)` (no layout thrash). Scroll handling is a passive listener batched in `requestAnimationFrame`.
- Width: `ResizeObserver` on the container; relayout on width change (debounced to one frame); row height from `--photo-row` read with `getComputedStyle` (so the token stays the source of truth).
- Month heading block: `--text-lg`, `--weight-heading`, `--track-heading` ("July 2024"); day header block: `--text-sm` 500 `--ink-muted`, tabular ("Sun 14 Jul 2024"), plus " · {place}" when any photo that day has a place in its summary (summaries do not carry places in Phase 3 → omit; noted for later).
- Keyboard: tiles are buttons in DOM order; arrow keys move focus between neighbouring tiles of the rendered rows (left/right within a row, up/down to the tile whose centre is closest in the next row); focus scrolls the tile into view.
- "Preparing" banner: when `mediaStatus()` reports `queued + processing > 0`, an `Alert` (tint) above the grid: "Preparing {n} photos and documents…" with a `<progress>`-free text (no invented progress); polled every 5 s while the page is visible (`document.visibilityState`), stopped at 0; on reaching 0, reload months whose items had pending thumbnails.
- No arrival animation for any of this.

**Tests:** `model.test.ts` additions for the in-flight limit and the load window; E2E (Task 43). Commit — `feat(web): show the photo timeline as a virtualized justified grid`.

---

### Task 36: Scrubber

**Files:**
- Create: `web/src/lib/components/photos/Scrubber.svelte`, `web/src/lib/photos/scrubber.ts` (+ test)

**Spec:**
- From 768 px: a column at the panel's right edge (inside the panel padding, sticky to the viewport, height = viewport − top row), listing years as `--text-xs` tabular `--ink-muted` labels at their proportional positions (`scrubber.ts`: position of each month = its section's top / total height); months are 2 px ticks. The current month (from the scroll position) is a `--card` pill with `--shadow-knob` and the label "Jul 2024" in `--ink`.
- Pointer: press/drag on the column maps the pointer's y to a month (nearest tick) and scrolls the window to that month's top instantly (no smooth scrolling: it would fight the virtualizer); the pill follows with the month label. Releasing keeps the position.
- Keyboard: the column is a `role="slider"` with `aria-label="Jump to month"`, `aria-valuetext="July 2024"`, `aria-valuemin/max` over month indexes; Up/Down move one month, PageUp/PageDown one year, Home/End newest/oldest.
- Phones (< 768 px): no column; while the user scrolls, a 44 px round `--card` handle with a `chevron-up`/`chevron-down` pair appears at the right edge for 1.5 s after the last scroll event (opacity only); dragging it scrubs like the column, showing the month pill next to it. With reduced motion it appears and disappears instantly.

**Tests:** `scrubber.test.ts`: month at y, y of month, year label positions without overlap (labels closer than 16 px are skipped). Commit — `feat(web): scrub through months on the photo timeline`.

---

### Task 37: Lightbox with details and offline map; multi-select

**Files:**
- Create: `web/src/lib/components/photos/Lightbox.svelte`, `web/src/lib/components/photos/PhotoInfo.svelte`, `web/src/lib/components/photos/Locator.svelte`, `web/src/lib/components/photos/SelectionBar.svelte`, `web/src/lib/photos/selection.svelte.ts` (+ test), `web/src/lib/photos/geo.ts` (+ test)
- Create: `web/static/maps/world-110m.svg` (generated, see below) and `web/static/maps/LICENSE.md` (Natural Earth: public domain)

**Lightbox spec:**
- Opens from a tile (and from album pages), as a native `<dialog>` (`showModal`, focus trap, Escape closes), full viewport, graphite stage. The URL gains `?photo=<id>` (`replaceState`) so reload/share-in-tab reopens it; closing restores focus to the tile.
- Shows the `preview` WebP (`thumbnail.state === 'ready'`), else the blurhash and, for formats the browser can display (JPEG/PNG/GIF/WebP), the original via `/api/files/{id}/content`; for HEIC without a preview: the `image-off` state with "Download original".
- Controls (rail-well circles, 44 px hit areas, accessible names with the photo's date): Close (`x`), Previous/Next (`chevron-left/right`, also ←/→ keys and horizontal swipe > 50 px on touch), Info (`info`, toggles the panel, `aria-expanded`), Add to album (`library`), Download (`download`, original), Delete (`trash`, danger, confirms with the existing `ConfirmButton` pattern). Neighbour previews are preloaded (`new Image()`).
- Arrival: the dialog's standard `--dur-slow` fade; navigation between photos is instant.

**Info panel (`PhotoInfo`) spec** — sections only when data exists:
- Date: "Sunday 14 July 2024, 18:30" + offset ("UTC+02:00") when known; source hint when not from EXIF ("Date from the file name" / "Upload date") in `--ink-muted`.
- Place: "Paris, Île-de-France, France" (`map-pin` well) + `Locator` + coordinates (6 decimals, tabular) + link "Open in OpenStreetMap" (`https://www.openstreetmap.org/?mlat=…&mlon=…#map=15/…/…`, `target="_blank" rel="noreferrer noopener"`, `external-link` icon) + the attribution line "Place names: GeoNames (CC BY 4.0)" in `--text-xs` `--ink-muted`. Coordinates without a place: locator + coordinates + link only.
- Camera (`camera` well): "Google Pixel 8" (make omitted when the model starts with it), lens, and one tabular line "1/250 · ƒ/1.7 · ISO 64 · 6.9 mm" from available values.
- File: name, dimensions "4032 × 3024", size, "Show in Files" link (`/files/{parent_id}`), albums it belongs to as `Badge`s linking to the album.
- All strings rendered as text (Svelte escaping); nothing from EXIF is interpreted as HTML or URL.

**Locator spec:** inline `<svg viewBox="0 0 360 180">` that `<use>`s the land paths from `/maps/world-110m.svg#land` (same-origin static asset, cached immutable by its hashed name — put it under `static/` with a version suffix) filled with `--tint-strong` on a `--tint` well (`--radius-well`), plus a 6 px `--accent` dot (selection/active item use of the accent) at `x = lon + 180`, `y = 90 − lat` (`geo.ts`, unit-tested including the antimeridian and poles). `aria-label="Map showing Paris, France"`. The SVG is generated once from Natural Earth's `ne_110m_land` (public domain) with an equirectangular projection simplified to ≤ 40 KB (`scripts/build-world-svg.mjs`, a 40-line Node script using only `node:fs` that reads the GeoJSON downloaded by hand from naturalearthdata.com; the generated SVG is committed, the GeoJSON is not).

**Multi-select spec (`selection.svelte.ts`, `SelectionBar`):**
- Enter selection: hover circle click (desktop), long-press (touch), or Space on a focused tile. Shift-click selects the range between the last toggled tile and this one (in timeline order, across loaded months only).
- While selecting: tiles toggle instead of opening; Escape clears; the `SelectionBar` shows "{n} selected" (tabular) and actions: **Add to album** (secondary; opens the album picker sheet from Task 38), **Download** is out of scope (no zip in Phase 3), **Delete** (danger; confirmation dialog "Move {n} photos to the trash?" → `DELETE /api/nodes/{id}` for each, at most 4 in parallel, reporting failures in an `Alert`), **Clear** (quiet `x`).
- Selection state survives month loading and relayout (keyed by id).

**Tests:** `selection.test.ts` (toggle, range across sections, clear), `geo.test.ts`. Commit — `feat(web): open photos in a lightbox with details, and select several`.

---

## Milestone G — Web: albums, list thumbnails, document search

### Task 38: Albums pages and the album picker

**Files:**
- Create: `web/src/routes/photos/albums/+page.svelte`, `web/src/routes/photos/albums/[id]/+page.svelte`, `web/src/lib/components/photos/AlbumCard.svelte`, `web/src/lib/components/photos/AlbumPicker.svelte`
- Modify: `web/src/lib/photos/api.ts`

**Spec:**
- `/photos/albums`: `Page` "Photos" with `PhotosTabs` on "Albums"; action **New album** (primary, `plus`) → `NameDialog` (existing component) "New album". A responsive grid (2 columns on phones, `auto-fill` with `minmax(var(--album-min), 1fr)` from 768 px) of `AlbumCard`s: a white card (`--radius-card`, `--shadow-card`, hover `--shadow-hover`) holding a 4:3 cover (cover thumbnail with its blurhash; empty album → `--tint` with the `library` icon), the name (500) and "{n} photos" (`--ink-muted`, tabular). Empty: `EmptyState` "No albums yet" / "Select photos in the timeline, then choose Add to album."
- `/photos/albums/[id]`: breadcrumbs "Albums › {name}", title = name, description "{n} photos"; actions (inline from 768 px, actions sheet on phones like file rows): Rename, Delete album (danger confirm: "Delete the album? The photos stay in your timeline."). Content: the same justified grid (no month sections; one day header per day) of `GET /api/albums/{id}/items`, paged by cursor as the user scrolls (sentinel), with lightbox and selection (selection action: **Remove from album**, **Set as cover** when exactly one is selected).
- `AlbumPicker`: bottom sheet / dialog "Add to album" listing albums (cover thumb 40 px round well + name + count) as row buttons, plus "New album…" first; choosing one calls `POST /api/albums/{id}/items` and shows "Added {changed} photos to {name}" in an `Alert` (success tone) on the page.

**Tests:** E2E (Task 43); unit tests for the album list sorting helper. Commit — `feat(web): browse albums and add photos to them`.

---

### Task 39: Thumbnails in the Files list and on Home

**Files:**
- Modify: `web/src/lib/components/ui/ListRow.svelte` (a `media` snippet alternative to `icon`, keeping the 36 px round well), `web/src/lib/components/files/FileTable.svelte`, `web/src/routes/+page.svelte` (recent list), `web/src/lib/files/nodeIcon.ts` (unchanged fallback), `web/src/lib/components/files/Preview.svelte` (images/PDFs use `preview` when ready)

**Spec:** when `node.thumbnail` is set, the row's well shows the `thumb` image (`object-fit: cover`, round, `loading="lazy"`, `alt=""`) over the blurhash average colour; otherwise the existing icon. The well keeps its size (36 px) and tint; no layout shift between states. Preview dialog: images show `preview` (1440 px) first and keep "Open in new tab" for the original; PDFs keep the browser viewer (the thumbnail is only for lists). E2E-relied labels and test ids stay unchanged (MASTER rule 7).

**Tests:** `pnpm check`, existing unit tests, E2E file specs still green. Commit — `feat(web): show thumbnails in file lists`.

---

### Task 40: Document search

**Files:**
- Create: `web/src/routes/files/search/+page.svelte`, `web/src/lib/files/search.ts` (+ test), `web/src/lib/components/files/SearchField.svelte`
- Modify: `web/src/lib/components/files/FolderView.svelte` (search field in the page actions)

**Spec:**
- `SearchField`: the `TextField` pill with a leading `search` icon and the label "Search files" (visually hidden label, placeholder "Search names and documents"); submitting navigates to `/files/search?q=…`. On phones it sits full width under the title (Page actions already wrap there).
- `/files/search`: title "Search", the field prefilled; results as row cards (`ListRow`): thumbnail well (PDF first page when ready) or icon, the name (link to the preview in its folder: `/files/{parent_id}?open={id}`), meta "PDF · 3 pages · 12 Mar 2026" (real values only), and the snippet below in `--text-sm` with highlighted parts in `<mark>` styled with `--accent-soft` background and `--ink` text (MASTER: `--accent-soft` is the text-highlight colour). Debounced as-you-type search (300 ms, `latestOnly` to drop stale answers) once 2+ characters are typed; "Load more" (secondary) when `has_more`. Empty result: `EmptyState` `search` "Nothing found" / "Search looks at file names, PDFs and text files. Scanned documents without text are not searchable yet."
- `search.ts`: `snippetParts` are rendered as text spans — the server already splits; the helper only merges adjacent parts and caps the total length (unit-tested).

**Tests:** unit + E2E (Task 43). Commit — `feat(web): search file names and document text`.

---

### Task 41: Media status everywhere it matters

**Files:**
- Modify: `web/src/routes/+page.svelte` (Home), `web/src/lib/components/files/FolderView.svelte`

**Spec:** after uploads settle (`onUploadsSettled`), Home and the folder view poll `mediaStatus()` (5 s, visible page only, stop at 0) and refresh their listing once when the count drops, so freshly generated thumbnails appear without a manual reload. No new visual element besides the Photos banner (Task 35).

**Tests:** unit test for the polling helper (`lib/photos/poll.ts`: stops at zero, pauses when hidden, single timer). Commit — `feat(web): refresh lists when media processing finishes`.

---

## Milestone H — E2E, CI & docs

### Task 42: E2E fixtures and a worker in the E2E server

**Files:**
- Create: `crates/squirrel-media/examples/make_fixtures.rs`, `web/e2e/fixtures/` (generated files + `README.md`)
- Modify: `scripts/e2e-server.sh`

**Step 1:** `make_fixtures.rs` (run with `cargo run -p squirrel-media --example make_fixtures --features test-util -- web/e2e/fixtures`) writes, from `testutil`:
- `paris-2024.jpg` — 1200 × 800, EXIF `2024:07:14 18:30:05`, `+02:00`, GPS 48°51′24″N 2°21′03″E, Google Pixel 8, exposure 1/250, f/1.7, ISO 64 (extend `ExifSpec` with these three fields);
- `lyon-2023.jpg` — 800 × 1200, `2023:05:01 09:00:00`, no offset, GPS Lyon, Apple iPhone 15;
- `Screenshot_2024-07-15-10-00-00.png` — 1080 × 2400 PNG, no EXIF;
- `notes.txt` — "Quarterly squirrel budget: acorns, hazelnuts and winter storage.";
- `invoice.pdf` — `pdf_with_text("Invoice 2026 for acorn storage")`.
The README states they are generated by that command and are project content (no third-party material).

**Step 2:** `scripts/e2e-server.sh` exports, before `exec cargo run … serve`:
```bash
export SQUIRREL_EMBEDDED_WORKER=true
export SQUIRREL_GEODATA_FILE="${PWD}/crates/squirrel-media/tests/fixtures/geo-sample.tsv"
# PDF thumbnails and text when pdfium was fetched (scripts/fetch-pdfium.sh); otherwise PDFs are unsupported.
if [[ -z "${SQUIRREL_PDFIUM_DIR:-}" && -d "${PWD}/target/pdfium/lib" ]]; then
	export SQUIRREL_PDFIUM_DIR="${PWD}/target/pdfium/lib"
fi
```
`SQUIRREL_MEDIA_EXEC` stays the default (`subprocess`): E2E exercises the sandbox through the real binary.

**Step 3:** `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` → the existing suites still PASS. **Commit** — `test: generate media fixtures and run the worker in E2E`.

---

### Task 43: E2E scenarios

**Files:**
- Create: `web/e2e/photos.spec.ts`, `web/e2e/photos-phone.spec.ts`, `web/e2e/documents.spec.ts`, `web/e2e/helpers/photos.ts`
- Modify: `web/playwright.config.ts` (projects `photos`, `photos-phone`, `documents`, each `dependencies: ['onboarding']`, `locale: 'en-GB'`, `timezoneId: 'Europe/Paris'`)

Scenarios (each waits for worker output with `expect.poll` on the API, at most 60 s, never fixed sleeps):

1. **Timeline** (desktop): Alice uploads `paris-2024.jpg`, `lyon-2023.jpg` and the screenshot into Photos; `/photos` shows month headings "July 2024" and "May 2023" in that order, the day header "Sun 14 Jul 2024", three tiles; each tile's `img` has `naturalWidth > 0` once ready; the description reads "3 photos".
2. **Lightbox**: clicking the Paris tile opens the dialog named after the photo; the info panel shows "Paris, Île-de-France, France", "Google Pixel 8", "1/250 · ƒ/1.7 · ISO 64", "UTC+02:00", the "Open in OpenStreetMap" link with `rel` containing `noreferrer`; → moves to the next photo; Escape closes and focus returns to the Paris tile.
3. **Albums**: select two tiles (hover circle), Add to album → New album "Summer" → the album page lists 2 photos; remove one; rename to "Summer 2024"; delete the album → the photos are still in the timeline.
4. **Scrubber** (desktop): the slider's `aria-valuetext` is "July 2024" at the top; pressing End moves to "May 2023" and the "May 2023" heading is in view.
5. **Isolation**: Bob's `/photos` is empty; `GET /api/files/<alice-paris-id>/thumbnail?size=thumb` with Bob's cookies → 404; `GET /api/photos/<id>` → 404; `GET /api/albums/<alice-album>` → 404.
6. **Trash**: deleting the Lyon photo from the lightbox removes it from the timeline and the months; restoring it from Trash brings it back.
7. **Phone** (`photos-phone`, 390 × 844, touch): the bottom bar shows "Photos" and it becomes the active item on `/photos`; long-press on a tile enters selection mode ("1 selected"); the lightbox opens and swiping left shows the next photo.
8. **Documents** (`documents`): upload `notes.txt` (and `invoice.pdf`); search "squir budg" from the Files page → a result "notes.txt" with "squirrel" highlighted; when `SQUIRREL_PDFIUM_DIR` is set for the server (checked through `GET /api/media/status` reaching zero and the PDF having `thumbnail` non-null), searching "acorn stor" also finds `invoice.pdf` and its row shows a thumbnail; otherwise that part is skipped with `test.skip` and a reason.
9. **Files list thumbnails**: the Photos folder lists `paris-2024.jpg` with an `img` inside its icon well.

**Run:** `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` → all PASS. **Commit** — `test(web): cover the photo timeline, lightbox, albums, search and isolation end to end`.

---

### Task 44: CI and README

**Files:**
- Modify: `.github/workflows/ci.yml`, `README.md`

**CI** (coordinate with whoever last touched `ci.yml`; append, do not reorder existing steps):
- `rust` job: before `cargo test`, `- run: scripts/fetch-pdfium.sh && echo "SQUIRREL_PDFIUM_DIR=$PWD/target/pdfium/lib" >> "$GITHUB_ENV"` so PDF tests run in CI.
- New job `rust-heic` (ubuntu-24.04, libheif 1.17): `sudo apt-get install -y libheif-dev libheif-plugin-libde265 || sudo apt-get install -y libheif-dev` then `cargo test -p squirrel-media -p squirrel-worker --features squirrel-media/heic` (Postgres service as in `rust`).
- `e2e` job: fetch pdfium the same way (so the PDF scenario runs).
- The migration guard already protects the new migrations once merged.

**README:** a "Media processing" section: the worker (`squirrel worker`, or `serve --worker` / `SQUIRREL_EMBEDDED_WORKER=true`), its options (`SQUIRREL_WORKER_CONCURRENCY`, `SQUIRREL_MEDIA_EXEC`, `SQUIRREL_PDFIUM_DIR`, `SQUIRREL_GEODATA_FILE`), the data directory additions (`derived/ab/cd/<sha256>/…`, `geo/`), `scripts/fetch-pdfium.sh` and `scripts/fetch-geodata.sh`, building with `--features heic` (libheif ≥ 1.17 + libde265), `squirrel admin jobs`, `squirrel admin media backfill`, the GeoNames attribution (CC BY 4.0) and pdfium's licences, and the "Geodata" publishing steps from Task 20.

**Verify** — full gate:
```bash
cargo fmt --all --check && rtk proxy cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm gen:api && git diff --exit-code src/lib/api/schema.d.ts \
  && pnpm check && pnpm test:unit && pnpm build && SQUIRREL_E2E_PORT=8199 pnpm test:e2e
```
→ all green. **Commit** — `docs: document the media worker, data files and licences`.

---

## How Phase 4 (video) and Phase 5 (ML) plug in

Nothing below is implemented in Phase 3; the seams are deliberate.

- **Video (Phase 4):** `MediaKind` gains `Video` (`video/*` types); a migration widens `media.kind` and `assets.kind` CHECKs to include `video` and adds `media.duration_ms`, `media.codec`, `media.playback` (direct/HLS state). `process_media` creates `assets` rows for videos (`taken_at` from the container's `creation_time`, same fallback chain) and queues `extract_metadata` (an `ExecRequest::Probe` running `ffprobe` inside the same sandbox), `thumbnails` (poster frame via `ffmpeg` → the same `thumb`/`preview` variants + blurhash, so the grid needs no change besides a duration badge) and `transcode_video` (dedupe `transcode_video:<sha>`, low priority, its own long timeout and lease; output in `derived/ab/cd/<sha>/hls/`, removed with the blob by the same GC). "Opening a not-yet-transcoded video bumps its job priority": add `queue::bump(dedupe_key, priority)` (`UPDATE jobs SET priority = GREATEST(priority, $2) WHERE dedupe_key = $1 AND status = 'queued'`). The playback endpoint follows the thumbnail endpoint's owner scoping.
- **ML (Phase 5):** new kinds `clip_embed:<sha>` and `faces:<sha>` with priorities below 0, enqueued by `process_media` for images/videos once thumbnails are ready (they read the `preview` WebP, never the original). They can run in a separate process — `squirrel worker --kinds clip_embed,faces` (add a `--kinds` filter; the queue already claims only registered kinds) — e.g. on a machine with more RAM, sharing the data directory read-only. Embeddings are content-derived (`asset_embeddings` keyed by `sha256`, like `media`), while faces, people and clusters are per user (keyed by `owner_id`, joined through `assets`), so dedup across users never leaks identities. `cluster_faces` is a per-user debounced job (`dedupe_key = cluster_faces:<owner_id>`, `delay` 10 min). Models live in `<data_dir>/models`, verified against pinned SHA-256 exactly like the geodata. The backfill job gains one more "stale step" rule per new kind.
- **Phone sync (Phase 6):** the upload request's optional `taken_at` (+ offset) becomes step 3 of the capture-time chain (`TakenSource::Upload` already exists; the device value is stored on the node or upload session).

## Security considerations

| Threat | Mitigation in this plan |
|---|---|
| **Decompression bombs / huge images** | Header dimensions checked before allocation (100 MP), `Limits::max_alloc` 1 GiB, inputs > 512 MiB never decoded, EXIF of TIFFs > 64 MiB skipped; the sandbox's `RLIMIT_AS` 2 GiB and `RLIMIT_CPU` 120 s back these up. |
| **Decoder memory-safety bugs** (pdfium, libheif, libwebp) | Untrusted bytes are decoded only in `squirrel media-exec` children: memory/CPU/file-descriptor rlimits set before the input is read, 150 s wall clock with `kill_on_drop`, outputs only into a parent-created scratch directory with fixed file names, results only as JSON on stdout. pdfium is the non-V8 build (no JavaScript). The worker itself is a separate process from the web server. |
| **Hung or crashing jobs** | Handler timeouts < lease; heartbeats; expired leases reaped; panics caught per job; 5 attempts with backoff, then `failed` and visible to admins; last-attempt handlers write terminal states so the UI never waits forever. |
| **Path traversal** | Derived paths are built only from a parsed `ContentHash` (`ab/cd/<64 hex>`) and fixed variant names; the sandbox receives absolute paths from the worker, never from users; geodata/pdfium locations come from operator configuration. |
| **Cross-user access** | Every new endpoint resolves the node/asset/album with `owner_id = caller` (foreign ids are `not_found`, same as missing); `media`/`document_text` are content-keyed and only reachable through the caller's own nodes; `album_items` composite foreign keys make cross-user album entries impossible in the database; search joins `nodes.owner_id` before `document_text`. API tests and E2E check each route as Bob. |
| **Dedup side channels** | Thumbnails and text of shared content are served only after the caller's own node check; the existence of a thumbnail for content the caller does not own is never observable (404 `not_found` before any media lookup). Processing time differs only for the caller's own uploads. |
| **XSS** | EXIF strings are stripped of control characters, length-capped, stored as data and rendered as text by Svelte; search snippets are split server-side into `{text, highlight}` parts (no HTML); thumbnails are `image/webp` generated by us, served with `nosniff`, the sandbox CSP and `CORP: same-origin`. The OSM link is built from numbers only. |
| **Location privacy** | GPS and places are shown only to the owner; geocoding is offline; no map tiles or third-party requests unless the user clicks "Open in OpenStreetMap" (`noreferrer`). |
| **Cached private images** | Thumbnails are `private` (never shared caches) and versioned; logout sends `Clear-Site-Data: "cache"` (see Open question 10 for Safari < 17). |
| **Supply chain** | pdfium and the geodata snapshot are pinned by SHA-256 and re-verified (geodata at load); new crates listed for approval; libwebp is compiled from the crate's vendored sources. |
| **Queue abuse** | Jobs are created only by the server (uploads, backfill, admin retry); payloads are server-built JSON; job kinds are validated by a CHECK and dispatched only to registered handlers; admin endpoints require the admin role and CSRF. |
| **Resource exhaustion by many uploads** | Dedupe keys collapse repeated work; worker concurrency is bounded (default ≤ 2 jobs); thumbnails are served from memory without download slots but are small (≤ ~300 KB) and behind authentication. |

## Not in Phase 3

- Video assets, posters, probing, transcoding and the player — **Phase 4**.
- CLIP tags, text-to-image search, faces and people, the Explore pages (People, Places map, Tags) and timeline filters (place, person, tag, camera, kind) — **Phase 5** (the `assets.kind` and `media` camera/place columns are ready for the filters).
- Phone sync and the device `taken_at` — **Phase 6**.
- Docker image with libheif, pdfium and the geodata baked in; backup guidance for `derived/` (it can be skipped: add a `squirrel admin media backfill --verify-files` that re-queues thumbnails whose files are missing) — **Phase 7**.
- OCR of scanned PDFs and images, document language detection/stemming, admin queue UI, download of a selection as a zip, editing dates or locations, sharing albums (out of scope for v1).

## Done when

- `cargo test --workspace` green (with pdfium in CI): queue semantics (claim order, SKIP LOCKED, fencing, reaping, retries/backoff, reschedule, release, dedupe, notifications), worker runtime (timeouts, panics, shutdown), media crate (EXIF, capture chain, screenshots, decode limits, orientation, variants, blurhash, geocoder, PDF), sandbox (child process, timeout), handlers (process, metadata, thumbnails, geocode, text, backfill), schema constraints and cascades, and every new endpoint including cross-user 404s.
- The `rust-heic` CI job green.
- `pnpm test:unit` and `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` green (existing suites + the 9 new scenarios).
- `openapi.json` and `schema.d.ts` regenerated and committed; CI green.
- Manual: a real phone export (≥ 500 photos incl. HEIC with `--features heic`) and a few real PDFs processed on the dev machine; the timeline scrolls smoothly at 375 px in Firefox for Android and Chrome (no arrival animations on touch); a worker killed with `kill -9` mid-batch recovers after the lease; `squirrel admin jobs` shows no failures (or explainable ones).
- Then add an **"As built"** section at the top of this plan, as Phases 1 and 2 did.
