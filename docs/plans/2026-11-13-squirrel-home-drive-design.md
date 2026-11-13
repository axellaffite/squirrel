# Squirrel — Self-Hosted Home Drive: Design

Date: 2026-11-13
Status: Validated

## Goals

A self-hosted drive running on a home x86 mini PC, offering:

- Per-user storage with quotas
- Photo classification (metadata, places, ML tags, faces)
- Document storage (PDF preview, full-text search)
- Video streaming
- Automatic photo sync from phones through a custom Kotlin Multiplatform app
- Web UI for registration and login; registration by invitation only, except for the very first user

## Decisions

| Topic | Decision |
|---|---|
| Backend | Rust, axum, single binary with `serve` / `worker` / `migrate` / `admin` subcommands |
| Database | Postgres 17 + pgvector (metadata, job queue, embeddings) |
| Storage | Content-addressed blobs (`/data/blobs/ab/cd/<sha256>`), logical tree in DB |
| Web UI | Svelte 5 + SvelteKit (`adapter-static`) + TypeScript, embedded in the binary via `rust-embed` |
| Mobile | Kotlin Multiplatform (Compose on Android, SwiftUI shell on iOS) |
| API contract | OpenAPI generated with `utoipa`, committed; web and KMP clients generated from it; CI drift check |
| Network access | LAN + WireGuard (Tailscale recommended), HTTPS required (`tailscale cert` or Caddy DNS-01) |
| ML | Local only, ONNX Runtime (`ort`): CLIP ViT-B/32, SCRFD + ArcFace |
| Video | Direct play when compatible, background HLS pre-transcode otherwise (VAAPI/QSV, fallback libx264); no live transcoding in v1 |
| Phone sync semantics | Backup-only: deleting on the phone never deletes on the server |

## Architecture

```
 Phone (KMP app) ─┐                         ┌──────────── mini PC (Docker Compose) ───────────┐
 Browser (SPA) ───┼── WireGuard/Tailscale ──┤  squirrel serve  (axum, :8080, embedded SPA)    │
                  │        HTTPS            │        │   ▲                                    │
                  └─────────────────────────┤        ▼   │ LISTEN/NOTIFY                      │
                                            │  Postgres 17 + pgvector  (metadata, jobs, vecs) │
                                            │        ▲                                        │
                                            │  squirrel worker (thumbs, EXIF, ffmpeg, ONNX)   │
                                            │        │                                        │
                                            │  /data/blobs/ab/cd/<sha256>   /data/derived/…   │
                                            └─────────────────────────────────────────────────┘
```

### Repository layout

```
crates/
  squirrel-core      domain types, no I/O
  squirrel-db        sqlx repositories + migrations
  squirrel-storage   blob store, derived files
  squirrel-auth      Argon2id, sessions, device tokens, invitations
  squirrel-api       axum routes, OpenAPI (utoipa), upload sessions
  squirrel-jobs      Postgres queue (SKIP LOCKED, retries, visibility timeout)
  squirrel-media     EXIF, thumbnails (image + libvips for HEIC), ffprobe/ffmpeg, offline geocoding
  squirrel-ml        ort: CLIP embeddings/tags, face detection/embeddings
  squirrel           binary (clap): serve | worker | migrate | admin
web/                 Svelte SPA
mobile/              KMP app
deploy/              docker-compose.yml, Dockerfile, Caddy/Tailscale examples
docs/                design and plans
```

## Users, authentication, invitations

- **Bootstrap**: `POST /api/auth/register` without invitation succeeds only when `users` is empty, checked in a transaction under an advisory lock. First user becomes `admin`. `GET /api/setup/status` tells the SPA whether to show first-user setup.
- **Invitations**: any user can invite (admins can restrict to admins only). 32-byte random token, base64url, stored as SHA-256 only, shown once as `/join/<token>`. Default 7-day expiry, single use, revocable. Redemption atomically increments `used_count` and records `invited_by`. No email in v1.
- **Users**: `users(id, username unique, display_name, password_hash, role, quota_bytes?, invited_by, created_at, disabled_at)`.
- **Web sessions**: opaque random session id in `HttpOnly; Secure; SameSite=Lax` cookie, stored hashed, 30-day sliding expiry, CSRF double-submit header. Server-side sessions so "log out everywhere" is immediate.
- **Mobile**: login returns a device-bound refresh token (`devices` table) and 15-minute access tokens. Devices listed and revocable in the web UI.
- **Hardening**: Argon2id (19 MiB, 2 iterations), min password length 10, rate limiting per IP and per username with backoff, generic login errors.
- **Authorization**: every query scoped by `owner_id`. Cross-user sharing is out of scope for v1.

**Phase 1 as built** (corrects the bullets above; the code is the source of truth):
- Bootstrap is `POST /api/setup/admin`, which requires a setup code (`SQUIRREL_SETUP_TOKEN`, or one logged on first start). Invited people join with `POST /api/join/{token}`. There is no `/api/auth/register`.
- CSRF uses a synchronizer token stored with the session and sent in `X-CSRF-Token`, not double-submit.
- Health is `GET /api/healthz` and checks the database only.

## Storage and sync

- **Blobs**: `blobs(sha256, size, mime, ref_count, created_at)`. Upload streams to `/data/tmp`, hashed while writing, fsync, atomic rename. Existing blob → increment `ref_count`. Daily GC of `ref_count = 0` after a grace period; weekly scrub re-hashes a sample.
- **Tree**: `nodes(id, owner_id, parent_id, name, kind, blob_sha256?, created_at, modified_at, deleted_at)`, unique `(owner_id, parent_id, name)`. Trash for 30 days. Default roots: `Photos/`, `Documents/`, `Files/`.
- **Assets**: image/video nodes get an `assets` row (taken_at, GPS, place, camera, dimensions, duration, kind, ML data).
- **Quota**: logical size of the user's non-deleted nodes. Over-quota → 507 before accepting bytes.
- **Upload protocol** (shared by SPA and app):
  1. `POST /api/uploads {sha256, size, name, parent_id, taken_at?, device_asset_id?}` → `200 {node, deduplicated: true}` if the hash already exists *in this user's library*, else `201 {upload_id, chunk_size: 8 MiB}`. The probe never reveals other users' content.
  2. `PUT /api/uploads/{id}/chunks/{n}`, any order, resumable; `GET /api/uploads/{id}` lists received chunks.
  3. `POST /api/uploads/{id}/complete` → full hash verification (422 on mismatch), node creation, `process_media` enqueued. Sessions expire after 24 h idle.
- **Phone sync**: `device_assets(device_id, device_asset_id, sha256, synced_at)`; `POST /api/sync/check` with batches of ≤500 `{device_asset_id, sha256}` returns the missing ones.
- **Downloads**: `GET /api/files/{id}/content` with Range, ETag (= hash), Content-Disposition; PDFs inline.

## Media pipeline

Independent, individually retried jobs after `process_media`:

1. `extract_metadata` — EXIF/XMP or ffprobe; `taken_at` fallback chain; screenshot detection.
2. `geocode` — offline GeoNames cities500 lookup → country/region/city.
3. `thumbnails` — 256 px grid + 1440 px preview (WebP), blurhash; HEIC via libvips; video poster frame.
4. `clip_embed` — 512-d CLIP vector (3 keyframes for video) in `asset_embeddings` (HNSW). Zero-shot tags against ~200 label prompts → `asset_tags(source='clip')`. Text search via CLIP text encoder, scoped to the user.
5. `faces` — SCRFD detection + ArcFace embedding → `faces(asset_id, bbox, embedding)`.
6. `cluster_faces` — per-user, debounced (≤ every 10 min), incremental DBSCAN on cosine distance → `people` (name, merge, split, hide). Manual assignments are pinned.

- ML jobs run at lower priority than thumbnails; worker concurrency configurable; progress visible in UI.
- Models downloaded on first worker start into `/data/models`, verified against pinned SHA-256.
- Faces are per-user and can be disabled per user (deletes face data).
- Gallery: keyset-paginated timeline on `(owner_id, taken_at desc)`, filters (place, person, tag, camera, kind), albums, combined search.
- Documents: PDF first-page thumbnail (`pdfium-render`), text extracted into `tsvector`. OCR deferred.

## Video streaming

- ffprobe on ingest. H.264/HEVC + AAC in MP4 → direct play via Range requests.
- Otherwise `transcode_video` → HLS 1080p + 720p (H.264/AAC, 6 s segments) in `/data/derived/hls/<sha256>/`, VAAPI when `/dev/dri` is present, else `libx264 -preset veryfast`.
- `GET /api/assets/{id}/playback` → `{direct_url?, hls_url?, poster, duration}`; playback URLs signed with short-lived, asset-scoped HMAC tokens.
- Opening a not-yet-transcoded video bumps its job priority; UI shows "preparing…".
- 3 s muted hover previews in the grid. Pre-transcoding can be disabled per user or globally.

## Web UI

Setup / join / login; Photos (virtualized justified grid, timeline scrubber, lightbox with EXIF and map, multi-select, drag-and-drop resumable upload with hashing in a Web Worker); Explore (People, Places map, Tags, Albums); Search; Files (browser, PDF preview, trash, document search); Videos (hls.js); Settings (profile, devices, invitations, storage, face toggle); Admin (users, queue, transcode settings). Responsive, dark-first, installable PWA.

## Mobile app (KMP)

- `shared/`: generated Ktor client, auth + refresh (Keystore/Keychain), SQLDelight upload queue, hashing, sync engine (`sync/check` → upload missing → record).
- Android: Compose, WorkManager periodic job (configurable constraints) + MediaStore `ContentObserver`.
- iOS: SwiftUI shell, `PHPhotoLibrary` observer, `BGProcessingTask`, background `URLSession` uploads.
- Screens: server + login, sync status, backup settings (albums, Wi-Fi only), basic gallery and file browser.

## Errors and observability

- API error envelope `{error: {code, message}}` from a `thiserror` enum; internal details logged only.
- Jobs: exponential backoff, 5 attempts, then `failed` with last error visible to admins.
- `tracing` structured logs (JSON in prod), `/healthz` (DB + disk), Prometheus `/metrics`.

## Testing

- TDD, 80%+ coverage.
- Unit: core, auth, storage. Integration: real Postgres via `sqlx::test` — auth, invites, bootstrap race, dedup, quotas, cross-user isolation.
- Media fixtures (JPEG+EXIF, HEIC, MP4, MKV, PDF); ML stability tests on fixture images.
- Web: Vitest + Playwright E2E (setup → invite → join → upload → photo visible).
- CI: `cargo fmt`, `clippy -D warnings`, tests, OpenAPI drift check.

## Delivery phases

1. Workspace, DB, auth, first-user bootstrap, invitations, SPA shell (setup / login / join).
2. Blob storage, upload protocol, file browser, documents + PDF preview, trash, quotas.
3. Media pipeline: EXIF, thumbnails, geocoding, timeline gallery, albums.
4. Video: probe, direct play, HLS transcode, player.
5. ML: CLIP tags + search, then faces + people.
6. KMP sync app (Android first, then iOS).
7. Deployment hardening: Docker Compose, Tailscale/Caddy docs, backups (`pg_dump` + blob rsync).

## Out of scope for v1

Cross-user sharing, public links, live transcoding, OCR, email delivery of invitations, WebDAV.
