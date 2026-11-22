# On-demand previews with libvips

**As built (2026-11-22).**
- Task 1 (M-series Mac, libvips 8.18, in-process / with child, 1440·2160·3200): 12 MP JPEG 57·61·64 / 136·141·146 ms; 50 MP JPEG 147·224·234 / 228·308·320 ms; 12 MP HEIC 437·438·450 ms; 48 MP HEIC (single-image, not a grid) ≈ 1.75 s. WebP at 3200 px ≈ 620 ms, so JPEG stays. Peak RSS 50–90 MB (JPEG), 120 MB (WebP 3200), 130 MB (12 MP HEIC), 420 MB (48 MP HEIC).
- Spawn + `vips_init` 75 ms on macOS (Homebrew loads many modules), 33 ms on Debian arm64: one child per request kept. **Open:** measure on the i3; past 60 ms switch to the recycled pool, and check big HEICs stay under the 10 s CPU limit (two threads count double).
- Deviations: the render limit (40, then 4/s) is per user, not per IP; a photo that cannot be rendered falls back to its stored preview while one exists; the worker probes libvips too; four non-variadic libvips setters the crate does not wrap (`block_untrusted`, `operation_block_set`, `type_find`, `foreign_find_load`) are called through its `bindings` (`unsafe`, `vips.rs` only); plain `thumbnail` (box from the header, no `fail_on=error`) because the crate's `thumbnail_with_opts` breaks on libvips 8.16 (Debian trixie).
- `drop-previews` is implemented, not run.

**Goal:** stop storing the 1440 px lightbox preview of images; render it from the original on request, at a screen-fitting size bucket, behind a small LRU disk cache. Saves disk on the NUC (≈ 7–15 GB per 50 000 photos) and gives phones and 4K screens sharper previews.

**Architecture:** `GET /api/files/{id}/thumbnail?size=preview&px=N` rounds N up to a bucket, looks in `<data_dir>/cache/previews/`, and on a miss runs a sandboxed `squirrel media-exec` child (new op `preview`) that calls libvips `thumbnail` (shrink-on-load for JPEG, heifload for HEIC), installs the result in the cache and serves it. Thumbnails, PDF previews and video posters stay stored. ML jobs render a temporary 1440 px frame the same way and delete it.

**Stack:** Phase 5 plus `libvips = "=2.3.0"` (cargo feature `vips`) and the system libvips (`brew install vips`; Debian `libvips42`). No new npm or Gradle dependency.

**Approved by the user (2026-11-18):** thumbnails stay stored; previews on demand with libvips and a 1–2 GB cache. Not instant: ≈ 0.1–0.3 s per JPEG and 0.3–0.6 s per HEIC on the i3 is fine. Scheduled after the SigLIP2 switch, before Phase 4.

Findings from the code that shape this plan:
- Face boxes are stored in 0–1 of the oriented image, so any preview size can crop them.
- Faces keep only boxes ≥ 40 px in the 1440 preview, so the ML render must stay at 1440.
- The web crops faces from `size=preview`.
- `PhotoViewer.kt` doesn't prefetch neighbouring photos.
- The exec runner lives in `squirrel-worker`, which `squirrel-api` doesn't depend on.

## Decisions

| Topic | Decision | Why |
|---|---|---|
| Sandbox | **One child per request**, using the existing `media-exec` launcher: empty env, own process group, no_new_privs. Preview limits: RLIMIT_AS 1.5 GiB, CPU 10 s, wall clock 10 s. libvips cache 0, concurrency 2, `vips_block_untrusted_set(true)`, loader allowlist jpeg/png/webp/tiff/gif/heif. | No cross-user state in a long-lived decoder; memory is freed every time. The spawn costs ≈ 20–40 ms. Fallback: a recycled pool (the `MlClient` pattern) if the spawn exceeds 60 ms on the i3. |
| Backpressure | `SQUIRREL_PREVIEW_CONCURRENCY` slots (default 2). Waiting more than 3 s gives 503 `busy` with `Retry-After: 1`. Per-client limit: burst 40, 4/s, else 429. Requests for the same key share one render; cache hits take no slot. | Leaves CPU for HTTP, thumbnails and ML. |
| Binding | **`libvips` 2.3.0** (olxgroup-oss), used only in `squirrel-media/src/vips.rs`, inside the child. Fallback: `rs-vips` 0.7.0. No FFI of our own: the variadic C API is unsafe to hand-roll. libvips is LGPL-2.1+, dynamically linked. | Organisation-maintained, recent release. |
| Buckets | Long side **1440 / 2160 / 3200**: `px` is rounded up and capped. No `px` gives 1440, for old clients and face crops. Never upscale: an original smaller than the bucket shares the smallest bucket covering it. Clients fit the photo to the screen in device pixels, then round up. | At most 3 cache entries per photo; no arbitrary sizes. |
| Format | JPEG q85 when opaque, WebP q80 with alpha; metadata stripped, sRGB, orientation applied. Task 1's benchmark can revisit this. | WebP at 3200 px costs ≈ 0.5 s on the i3; JPEG ≈ 10× faster. |
| Cache | `<data_dir>/cache/previews/ab/<sha256>-<bucket>-r<PREVIEW_VERSION>.{jpg,webp}`, mode 0400, tmp then rename, regular files ≤ 5 MiB only. In-memory index rebuilt at start. LRU, with access time refreshed at most hourly. `SQUIRREL_PREVIEW_CACHE_MB` (default 1536) trims to 90 %. Nothing is written below `min_free_bytes`. Blob GC removes entries; another version is purged at start. `admin previews stats`. Not counted in quotas. | Hit rate, privacy and bounded disk. |
| ML input | `clip_embed` and `faces` render `Preview { bucket: 1440, format: Png }` into the worker's own temp dir (removed afterwards) and pass that path to `ml-exec`. Existing embeddings and faces are kept. | Same geometry and threshold; works with a read-only data dir. |
| PDF and video | PDF previews stay stored at 1440 and ignore `px`. Phase 4 posters are stored. | pdfium is slow; documents are few. |
| Stored previews | 1. The thumbnails job stops writing the image preview (THUMBS_VERSION unchanged). 2. After the release is checked, `admin media drop-previews` queues an idempotent `drop_previews` job that deletes `preview-v*.webp` for images, 500 at a time. | Nothing is deleted before the server and ML stop reading it. |
| Fallback | Feature `vips`, off by default, on in Docker and in one CI job. Without it, or if `--probe` reports a broken libvips or missing loader, the built-in pipeline renders the preview, ≈ 2–3× slower. | Dev and CI need nothing; a broken install still serves. |
| HTTP | Same route and owner check (another user's file gives `not_found`); `thumbs_state = ready` is required. ETag `"<sha256>-p<bucket>-r<PREVIEW_VERSION>"`, `Cache-Control: private, max-age=604800`, 304 on match. Thumbnails stay `immutable`. Same security headers. | A renderer change reaches clients within 7 days. |

## Risks

- **Memory:** non-interlaced PNG and TIFF decode whole (100 MP RGBA ≈ 400 MB; 50 MP HEIC ≈ 300 MB). Refuse above 100 MP from the header, RLIMIT_AS 1.5 GiB, 2 concurrent renders, `MALLOC_ARENA_MAX=2`.
- **Bombs and parser bugs:** loader allowlist, untrusted operations blocked, `fail_on=error`, the 512 MiB input cap, sandbox.
- **Cold People pages** trigger many 1440 renders. They're cached after the first visit; clients retry on 503 and 429.
- **8 GB of RAM:** renders ≈ 1.2 GB, plus the thumbnail cap 2 GiB, plus ML ≈ 1–2 GB, plus Postgres. Measured in Task 1.
- **Debian libvips** may pull GPL modules (poppler, ImageMagick). Those loaders are blocked; Phase 7 checks the package and installs with `--no-install-recommends`.
- **Orientation:** libvips must match our EXIF 1–8 and HEIF `irot` handling. Golden comparison.

## Tasks (each: fmt, clippy `-D warnings`, narrow tests, one commit; full gate at the end)

1. **Spike and benchmark:** `squirrel-media/examples/preview_bench.rs` (feature `vips`). Time each bucket for 12 MP and 50 MP JPEG and for HEIC, with and without the child spawn; record peak memory. Confirm the crate's API and licence.
2. **Shared sandbox crate:** move `squirrel-worker/src/exec.rs` unchanged into `crates/squirrel-sandbox`.
3. **Renderer:** `squirrel-media` `preview.rs` (buckets, `PREVIEW_VERSION`, built-in path), `vips.rs` (feature-gated, allowlist), `ExecRequest::Preview`/`Probe`. In `squirrel/src/media_exec.rs`, per-operation limits and `--probe`. Tests: buckets, no upscaling, orientation 1–8 against the built-in path, the 100 MP refusal, garbage input is permanent, a missing loader falls back.
4. **Cache:** `squirrel-storage/src/preview_cache.rs`, plus blob-GC removal in `squirrel-api/src/maintenance.rs`. Tests: eviction order, restart rebuild, symlinks and oversized files refused, low disk means no cache.
5. **API:** `routes/thumbnails.rs` takes `px`, with single render per key, slots in `state.rs`, and 503/429. Config, then regenerate `openapi.json`. Tests: buckets, ETag/304, another user gets `not_found`, busy gives 503 with a retry header, PDFs stay 1440.
6. **ML:** temporary renders in `clip_job.rs` and `faces_job.rs`. Tests: same boxes on a rotated fixture; temp dir removed.
7. **Thumbnails job and cleanup:** stop writing image previews; `drop_previews` job; `admin media drop-previews` and `admin previews stats|clear`.
8. **Web:** `lib/photos/previewSize.ts` (tests); the lightbox, preview and prefetch send `px`, with one retry on 503/429. No visual change.
9. **Phone:** `PreviewSize.kt` (shared test vectors), `thumbnailUrl(px)`, and `PhotoViewer` prefetching one page each side.
10. **E2E, CI and docs:** E2E for a phone-sized preview and cross-user denial; a CI job with `--features vips`; README; As-built in 5 lines.

## Open questions, settled with the recommended defaults (the user can override)

1. `libvips` 2.3.0, LGPL, dynamically linked, feature off by default: **accepted**.
2. Buckets 1440/2160/3200 and a 1.5 GB cache: **accepted**; revisit after a month of cache stats.
3. JPEG for opaque previews: **accepted**, unless Task 1 shows WebP at 3200 px under 150 ms on the i3.
4. `drop-previews` runs **by hand, once**, after the release is checked.
5. Thumbnails on libvips (faster imports): **follow-up** after this ships.
