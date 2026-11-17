# Phase 6 — Phone App (Kotlin Multiplatform) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A phone app, Android first and then iOS, that signs in to a Squirrel server, backs up the
phone's photos and videos automatically (backup only: deleting on the phone never deletes on the
server), shows what is backed up and what is left, and lets the user browse their photo timeline
and their files. The server learns about devices: each phone signs in with its own revocable
tokens, appears in a new **Settings › Devices** page on the web, asks `POST /api/sync/check` which
of its photos are missing, and passes the phone's capture time so it becomes step 3 of the
capture-time chain.

**Order:** decided on 2026-11-16, Phase 6 is built **before** Phase 4 (video) and Phase 5 (ML).
Videos the phone backs up are uploaded and stored as ordinary files now; Phase 4's backfill job
will give them posters, metadata and playback later, with no change to the app.

**Architecture:** Server side, a new `devices` table and an opaque bearer-token flow
(15-minute access tokens, rotating refresh tokens with reuse detection) sit beside the existing
cookie sessions. A new `Caller` extractor accepts either a session cookie (with CSRF, as today) or
`Authorization: Bearer`, and the data routes (files, uploads, photos, albums, thumbnails, content,
trash, storage, search) switch to it; account, invitation, admin and device-management routes stay
session-only. `device_assets` records which phone asset became which content, `POST /api/sync/check`
answers in batches of 500, and `POST /api/uploads` gains `device_asset_id` and `taken_at`.
Phone side, `mobile/` is one Gradle build: `shared` (Kotlin Multiplatform, no UI: Ktor client,
token store, SQLDelight queue, streaming SHA-256, chunked uploader, sync engine, and in `iosMain`
the iOS integrations: PhotoKit, `BGTaskScheduler` and a background `URLSession`, all through
Kotlin/Native `platform.*` interop), `theme` (the one Compose Multiplatform design-system module
derived from MASTER.md: generated tokens, theme, components), `ui` (Compose Multiplatform screens
and view models, **the same UI on Android and iOS**), `androidApp` (MediaStore source,
permissions, WorkManager) and `iosApp` (an Xcode project whose only Swift is the entry point
hosting `ComposeUIViewController` and a few lines of app-delegate glue).

**Tech Stack (all versions checked against Maven Central / Google Maven metadata on 2026-11-16,
listed again under "Open questions"):** Kotlin 2.4.21 (latest stable) with the newest Gradle and AGP
lines it is **tested with**: Gradle wrapper 9.7.1 and Android Gradle Plugin 9.3.3 (Kotlin 2.4.21 is
tested up to Gradle 9.7.0 and AGP 9.3.1; the latest Gradle 9.9.1 and AGP 9.5.1 are untested with it,
Open question 14), Compose Multiplatform 1.12.2 (Jetpack Compose UI 1.12.1 underneath),
JetBrains `navigation-compose` 2.10.0 and `lifecycle-viewmodel-compose` 2.11.0, Ktor client 3.6.1
(OkHttp engine on Android, Darwin on iOS, `ktor-client-mock` for tests), kotlinx.serialization
1.13.0, kotlinx.coroutines 1.11.0, kotlinx-datetime 0.8.0, openapi-generator Gradle plugin 7.27.0
(`kotlin` generator, `multiplatform` library), SQLDelight 2.4.1, AndroidX WorkManager
2.12.0, `activity-compose` 1.13.0, `core-ktx` 1.19.1, Coil 3.6.3 (`coil-compose`,
`coil-network-ktor3`); tests: kotlin.test, Turbine 1.2.1, Robolectric 4.18, AndroidX Test core 1.7.0
/ runner 1.7.0 / ext-junit 1.3.0. No new Rust crate. Web: no new npm dependency.

**Design reference:** `docs/plans/2026-11-13-squirrel-home-drive-design.md` (sections "Users,
authentication, invitations", "Storage and sync", "Mobile app (KMP)", "Visual design",
"Delivery phases"). **Phase 3 reference:** `docs/plans/2026-11-14-phase-3-media-pipeline.md`,
section "How Phase 4 (video) and Phase 5 (ML) plug in" (the device `taken_at` as step 3 of the
chain). The code is the source of truth, not earlier plans' snippets.
**Visual design:** `MASTER.md` governs every screen, web and native. Native tokens are generated
from `web/src/lib/styles/tokens.css` (which implements MASTER.md), never typed in by hand (Task 15,
Open question 8). Phone-specific additions go into MASTER.md first and need the user's validation
(Task 15, Open question 9).

## Conventions / environment

- Run commands from the repo root unless stated. Never `cd` outside the worktree; never bare
  `git stash` (use a WIP commit).
- **Dev Postgres is `localhost:55432` only** (`deploy/docker-compose.dev.yml`). **Never use port
  5433.**
- After each Rust task, before committing:
  `cargo fmt --all && rtk proxy cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Web commands are prefixed with `source ~/.nvm/nvm.sh && nvm use 22 >/dev/null &&` and run in
  `web/`. E2E: `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` (8099 is taken on this machine).
- Every API change regenerates the contract in the same task, or `tests/openapi.rs` fails:
  `cargo run -q -p squirrel -- openapi > openapi.json && (cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm gen:api)`.
  From Milestone D on, the phone's API client is generated from `openapi.json` too: run
  `(cd mobile && ./gradlew :shared:checkApiClient)` (Task 17) after regenerating it.
- Existing migrations are **frozen** (CI refuses edits). Phase 6 adds new files only:
  `20261116000001_devices.sql` (A), `20261116000002_device_sync.sql` (B),
  `20261116000003_upload_tokens.sql` (H). If the calendar moved on, use the current date for the
  prefix, keeping this order.
- **Gradle:** never a global Gradle. The wrapper is created once in Task 14 and committed; every
  later command is `./gradlew …` from `mobile/`. Mobile commands in this plan are written as
  `(cd mobile && ./gradlew …)`.
- **JDK:** the machine's default is OpenJDK 25.0.2. Gradle runs on Java 25 since 9.1.0 (compatibility
  matrix), AGP states only a minimum (JDK 17) and its 9.5 line fixed a JDK 25 warning, and Kotlin
  compiles on whatever JDK runs the daemon, so the daemon can run on 25; the **compile toolchain is pinned
  to JDK 21** (`kotlin { jvmToolchain(21) }`, Android `compileOptions` 17 bytecode), and Temurin
  21.0.9 is **already installed** (`/usr/libexec/java_home -v 21`), so no download is needed
  (`org.gradle.java.installations.auto-download=false` in `mobile/gradle.properties`). If the
  daemon misbehaves on 25, set `org.gradle.java.home` in `~/.gradle/gradle.properties` (not
  committed) to that JDK 21 (Open question 1).
- **Android SDK:** `~/Library/Android/sdk` (platforms 34–37.0, build-tools 36.1.0, emulator, AVD
  `Medium_Phone_API_36.1`). `mobile/local.properties` (git-ignored) holds `sdk.dir`. The emulator
  reaches the dev server on the host as `http://10.0.2.2:8080` (debug builds only allow clear text,
  and only to `10.0.2.2` and `localhost`).
- **iOS:** Xcode 27.0 is installed (simulator runtime iOS 27.0); Kotlin 2.4.x officially supports
  Xcode 26.4 (Open question 17). The Kotlin framework is built by the Xcode build phase
  `./gradlew :ui:embedAndSignAppleFrameworkForXcode`.
- **Mobile tests:** `(cd mobile && ./gradlew :shared:jvmTest)` (fast, every shared task),
  `:shared:iosSimulatorArm64Test` (Milestone H on), `:androidApp:testDebugUnitTest` (Robolectric),
  `:androidApp:connectedDebugAndroidTest` (emulator running), `:androidApp:lintDebug`.
- Commit messages follow the repo pattern `<type>(<scope>): <description>`; new scopes `mobile`,
  `android`, `ios`; short body optional, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Keep files under ~400 lines; split modules rather than growing one. Swift is limited to
  `mobile/iosApp/iosApp/{SquirrelApp,AppDelegate}.swift` and XCTest code (see "iOS through Kotlin/Native").

### What was verified while writing this plan

- **Versions:** every library version above is the latest non-prerelease in its Maven metadata on
  2026-11-16 (`repo1.maven.org` / `dl.google.com/dl/android/maven2`), except Gradle and AGP, chosen
  as the newest patch releases of the lines Kotlin 2.4.21 is tested with (Kotlin's compatibility
  guide: Gradle up to 9.7.0, AGP up to 9.3.1; "newer versions may give deprecation warnings or some
  features may not work"). Latest overall: Gradle 9.9.1, AGP 9.5.1 (needs Gradle ≥ 9.6.0, supports
  API 37). Google Play requires target API 36 for new apps and updates from 2026-08-31.
- **Compose Multiplatform on iOS:** stable since CMP 1.8.0 (JetBrains, May 2025); this plan uses
  1.12.2. CMP 1.11 (May 2026) added an **opt-in native iOS text input** (Compose's own text input
  stays the default) and iOS credential autofill prompts.
- **Server facts from the code:** sessions are cookie + synchronizer CSRF (`X-CSRF-Token`) checked
  in `CurrentUser` (`crates/squirrel-api/src/extract.rs`) for every non-safe method; there is no
  CORS layer, so browsers cannot send `Authorization` cross-site; `MeResponse.csrf_token` is
  required; `POST /api/uploads` takes `{parent_id, name, size, sha256}` only; the JSON body limit
  is 64 KiB (too small for a 500-item sync batch, hence the sync route's own group);
  `nodes_owner_blob_idx (owner_id, blob_sha256)` exists (the sync check's library lookup is an
  index probe); `resolve_taken` in `crates/squirrel-media/src/capture.rs` has three steps today
  (EXIF, file name, upload time as `TakenSource::Upload`).
- **UNVERIFIED** (check against compiler and docs when implementing): every Kotlin, Swift and Gradle
  snippet below (written against the current docs of each library, not compiled); **Kotlin/Native
  and Xcode 27**: Kotlin 2.4.20 (2026-09-07) bundles the iOS 26.4 SDK; Kotlin 2.5.0 (RC on
  2026-11-16) is announced as the first to bundle the 27 SDK, and only a community report says
  2.4.20 works with Xcode 27 (Open question 17); the KMP Android
  library plugin id `com.android.kotlin.multiplatform.library` and its test task names on AGP 9.5
  (the plan adds a `jvm()` target so shared tests never depend on them); the minimum Gradle of AGP
  9.3 (≤ 9.7.1 expected; check its release notes); the CMP 1.12 API for opt-in native text input;
  that every Kotlin/Native delegate used in `iosMain` compiles without Objective-C selector clashes
  (`@ObjCSignatureOverride` where needed); Compose's
  `Modifier.dropShadow` API (Compose UI ≥ 1.9) for the MASTER.md diffuse shadow; the exact name of
  the `assets.taken_source` CHECK constraint (`\d assets` on the dev database; Postgres names an
  unnamed column CHECK `assets_taken_source_check`); GitHub-hosted macOS runners shipping Xcode 27.

## Key decisions (read before starting)

### Authentication for a native client

| Topic | Decision | Why |
|---|---|---|
| Cookies or tokens | The phone does **not** use the cookie session. It gets **bearer tokens**: `POST /api/auth/device/login` (username, password, device name/platform/model/app version) creates a `devices` row and returns an access token (15 min) and a refresh token. | Matches the design ("device-bound refresh token, 15-minute access tokens"). A cookie jar plus a CSRF token in a background worker is fragile, and a phone needs per-device revocation. |
| Token format | Opaque random tokens, 32 bytes base64url, prefixed `sqa_` (access) and `sqr_` (refresh) so they can never be confused or mistaken for a session id, and so secret scanners can find them. Stored as SHA-256 only (`squirrel_auth::hash_token`), like sessions and invitations. Compared by hash lookup. | Revocation is immediate (one indexed lookup per request, as for sessions); no signing key to manage or rotate; no new crate (**Open question 2**). |
| Access tokens | `device_access_tokens (token_hash PK, device_id, expires_at)`; several may be alive at once (a worker and the UI refreshing concurrently must not kick each other out); expired rows deleted by the maintenance loop. | — |
| Refresh tokens | One current refresh hash per device, **rotated on every refresh**. The previous token stays usable, so a refresh whose response was lost can be retried; tokens rotated out twice are kept (hashed) as **retired**, and presenting one is **reuse**: the device is revoked and a warning is logged. Idle expiry 90 days, extended on each refresh; no absolute expiry (**Open question 3**). | Phones must keep backing up for months untouched; theft of an old token is detected. |
| Where tokens live on the phone | Android: an AES-256-GCM key generated in the **Android Keystore** (no user authentication, so background work can use it) encrypts the refresh token; the ciphertext lives in the SQLDelight database. iOS: the **Keychain**, `kSecClassGenericPassword`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` (readable by background tasks after the first unlock, never in backups or on another device). Access tokens stay in memory. | Matches the design; `EncryptedSharedPreferences` is deprecated. |
| Which routes accept a bearer | New extractor `Caller` (session **or** bearer). The **data routes** move to it: nodes, folders, uploads (all five), content, thumbnails, trash, storage, photos, media status, albums, document search. **Session-only** (keep `CurrentUser`): `/api/auth/me`, logout, logout-all, invitations, admin settings and jobs, device management. **Bearer-only**: `/api/devices/current*`, `/api/auth/device/logout`, `/api/sync/*`. A route-matrix test pins this (Task 5). | Least privilege: a stolen phone token can read and add files but cannot invite people, change instance settings, or manage other devices (**Open question 4**). |
| CSRF | Not checked for bearer requests. If an `Authorization` header is present, the cookie is ignored entirely (no mixing). | CSRF exploits ambient cookies; a bearer token is never sent automatically, and there is no CORS, so a page cannot set the header cross-origin. |
| Logout everywhere | `POST /api/auth/logout-all` also revokes every device of the user. Disabling a user makes all their tokens fail at once (the extractor checks `disabled_at`, as for sessions). | "Log out everywhere" must mean everywhere. |
| Joining by invitation | **v1: on the web.** The app's sign-in screen says "Have an invitation? Open the link in your browser, then sign in here." | The join flow (setup code, password rules, invitation checks) already exists and is E2E-tested on the web; the app gains nothing but surface (**Open question 5**). |
| Rate limits | Device login shares the login bucket (`login_rate_limit`, per IP and per username with lockout, as `POST /api/auth/login`). Refresh gets its own per-IP bucket (`device_refresh_rate_limit`, 30/min). Both are in `credential_routes` (16 KiB bodies). | Same hardening as the web login. |

### Sync semantics

| Topic | Decision | Why |
|---|---|---|
| Identity of a phone asset | `device_asset_id` = Android MediaStore `_ID` as a string (`"images:1234"` / `"video:5678"`, since images and videos have separate id spaces), iOS `PHAsset.localIdentifier`. 1–256 bytes, no control characters. | Stable for the life of the asset on that phone. |
| `device_assets` | `(device_id, device_asset_id)` PK, `sha256`, `node_id` (nullable, `ON DELETE SET NULL`), `synced_at`. Written when an upload from that device completes or is deduplicated, and when `sync/check` finds the content already in the caller's library. | The design's table, plus `node_id` for support and later "where did this come from". |
| `POST /api/sync/check` | Bearer only. `{items: [{device_asset_id, sha256}]}`, 1–500 items, ids unique within the batch → `{missing: [device_asset_id]}`. An item is **present** if this device already recorded that id with that hash, **or** the caller owns any node (live or in the trash) with that hash; present-by-library items are recorded in `device_assets` in the same transaction. Everything else is missing. Own route group: 256 KiB body (500 × ≤ 350 bytes), 30 s timeout, per-device rate limit (2/s, burst 20). | Owner-scoped exactly like the upload probe: it never looks at other users' nodes, so it cannot reveal their content; answering by hash makes a reinstall (new device id) cheap. |
| Edited photos | Same id, new hash → missing → uploaded as a new file next to the old one (the server names it `IMG_1234 (1).jpg`). The old version stays. | Backup-only; no destructive replace. |
| Deleted on the server | Once recorded, an asset is never re-uploaded by that device, even if the user later trashes or deletes it on the server. | Respects deliberate deletions; a fresh install (new device) re-checks by hash and only re-uploads content that is truly gone (**Open question 6**). |
| Where uploads go | `Photos/<device name>/<year>/`, created on demand by `POST /api/sync/folder {year}` (bearer only, idempotent; the device's root folder id is kept on the `devices` row and recreated if the user deleted it). The year is the asset's local capture year. | Keeps the file browser usable for 30 000 photos; the timeline ignores folders anyway (**Open question 7**). |
| Capture time from the phone | `POST /api/uploads` gains optional `taken_at` (RFC 3339 **with offset**, e.g. `2024-07-14T18:30:05+02:00`). Stored on the new node (`nodes.client_taken_at` wall clock + `client_taken_offset_min`) both when the upload completes and when it is deduplicated into a new node. `resolve_taken` gains a step: EXIF → file name → **device** → upload time, recorded as a new `taken_source = 'device'` (the CHECK is widened). Values more than a day in the future, or before 1900, are ignored like EXIF ones. | The Phase 3 seam. A distinct source keeps "the phone said so" apart from "we guessed the upload time" for a later "date may be wrong" hint. Android's offset is the device time zone's offset at that instant (`TimeZone.getOffset(dateTaken)`), iOS the same from `PHAsset.creationDate`: right unless the user travelled, and EXIF wins anyway when present. |
| Videos | Backed up like photos (toggle "Include videos", on by default). The server stores them as files; `MediaKind::for_mime` has no video kind yet, so no `process_media` is queued. **Phase 4's backfill** creates their assets, posters and playback later, and they get `client_taken_at` as step 3 then. | The user decided Phase 6 first; nothing in the app changes when Phase 4 lands. |
| `device_asset_id` on uploads | Optional on `POST /api/uploads`; accepted only from a bearer caller (a session caller sending it gets `validation_error`). Recorded at completion or dedup. | A browser has no device. |

### Phone side

| Topic | Decision | Why |
|---|---|---|
| UI toolkit (decided 2026-11-16) | **Compose Multiplatform for every screen on both platforms**: module `ui` (screens, view models, navigation) on top of module `theme` (tokens, theme, components derived from MASTER.md). Same code, same look on Android and iOS. On iOS, `ComposeUIViewController` is hosted by a Swift entry point; no SwiftUI screens. | The user's direction: one UI and one theme, MASTER.md implemented once. Trade-offs (iOS feel, accessibility, size, stability) are listed under "Risks of Compose Multiplatform on iOS". |
| iOS through Kotlin/Native | PhotoKit (`platform.Photos`), `BGTaskScheduler` (`platform.BackgroundTasks`), the background `URLSession` and its delegate (`platform.Foundation`), Keychain (`platform.Security`), CommonCrypto, sharing and opening files (`platform.UIKit`) are written in Kotlin in `shared/src/iosMain` and `ui/src/iosMain`. **Swift only where Kotlin cannot reach cleanly:** (1) `SquirrelApp.swift`, the `@main` entry point hosting `ComposeUIViewController` (an iOS app's entry point and its `UIApplicationDelegate` adaptor are declared in Swift; a Kotlin `main` calling `UIApplicationMain` is possible but unsupported by the Xcode templates and CMP docs); (2) `AppDelegate.swift`, ~30 lines forwarding `application(_:didFinishLaunchingWithOptions:)` (BGTask handlers must be **registered before launch finishes**) and `application(_:handleEventsForBackgroundURLSession:completionHandler:)` (only delivered to the app delegate) to Kotlin; (3) XCTest code, because PhotoKit needs a host app with an `Info.plist` and photo permission, which the Kotlin/Native test runner does not have. | The user's direction: Swift kept to the minimum. Kotlin/Native exposes these frameworks directly; delegates are Kotlin classes extending `NSObject` and implementing the `…Protocol` interfaces. |
| API client (answered 2026-11-16) | **Generated** with openapi-generator 7.27.0 (`kotlin` generator, `library=multiplatform`: Ktor client + kotlinx.serialization) from the committed `openapi.json` into `shared/build/generated/openapi/` at build time, not committed. Hand-written on top: the auth/refresh layer (Ktor `Auth` bearer provider), the streaming chunk PUT (also used with the `Upload` token scheme and on iOS through `NSURLSession`), and a facade for `POST /api/uploads`'s two success bodies (Task 16). Drift check: `checkApiClient` in CI (Task 17). | The design's "KMP clients generated from it"; the user chose it (Open question 10). |
| Local database | SQLDelight 2.4.1, one database `squirrel.db`: `local_asset` (the queue), `scan_state`, `settings` (backup options; avoids DataStore), `secret` (the Android-encrypted refresh token). | One storage mechanism, transactional queue updates, in-memory driver in tests. |
| Hashing | `expect class Sha256` with `update(bytes, offset, length)` / `digest()`: Android/JVM `java.security.MessageDigest`, iOS CommonCrypto (`CC_SHA256_Init/Update/Final`) via `platform.CommonCrypto`. Files are streamed in 1 MiB buffers; the hash is cached per `(device_asset_id, size, modified_at)` so a file is hashed once. | No dependency, hardware-accelerated on both; constant memory for 4 GB videos. |
| Reading bytes | `AssetSource` interface (shared): `scan(since)`, `open(asset): AssetReader` with `read(offset, buffer)` and `size`. Android: `ContentResolver.openFileDescriptor` + `FileChannel` positional reads on `MediaStore.setRequireOriginal(uri)` (needs `ACCESS_MEDIA_LOCATION`, or GPS is redacted from the bytes and the hash differs from the original). iOS (Kotlin, `iosMain`): exports the asset's current resource with `PHAssetResourceManager.writeDataForAssetResource(…, toFile:)` into the app's caches directory, then reads that file with `NSFileHandle`. | Chunks need random access to resume; iOS gives only a stream, and background uploads need files anyway. |
| Upload | 8 MiB chunks (the server's `chunk_size` is authoritative), one asset at a time, up to 2 chunks in flight; resume asks `GET /api/uploads/{id}` for received chunks. Android sends chunks from the `WorkManager` worker with Ktor/OkHttp. iOS sends chunks as **background `URLSession` upload tasks from chunk files**, authorised by a per-upload **upload token** (Milestone H), because the system may run them long after a 15-minute access token expired. | Resumable across process death on both platforms (**Open question 11**). |
| Android scheduling | `PeriodicWorkRequest` every 6 h plus a **content-URI-triggered** `OneTimeWorkRequest` (`Constraints.Builder().addContentUriTrigger(MediaStore.Images/Video EXTERNAL_CONTENT_URI, true)`, re-armed after each run) that wakes the app for new photos even when it is not running, and a `ContentObserver` while the app is in the foreground. Constraints from settings: unmetered network when "Wi-Fi only" (default on), charging when "Only while charging" (default off), battery not low (always). A long first backup runs as a foreground worker (`setForeground`, type `dataSync`, progress notification). | The design's WorkManager + ContentObserver, using the trigger that survives process death. |
| iOS scheduling | `BGProcessingTask` (`requiresNetworkConnectivity = true`, `requiresExternalPower` from "Only while charging") and `BGAppRefreshTask` for the check; `PHPhotoLibrary.fetchPersistentChanges(since:)` finds what changed since the last token; a `PHPhotoLibraryChangeObserver` while in the foreground. "Back up now" uses `BGContinuedProcessingTask` on iOS 26+. Wi-Fi only maps to `allowsExpensiveNetworkAccess = false` and `allowsConstrainedNetworkAccess = false` on the background session. | iOS decides when background work runs; the background `URLSession` carries uploads between those windows. |
| Permissions | Android 13+: `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, and on 14+ `READ_MEDIA_VISUAL_USER_SELECTED` (declared, so partial access is a supported state); `ACCESS_MEDIA_LOCATION`; `POST_NOTIFICATIONS` (progress); Android ≤ 12: `READ_EXTERNAL_STORAGE`. **Partial access** (Android 14+ "Select photos", iOS "Limited Access") is shown honestly: "Squirrel can see 42 selected photos. Only these are backed up." with "Allow all photos" (opens the system dialog / settings) and "Change selection". iOS: `NSPhotoLibraryUsageDescription`, `PHPhotoLibraryPreventAutomaticLimitedAccessAlert = YES`, `PHPhotoLibrary.shared().presentLimitedLibraryPicker(from:)`. | Partial access silently backing up a subset would betray "backup"; MASTER rule 6 (real data only). |
| Server URL and TLS | The user types the server URL; the app checks `GET /api/healthz` and `GET /api/setup/status` before showing sign-in. **HTTPS only** in release builds, system CAs only (Tailscale and Caddy DNS-01 certificates are publicly trusted); debug builds also allow `http://10.0.2.2` and `http://localhost`. | Design: "HTTPS required". |
| Gallery v1 | Timeline grouped by month (`GET /api/photos/months` + `GET /api/photos?month=`), justified-ish fixed-height rows using the server's `width`/`height`, blurhash placeholders, thumbnails via `GET /api/files/{id}/thumbnail` through Coil with the bearer header; tap → full-screen preview (`size=preview`), swipe between photos, share/open original (download via `content`). Albums list and album contents, read-only. **No** selection, deletion, album editing, scrubber or map in v1. | Enough to trust the backup; editing stays on the web (**Open question 12**). |
| File browser v1 | Folder listing (`GET /api/nodes`), breadcrumbs, thumbnails in rows, open a file with the system viewer (download to cache, then share intent / `UIDocumentInteractionController`), trash is not shown. Read-only. | Same reasoning. |
| Navigation | MASTER.md's phone shell: a floating graphite bottom rail with **Photos**, **Files**, **Backup**; the account menu (top row) holds the server, the device name, "Sign out". | — |
| Minimum OS | Android **minSdk 29** (Android 10: `setRequireOriginal`, scoped storage baseline), targetSdk/compileSdk **36**; iOS **17.0**. | **Open question 13.** |
| Dependency injection | Manual constructor wiring in one `AppGraph` per platform. | ~15 classes; Koin would add a dependency for no gain. |

### Native design tokens (MASTER.md)

| Topic | Decision | Why |
|---|---|---|
| Source | `web/scripts/native-tokens.mjs` (Node, no dependency) reads the custom properties of `tokens.css` for light and dark and writes `mobile/theme/src/commonMain/kotlin/app/squirrel/theme/Tokens.kt` (colours, spacing, radii, type sizes, durations, the easing). A CI step regenerates it and fails on a diff. | CLAUDE.md: native apps derive their theme from MASTER.md and never invent values; a generator makes that mechanical (**Open question 8**). |
| What does not translate | CSS gradients become `Brush.verticalGradient` from the two stops; `color-mix` tokens are computed by the generator; diffuse shadows map to `Modifier.dropShadow` with the token's blur/offset/alpha; `svh`/media queries are not tokens. The phone breakpoints are MASTER.md's (375–412 px design width). | — |
| Font | Geist (OFL), the same variable font the web self-hosts, bundled as `ttf` in Compose resources (the web ships `woff2`, which Android and iOS cannot load). | Same typography everywhere. |
| Motion | `tween(160/240/360, easing = CubicBezierEasing(.22f, 1f, .36f, 1f))`; arrival animations are skipped on phones exactly as on touch web (MASTER.md "Motion"); reduced motion (Android animator scale 0, iOS `accessibilityReduceMotion`) makes everything instant. | MASTER.md "Motion". |
| New phone-only pieces | Backup status card (graphite card for the featured figure "1 204 left"), permission notice (Alert, `info` tone), progress (`Progress` component). Values come from existing tokens; anything new (none expected) is added to MASTER.md first. | **Open question 9.** |

## Open questions for the user

**Answered 2026-11-16: every recommendation below was accepted except question 10:** the API
client is **generated** with openapi-generator (Task 16), not hand-written. The answers, in short:
1 JDK 25 daemon + JDK 21 toolchain · 2 opaque tokens · 3 90-day sliding refresh with rotation ·
4 phone tokens on data routes only · 5 invitations joined on the web · 6 no re-upload after a
server-side delete · 7 `Photos/<device>/<year>/` · 8 tokens generated from `tokens.css` · 9 MASTER.md
phone additions accepted (Task 15 writes them) · **10 generated client** · 11 per-upload tokens for
iOS · 12 read-only gallery and files · 13 minSdk 29 / target 36, iOS 17 · 14 the dependency list
(with openapi-generator 7.27.0) · 15 `app.squirrel.android` / `app.squirrel.ios` · 16 sideloaded
builds · 17 Kotlin 2.4.21 now, Kotlin 2.5 or an Xcode fallback at Milestone H.

| # | Question | Recommendation |
|---|---|---|
| 1 | **JDK:** the Gradle daemon on the default JDK 25, compiling with a JDK 21 toolchain (Temurin 21.0.9 is already installed, nothing to download). If AGP or Kotlin misbehave on 25, pin the daemon to 21 in `~/.gradle/gradle.properties`. CI uses Temurin 21 via `actions/setup-java`. | **Accept** (no installation needed). |
| 2 | **Token format:** opaque random tokens stored hashed (one DB lookup per request, instant revocation, no new crate) vs. signed JWT access tokens (no lookup, but revocation waits up to 15 min and adds a signing key plus a JWT crate). | **Opaque tokens.** |
| 3 | **Refresh lifetime:** 90 days idle, sliding, no absolute expiry; rotation with reuse detection. | **Accept** (a phone left alone for three months signs in again). |
| 4 | **Phone token scope:** data routes only (files, photos, albums, uploads, search); no invitations, admin or device management from the app. | **Accept.** |
| 5 | **Joining by invitation in the app:** v1 sends people to the web join page, then they sign in in the app. Alternative: `POST /api/join/{token}` gains a `device` field and returns tokens, and the app handles `/join/<token>` links. | **Web join in v1**; app-side join later if inviting phone-only people becomes common. |
| 6 | **Re-uploading what was deleted on the server:** never, once a device recorded the asset; a reinstall re-checks by hash. | **Accept.** |
| 7 | **Folder layout:** `Photos/<device name>/<year>/` vs. one flat `Photos/<device name>/` vs. `Photos/<year>/<month>/` shared by all devices. | **`Photos/<device name>/<year>/`.** |
| 8 | **Native tokens generated from `tokens.css`** by a dependency-free Node script with a CI drift check, vs. a hand-written `Theme.kt`. | **Generate.** |
| 9 | **Phone design additions to MASTER.md** (needs re-validation): the backup status card is the screen's one graphite card ("1 204 left · 3.2 GB"); the permission notice is an `info` Alert with two buttons; the phone gallery uses the web's photo tokens (`--photo-row` 112 px, `--radius-tile` 6 px, gaps `--space-1`). No new values. | **Accept** (Task 15 writes the paragraph into MASTER.md before any screen). |
| 10 | **API client:** hand-written Ktor client + `openapi.json` contract test vs. `openapi-generator` (`kotlin`, `library=multiplatform`) output. | Recommended hand-written; **answered: generated** (Task 16). |
| 11 | **iOS upload tokens:** `POST /api/uploads` from a bearer caller also returns an `upload_token` (scoped to that one upload, chunk PUTs and status only, expires with the session) so background `URLSession` chunk uploads keep working after the access token expires. Alternative: no token; chunks failing with 401 are re-queued when the app next wakes (slower, more wakeups). | **Upload tokens** (Milestone H, server task first). |
| 12 | **Gallery scope in v1:** timeline by month, preview, swipe, open original, read-only albums, read-only file browser. No selection, deletion, album editing, scrubber, search or map. | **Accept.** |
| 13 | **Minimum OS:** Android 10 (minSdk 29), target/compile 36; iOS 17. Android 8–9 would need a different original-bytes path; iOS 16 lacks parts of the Observation stack the shell uses. | **Accept.** |
| 14 | **Dependencies** (all Apache-2.0 except where noted): Kotlin 2.4.21, **AGP 9.3.3 and Gradle 9.7.1** (the newest patches of the lines Kotlin 2.4.21 is tested with; the latest AGP 9.5.1 and Gradle 9.9.1 are untested with it and gain this app nothing, so move up together with Kotlin 2.5), Compose Multiplatform 1.12.2, JetBrains navigation-compose 2.10.0 and lifecycle-viewmodel-compose 2.11.0, Ktor 3.6.1, kotlinx.serialization 1.13.0, kotlinx.coroutines 1.11.0, kotlinx-datetime 0.8.0, SQLDelight 2.4.1, WorkManager 2.12.0, activity-compose 1.13.0, core-ktx 1.19.1, Coil 3.6.3; test: Turbine 1.2.1, Robolectric 4.18 (MIT), AndroidX Test 1.7.0 / ext-junit 1.3.0. openapi-generator Gradle plugin 7.27.0 (Apache-2.0; build-time only, nothing of it ships). Geist font (OFL-1.1). Not used: Koin 4.2.2, DataStore 1.3.0 (now KMP), KotlinCrypto sha2 0.8.0 / Okio 3.18.2 `HashingSink` (common SHA-256). | **Accept.** |
| 15 | **App identity:** application id / bundle id `app.squirrel.android` / `app.squirrel.ios`, display name "Squirrel". Signing for real devices needs your Apple team and an Android keystore (not in the repository). | **Accept the ids**; you provide signing when installing on your own phones. |
| 16 | **Distribution:** sideloaded debug/release builds (Android APK, iOS via Xcode to your own devices) in Phase 6; no store listing. | **Accept**; store publishing is out of scope for v1. |
| 17 | **Kotlin vs. Xcode:** Kotlin 2.4.20–2.4.21's compatibility guide lists **Xcode 26.4** only; this machine has Xcode 27.0 (latest is 27.2); Kotlin 2.5.0, announced as the first with the 27 SDK, is a release candidate today. Options at Milestone H: (a) Kotlin 2.5.0 if stable by then (with the Gradle/AGP lines it is tested with); (b) Kotlin 2.4.21 with Xcode 27, unsupported (a community report says it works); (c) install Xcode 26.4 beside 27 (needs your approval, ~10 GB). | **Start on 2.4.21** (Milestones A–G are server and Android only); at Milestone H take **(a)**, else try **(b)** and fall back to **(c)** with your approval. Record the outcome. |

## API added in Phase 6

| Method & path | Auth | Group | Purpose |
|---|---|---|---|
| `POST /api/auth/device/login` | none | credential | `{username, password, device: {name, platform, model?, app_version}}` → 201 `{device, account, tokens}` |
| `POST /api/auth/device/refresh` | none | credential | `{refresh_token}` → `{tokens}` (rotated); 401 `unauthorized` |
| `POST /api/auth/device/logout` | bearer | json | Revokes the calling device → 204 |
| `GET /api/devices/current` · `PATCH /api/devices/current` | bearer | json | `{device, account, backup_folder_id}` · rename `{name}` |
| `GET /api/devices` | session | json | The caller's devices (`DeviceDto[]`, revoked ones last) |
| `DELETE /api/devices/{id}` | session + CSRF | json | Revoke one device → 204 (idempotent; another user's id → 404) |
| `POST /api/sync/check` | bearer | sync (256 KiB) | `{items: [{device_asset_id, sha256}]}` (≤ 500) → `{missing: [..]}` |
| `POST /api/sync/folder` | bearer | json | `{year}` → `{folder_id}` (`Photos/<device>/<year>`, created if needed) |

Security schemes in `openapi.json` (so the generated phone client authenticates correctly):
`session_cookie` + `csrf_token`, `device_bearer` (HTTP bearer), and from Milestone H
`upload_token` (HTTP scheme `Upload`); every operation lists the ones it accepts.

Changed: `POST /api/uploads` accepts optional `taken_at` and `device_asset_id`, and (Milestone H) a
bearer caller's 201 response carries `upload_token`; `PUT /api/uploads/{id}/chunks/{n}` and
`GET /api/uploads/{id}` also accept `Authorization: Upload <token>`. Every data route accepts
`Authorization: Bearer sqa_…` besides the session cookie. `POST /api/auth/logout-all` also revokes
devices. `assets.taken_source` gains `device`. No new error code (`unauthorized`, `forbidden`,
`validation_error`, `rate_limited` cover everything).

`DeviceDto`: `{id, name, platform: "android"|"ios", model, app_version, created_at, last_seen_at,
revoked_at, current}` (`current` true for the calling device). `TokenPair`: `{access_token,
access_expires_at, refresh_token, refresh_expires_at}`. `AccountDto`: `{id, username, display_name,
role}`.

## Milestones

- **A — Server: devices and bearer tokens** (Tasks 1–6): ships token auth usable with `curl`.
- **B — Server: sync check, device capture time, sync folders** (Tasks 7–11): ships the whole sync
  protocol, testable with `curl` and integration tests.
- **C — Web: Settings › Devices** (Tasks 12–13): ships device listing and revocation.
- **D — Mobile foundation** (Tasks 14–20): the Gradle build, generated tokens and theme, shared API
  client, token store, database. Ships an Android app that signs in and shows the account.
- **E — Shared sync engine** (Tasks 21–25): hashing, queue, uploader, engine, all unit-tested on
  the JVM.
- **F — Android backup end to end** (Tasks 26–32): permissions, MediaStore, WorkManager, backup
  screens. **Ships: an Android phone backs up its photos.**
- **G — Android gallery and files** (Tasks 33–36): ships browsing on Android.
- **H — iOS** (Tasks 37–45): server upload tokens, then the Swift entry point hosting the shared
  Compose UI, and PhotoKit, background tasks and uploads in Kotlin/Native. Ships the iOS app with
  backup, gallery and files.
- **I — CI, docs and device checklist** (Tasks 46–48).

Each milestone ends green (`cargo test --workspace`, the web checks from C on, the mobile checks
from D on) and can be reviewed and merged on its own.

---

## Milestone A — Server: devices and bearer tokens

### Task 1: Migration `20261116000001_devices.sql` and the devices repository

**Files:**
- Create: `crates/squirrel-db/migrations/20261116000001_devices.sql`,
  `crates/squirrel-db/src/devices.rs`, `crates/squirrel-db/tests/devices.rs`
- Modify: `crates/squirrel-db/src/lib.rs`

**Step 1: Write the migration**

```sql
-- Phones signed in with bearer tokens (Phase 6). Tokens are stored as SHA-256 only.
CREATE TABLE devices (
    id                     UUID PRIMARY KEY,
    owner_id               UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name                   TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    platform               TEXT NOT NULL CHECK (platform IN ('android', 'ios')),
    model                  TEXT CHECK (model IS NULL OR char_length(model) <= 64),
    app_version            TEXT NOT NULL CHECK (char_length(app_version) BETWEEN 1 AND 32),
    refresh_hash           BYTEA NOT NULL UNIQUE,
    -- Kept until the current refresh token is first used (a lost response can be retried).
    previous_refresh_hash  BYTEA UNIQUE,
    refresh_expires_at     TIMESTAMPTZ NOT NULL,
    -- Photos/<name>, created on first use by POST /api/sync/folder.
    backup_folder_id       UUID,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at             TIMESTAMPTZ,
    CONSTRAINT devices_id_owner_key UNIQUE (id, owner_id),
    CONSTRAINT devices_backup_folder_fkey FOREIGN KEY (backup_folder_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE SET NULL (backup_folder_id)
);
CREATE INDEX devices_owner_idx ON devices (owner_id, revoked_at NULLS FIRST, last_seen_at DESC);

CREATE TABLE device_access_tokens (
    token_hash  BYTEA PRIMARY KEY,
    device_id   UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX device_access_tokens_device_idx ON device_access_tokens (device_id);
CREATE INDEX device_access_tokens_expires_idx ON device_access_tokens (expires_at);

-- Refresh tokens that were rotated out twice. Presenting one is reuse: the device is revoked.
CREATE TABLE device_retired_refresh (
    token_hash  BYTEA PRIMARY KEY,
    device_id   UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE
);
```

**Step 2: Write the failing tests** (`tests/devices.rs`, `#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]`,
users created with the existing test helper):
1. `devices::create` stores the row and its first access token; `find_by_access_hash` returns
   `(DeviceRecord, owner)` for a live token and `None` once `expires_at` passed, once the device is
   revoked, or for an unknown hash.
2. `rotate_refresh(presented_hash, new_hash, new_access_hash, ttl)` with the **current** hash:
   the previous hash (if any) moves to `device_retired_refresh`, the current one becomes
   `previous_refresh_hash`, the new one becomes current, `refresh_expires_at` is extended, the
   access token is inserted → `Rotation::Rotated`.
3. Same call with the **previous** hash (the client never received the last response) →
   `Rotated` too: the current hash (never used) is retired, the previous one stays previous.
4. A **retired** hash → `Rotation::Reused`; the device is revoked and all its access tokens
   deleted. An unknown hash → `Rotation::Unknown`.
5. Expired refresh (`refresh_expires_at < now`) → `Rotation::Expired`, device untouched.
6. `list_for_owner` returns the owner's devices, live first then revoked, never another user's;
   `revoke(owner, id)` is idempotent and returns `false` for another user's id; `revoke_all_for_user`
   counts the live ones it revoked and deletes their access tokens.
7. `delete_expired_tokens(now)` removes expired access tokens only.
8. Deleting the user cascades; deleting the backup folder sets `backup_folder_id` to NULL.

So a legitimate phone can always retry its last refresh, and a thief replaying a token the phone
has since rotated past is detected.

**Step 3: Run** `cargo test -p squirrel-db --test devices` → FAIL (module missing).

**Step 4: Implement** `devices.rs` with runtime queries (`query_as::<_, DeviceRecord>`) as the
other repositories do. `rotate_refresh` runs in one transaction with `SELECT … FOR UPDATE` on the
matching row (`WHERE refresh_hash = $1 OR previous_refresh_hash = $1`; when nothing matches,
a lookup in `device_retired_refresh`), so two concurrent refreshes with the same token cannot both
rotate.

```rust
pub enum Rotation {
    Rotated(DeviceRecord),
    Reused { device_id: Uuid, owner_id: Uuid },
    Expired,
    Unknown,
}
```

**Step 5: Run** → PASS. **Step 6: Commit** — `feat(db): store devices and their hashed tokens`.

---

### Task 2: Token generation and the `devices` auth helpers

**Files:**
- Modify: `crates/squirrel-auth/src/token.rs`, `crates/squirrel-auth/src/lib.rs`

**Step 1: Failing tests** (unit, in `token.rs`):
1. `DeviceToken::access()` starts with `sqa_`, `DeviceToken::refresh()` with `sqr_`, both 4 + 43
   characters (32 bytes base64url, no padding), two calls never equal.
2. `parse_access("sqa_…")` accepts only the `sqa_` prefix and exactly 43 base64url characters;
   `sqr_` tokens, session tokens, empty strings and over-long input are rejected **before** hashing.
3. `hash_token` of the full string (prefix included) is what is stored.

**Step 2: Run** `cargo test -p squirrel-auth` → FAIL. **Step 3: Implement** on top of
`generate_token()`. **Step 4: Run** → PASS.
**Step 5: Commit** — `feat(auth): prefixed device access and refresh tokens`.

---

### Task 3: `POST /api/auth/device/login` and `POST /api/auth/device/refresh`

**Files:**
- Create: `crates/squirrel-api/src/routes/device_auth.rs`, `crates/squirrel-api/src/devices_dto.rs`,
  `crates/squirrel-api/tests/device_auth.rs`
- Modify: `crates/squirrel-api/src/lib.rs` (credential routes, tag `devices`),
  `crates/squirrel-api/src/state.rs` (`device_access_ttl` 15 min, `device_refresh_ttl` 90 days,
  `device_refresh_rate_limit`), `crates/squirrel-api/src/routes/mod.rs`, `openapi.json`,
  `web/src/lib/api/schema.d.ts`

**Step 1: Failing tests** (`tests/device_auth.rs`, using the existing `common` helpers that start
the router against a `sqlx::test` pool):
1. Login with the right password → 201, `tokens.access_token` starts with `sqa_`, the device is in
   the database with the given name/platform/app version, **no `Set-Cookie`** header.
2. Wrong password, unknown user, disabled user, locked user → 401 `invalid_credentials`, exactly
   like `/api/auth/login` (same body, failures counted, lockout shared).
3. Validation: empty or 65-character device name, platform `"windows"`, missing app version → 422.
4. Refresh with refresh token R1 → 200 with R2; the old access token keeps working until it
   expires; refreshing with R1 again → 200 with R3 (lost response); refreshing with R3 → R4;
   now R1 is retired, and presenting it → 401 and the device is revoked (every token of it fails).
5. Refresh with an access token, a session token, garbage → 401 `unauthorized` without a database
   write (prefix check first).
6. The login bucket: the 11th device login per minute from one IP → 429 (use the test config's
   limit, as `tests/rate_limit.rs` does); refresh has its own bucket.
7. A revoked or expired device cannot refresh (401).

**Step 2: Run** → FAIL. **Step 3: Implement.** `device_login` reuses the body of `auth::login` up to
the password check (extract a shared `authenticate(state, client_ip, username, password) ->
Result<UserRecord, ApiError>` from `routes/auth.rs`, so lockout, timing equalisation and logging
stay identical), then creates the device and its first tokens. Log `device login succeeded` with
`user_id`, `device_id`, platform and client IP, never tokens. `device_refresh` maps `Rotation`:
`Rotated` → 200, `Reused` → revoke + `tracing::warn!(… "refresh token reuse: device revoked")` +
401, `Expired`/`Unknown` → 401.
Regenerate `openapi.json` and `schema.d.ts`.

**Step 4: Run** → PASS. **Step 5: Commit** — `feat(api): sign phones in with device tokens`.

---

### Task 4: The `Caller` extractor (session or bearer)

**Files:**
- Modify: `crates/squirrel-api/src/extract.rs`
- Create: `crates/squirrel-api/tests/bearer.rs`

**Step 1: Failing tests:**
1. A data route (`GET /api/nodes`) with `Authorization: Bearer <access>` → 200 and lists that
   user's roots; with an expired, revoked, unknown or malformed bearer → 401 `unauthorized`.
2. A `POST /api/folders` with a bearer and **no** CSRF header → 201.
3. A request with both a valid session cookie and an **invalid** bearer → 401 (the cookie is
   ignored once `Authorization` is present). A non-`Bearer` scheme (e.g. `Basic`) → 401.
4. A disabled user's bearer → 401.
5. `devices.last_seen_at` moves forward at most once per `session_touch_interval` (reuse the
   setting).

**Step 2: Run** → FAIL. **Step 3: Implement:**

```rust
/// Who is calling: a browser session (CSRF-checked) or a signed-in device.
pub enum Principal {
    Session(SessionRecord),
    Device(DeviceRecord),
}

/// An authenticated, non-disabled user, by session cookie or by device bearer token.
/// Data routes take this; account and admin routes keep `CurrentUser` (session only).
pub struct Caller {
    pub user: UserRecord,
    pub principal: Principal,
}

impl Caller {
    pub fn device(&self) -> Option<&DeviceRecord> { /* … */ }
    pub fn require_device(&self) -> Result<&DeviceRecord, ApiError> {
        self.device().ok_or(ApiError::Forbidden)
    }
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        match parts.headers.get(AUTHORIZATION) {
            Some(value) => bearer(value, state).await,   // never falls back to the cookie
            None => {
                let CurrentUser { user, session } = CurrentUser::from_request_parts(parts, state).await?;
                Ok(Self { user, principal: Principal::Session(session) })
            }
        }
    }
}
```

`bearer` parses `Bearer sqa_…` with `parse_access`, hashes, calls
`devices::find_by_access_hash`, loads the user and filters disabled ones, then touches
`last_seen_at`. Also add `DeviceCaller` (bearer only: `Caller` + `require_device`) for the
`/api/devices/current`, `/api/sync/*` and device-logout routes.

**Step 4: Describe the schemes in `openapi.json`** (the phone's client is generated from it,
Task 16): a `utoipa::Modify` on `ApiDoc` registers `session_cookie` (`apiKey`, in cookie
`squirrel_session`), `csrf_token` (`apiKey`, header `X-CSRF-Token`) and `device_bearer`
(`http`, scheme `bearer`, description "Device access token, `sqa_…`"). This task only adds the
schemes; Task 5 attaches them to every route. Regenerate the contract.

**Step 5: Run** → PASS. **Step 6: Commit** — `feat(api): accept device bearer tokens beside sessions`.

---

### Task 5: Move the data routes to `Caller`, and pin the route matrix

**Files:**
- Modify: `crates/squirrel-api/src/routes/{nodes,content,thumbnails,trash,storage,photos,albums,search}.rs`,
  `crates/squirrel-api/src/routes/uploads/{mod,chunks,complete}.rs`
- Create: `crates/squirrel-api/tests/route_auth_matrix.rs`

**Step 1: Failing test** `route_auth_matrix.rs`: parse the committed `openapi.json`, and for
**every** path and method send a request with only a valid device bearer (path ids filled with a
random UUID, JSON bodies `{}`); assert the status is **not 401** for the data routes and **401**
for the session-only list (`/api/auth/me`, `/api/auth/logout`, `/api/auth/logout-all`,
`/api/invitations*`, `/api/admin/*`, `GET /api/devices`, `DELETE /api/devices/{id}`), and the
reverse with only a session cookie + CSRF for the bearer-only list (`/api/devices/current`,
`/api/auth/device/logout`, `/api/sync/*`: 403 `forbidden`). Unauthenticated routes (`healthz`,
`setup/*`, `join/*`, `auth/login`, `auth/device/*` login and refresh) are listed explicitly. A path
missing from all lists fails the test, so a future route must choose. A second test checks the
**document matches the behaviour**: every operation's `security` in `openapi.json` lists exactly
the schemes the matrix found accepted (`[]` for the unauthenticated ones; session routes
`session_cookie` + `csrf_token` for non-GET; data routes `device_bearer` or the session pair;
bearer-only routes `device_bearer`), so the generated phone client sends the right credentials.

**Step 2: Run** → FAIL. **Step 3: Implement:** replace `current: CurrentUser` by `caller: Caller`
in the data handlers (`current.user.id` → `caller.user.id`; nothing else uses the session there),
and add `security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = []))` (or the
session-only / bearer-only / empty variant) to every `#[utoipa::path]`. Regenerate the contract.
Keep upload rate limits and slots keyed by user id (a phone and a browser of the same user share
them, as two browsers do today).

**Step 4: Run** the whole API suite → PASS (existing tests prove sessions still work).
**Step 5: Commit** — `feat(api): let devices use the data routes`.

---

### Task 6: Device management: current device, list, revoke, logout, logout-all, maintenance

**Files:**
- Create: `crates/squirrel-api/src/routes/devices.rs`, `crates/squirrel-api/tests/devices.rs`
- Modify: `crates/squirrel-api/src/routes/auth.rs` (`logout_all`), `crates/squirrel-api/src/maintenance.rs`,
  `crates/squirrel-api/src/lib.rs`, `openapi.json`, `web/src/lib/api/schema.d.ts`

**Step 1: Failing tests:**
1. `GET /api/devices/current` (bearer) → `{device: {current: true, …}, account, backup_folder_id: null}`;
   `PATCH` renames (validation as at login).
2. `GET /api/devices` (session) lists alice's two devices, live before revoked, `current: false`;
   bob sees none of them.
3. `DELETE /api/devices/{id}` (session + CSRF) → 204; the device's access and refresh tokens fail
   at once; repeating → 204; bob deleting alice's device → 404 `not_found`; without CSRF → 403.
4. `POST /api/auth/device/logout` (bearer) → 204, then the token fails.
5. `POST /api/auth/logout-all` revokes alice's devices too (and only hers).
6. Maintenance: one pass deletes expired access tokens and **deletes** devices revoked or expired
   for more than 30 days (their `device_assets` go with them; Task 7's FK is `ON DELETE CASCADE`).

**Step 2: Run** → FAIL. **Step 3: Implement.** Log revocations with `user_id`, `device_id`, and who
revoked (`web` / `self` / `logout-all` / `reuse`). Regenerate the contract.
**Step 4: Run** → PASS. **Step 5: Commit** — `feat(api): list, rename and revoke devices`.

**Milestone A check:** `cargo test --workspace`; manually:
`curl -s -X POST localhost:8080/api/auth/device/login -H 'content-type: application/json' -d '{"username":"alice","password":"…","device":{"name":"Test","platform":"android","app_version":"0.1.0"}}'`,
then `curl -H "Authorization: Bearer sqa_…" localhost:8080/api/nodes`.

---

## Milestone B — Server: sync check, device capture time, sync folders

### Task 7: Migration `20261116000002_device_sync.sql` and repositories

**Files:**
- Create: `crates/squirrel-db/migrations/20261116000002_device_sync.sql`,
  `crates/squirrel-db/src/device_assets.rs`, `crates/squirrel-db/tests/device_assets.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `crates/squirrel-db/src/assets.rs` (`TakenSource::Device`),
  `crates/squirrel-db/src/nodes/{mod,insert}.rs` (the two new columns),
  `crates/squirrel-db/src/uploads.rs` (the session columns)

```sql
-- Which phone asset became which content (Phase 6). Backup-only: rows are never
-- removed when the phone deletes the asset.
CREATE TABLE device_assets (
    device_id        UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    device_asset_id  TEXT NOT NULL CHECK (octet_length(device_asset_id) BETWEEN 1 AND 256),
    sha256           TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    node_id          UUID REFERENCES nodes (id) ON DELETE SET NULL,
    synced_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, device_asset_id)
);

-- The capture time a phone reported, step 3 of the taken_at chain.
ALTER TABLE nodes
    ADD COLUMN client_taken_at TIMESTAMP,
    ADD COLUMN client_taken_offset_min SMALLINT
        CHECK (client_taken_offset_min BETWEEN -1080 AND 1080),
    ADD CONSTRAINT nodes_client_taken_shape
        CHECK (client_taken_offset_min IS NULL OR client_taken_at IS NOT NULL);

-- Carried by the upload session until completion creates the node.
ALTER TABLE upload_sessions
    ADD COLUMN client_taken_at TIMESTAMP,
    ADD COLUMN client_taken_offset_min SMALLINT
        CHECK (client_taken_offset_min BETWEEN -1080 AND 1080),
    ADD COLUMN device_id UUID REFERENCES devices (id) ON DELETE SET NULL,
    ADD COLUMN device_asset_id TEXT
        CHECK (device_asset_id IS NULL OR octet_length(device_asset_id) BETWEEN 1 AND 256),
    ADD CONSTRAINT upload_sessions_device_asset_shape
        CHECK ((device_asset_id IS NULL) = (device_id IS NULL));

ALTER TABLE assets DROP CONSTRAINT assets_taken_source_check;   -- verify the name first (\d assets)
ALTER TABLE assets ADD CONSTRAINT assets_taken_source_check
    CHECK (taken_source IN ('exif', 'filename', 'device', 'upload'));
```

**Failing tests** (`tests/device_assets.rs`):
1. `device_assets::missing(conn, device_id, owner_id, &[(id, sha)])` returns only the ids that are
   neither recorded for this device with this hash nor owned by the caller as a node (live **or
   trashed**); content owned **only by another user** is reported missing (no leak).
2. The same call records (`INSERT … ON CONFLICT (device_id, device_asset_id) DO UPDATE SET sha256,
   node_id, synced_at`) the ids found through the library, with the matching node id (any one).
3. A recorded id whose hash changed (edited photo) is missing again.
4. `record(conn, device_id, id, sha, node_id)` upserts.
5. `EXPLAIN` of the `missing` query on 500 items uses `nodes_owner_blob_idx` and the
   `device_assets` primary key (`Index Cond`), as Phase 2/3 keyset tests do.
6. The widened CHECK accepts `'device'`; `nodes_client_taken_shape` rejects an offset without a time.

**Implement** `missing` with one statement over `unnest($3::text[], $4::text[])`:

```sql
WITH input AS (
    SELECT * FROM unnest($3::text[], $4::text[]) AS i (device_asset_id, sha256)
), known AS (
    SELECT i.device_asset_id, i.sha256,
           EXISTS (SELECT 1 FROM device_assets d
                    WHERE d.device_id = $1 AND d.device_asset_id = i.device_asset_id
                      AND d.sha256 = i.sha256) AS recorded,
           (SELECT n.id FROM nodes n
             WHERE n.owner_id = $2 AND n.blob_sha256 = i.sha256 LIMIT 1) AS library_node
      FROM input i
)
SELECT device_asset_id, sha256, recorded, library_node FROM known
```

then, in the same transaction, upsert the `library_node IS NOT NULL AND NOT recorded` rows and
return the ids with neither. **Commit** — `feat(db): track device assets and phone capture times`.

---

### Task 8: `POST /api/sync/check`

**Files:**
- Create: `crates/squirrel-api/src/routes/sync.rs`, `crates/squirrel-api/src/sync_dto.rs`,
  `crates/squirrel-api/tests/sync.rs`
- Modify: `crates/squirrel-api/src/lib.rs` (new `sync_routes`: 256 KiB body, inside the 30 s
  timeout, per-device token bucket `UserRateLimiter` keyed by device id: 2/s, burst 20),
  `openapi.json`, `web/src/lib/api/schema.d.ts`

**Failing tests:**
1. Alice's phone checks 3 items: one she uploaded on the web, one only Bob has, one nobody has →
   `missing` = the last two; a second identical check returns the same two (the first is now
   recorded).
2. Session caller → 403 `forbidden`; no auth → 401.
3. 0 items, 501 items, a duplicate id, a 257-byte id, a control character in an id, an uppercase
   or 63-character hash → 422 `validation_error` (whole batch refused, nothing recorded).
4. Rate limit: the 21st immediate request → 429 `rate_limited`.
5. Timing note in the test file: the response time does not depend on whether **another** user has
   the content (the query never touches other users' nodes; asserted structurally by test 1, not by
   timing).

**Implement**, regenerate the contract, run, **commit** — `feat(api): let phones ask which photos are missing`.

---

### Task 9: `taken_at` and `device_asset_id` on uploads

**Files:**
- Modify: `crates/squirrel-api/src/files_dto.rs` (`CreateUploadRequest`),
  `crates/squirrel-api/src/routes/uploads/{create,complete,mod}.rs`, `crates/squirrel-db/src/nodes/insert.rs`
  (`NewFile` gains `client_taken: Option<ClientTaken>`), `crates/squirrel-db/src/uploads.rs`
- Create: `crates/squirrel-api/tests/upload_device.rs`

**Failing tests:**
1. A bearer upload with `taken_at: "2024-07-14T18:30:05+02:00"` and `device_asset_id: "images:12"`
   completes; the node has `client_taken_at = 2024-07-14 18:30:05`, offset 120; `device_assets`
   holds `(device, "images:12", sha, node)`.
2. The same content uploaded again by the same phone under another id is **deduplicated** (200):
   the new node also carries the phone's time, and the second id is recorded.
3. A session caller sending `device_asset_id` → 422; sending `taken_at` → accepted (harmless, and
   the web may use it one day).
4. `taken_at` without an offset (`2024-07-14T18:30:05`), with a ±19 h offset, or not a date → 422.
5. Resuming an open session returns it unchanged even if the second request's `taken_at` differs
   (the first one wins; the session is matched by owner, folder, name, hash and size as today).

**Implement:** parse with `time::OffsetDateTime::parse(…, &Rfc3339)`; store the wall clock
(`PrimitiveDateTime::new(dt.date(), dt.time())`) and `dt.offset().whole_minutes()`. `link_existing`
and completion both pass `client_taken` to `insert_file` and call `device_assets::record` in the
same transaction when the session (or request) carries a device. **Commit** —
`feat(api): carry the phone's capture time and asset id through uploads`.

---

### Task 10: Step 3 of the capture-time chain

**Files:**
- Modify: `crates/squirrel-media/src/capture.rs`, `crates/squirrel-worker/src/facts.rs`,
  `crates/squirrel-db/src/assets.rs` (`BlobNode` loads `client_taken_at`/`client_taken_offset_min`)
- Test: `crates/squirrel-media/src/capture.rs` (unit), `crates/squirrel-worker/tests/process.rs`

**Failing tests:**
1. `resolve_taken(None, "photo.jpg", Some(device), uploaded, now)` → `device.local`, its offset,
   `TakenSource::Device`.
2. EXIF wins over device; a date in the file name wins over device (it is the camera's own record);
   a device time more than a day in the future, or before 1900, falls through to the upload time.
3. Worker: a node with `client_taken_at` and an image without EXIF gets `taken_source = 'device'`.

**Implement:**

```rust
pub enum TakenSource { Exif, Filename, Device, Upload }   // as_str: "device"

/// What the uploading client said, if anything (the phone's capture time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientTaken {
    pub local: PrimitiveDateTime,
    pub offset: Option<UtcOffset>,
}

pub fn resolve_taken(
    exif: Option<CaptureTime>,
    file_name: &str,
    client: Option<ClientTaken>,
    uploaded_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Taken
```

The year-1900 floor reuses the existing "clock not set" rule. Update every caller (`facts.rs`
passes the node's values). **Commit** — `feat(media): use the phone's capture time when EXIF and the name have none`.

---

### Task 11: `POST /api/sync/folder`

**Files:**
- Modify: `crates/squirrel-api/src/routes/sync.rs`, `crates/squirrel-db/src/devices.rs`
  (`set_backup_folder`), `crates/squirrel-db/src/nodes/` (reuse `find_live_folder`, `insert_folder`)
- Test: `crates/squirrel-api/tests/sync.rs`

**Failing tests:**
1. First call `{year: 2024}` creates `Photos/<device name>` and `Photos/<device name>/2024`, stores
   the device folder on the device, returns the year folder's id; the second call returns the same
   id and creates nothing.
2. If a folder named like the device already exists (the user made it), it is reused, not suffixed.
3. After the user trashes the device folder, the next call creates a fresh one (the trashed folder
   is not resurrected) and updates `backup_folder_id`.
4. Two concurrent calls create exactly one folder (the unique live-name index; the loser re-reads).
5. `year` outside 1900–9999 → 422; session caller → 403.

**Implement** (one transaction; on a `name_conflict` unique violation, re-read the existing folder).
Regenerate the contract. **Commit** — `feat(api): give each phone its own backup folders`.

**Milestone B check:** `cargo test --workspace`; with `curl`, run check → create upload → chunks →
complete → check again (now not missing) on the dev server.

---

## Milestone C — Web: Settings › Devices

### Task 12: Devices API client and model

**Files:**
- Create: `web/src/lib/devices.ts`, `web/src/lib/devices.test.ts`

**Failing tests** (Vitest): `deviceLabel(device)` (`"Pixel 8 · Android · 0.1.0"`),
`lastSeen(device, now)` (`"Active now"` under 5 min, `"2 h ago"`, `"14 Nov"`; tabular figures
are a CSS concern), `sortDevices` (live first by `last_seen_at` desc, then revoked).
**Implement**, **run** `pnpm test:unit`, **commit** — `feat(web): device list helpers`.

### Task 13: `/settings/devices` page

**Files:**
- Create: `web/src/routes/settings/devices/+page.svelte`, `web/e2e/devices.spec.ts`
- Modify: the account menu component (add "Devices" next to the theme picker; not a rail item:
  the phone rail keeps its five items), `web/src/lib/guard.ts` if routes are listed there

**Screen (MASTER.md):** page title "Devices" (`h1`), muted description "Phones signed in to your
Squirrel. Revoking one signs it out at once; what it backed up stays." Below, a `--tint` **well**
of **row cards** (`ListRow`): a 36 px round icon well with the `smartphone` icon (add the Lucide
shape to `icons/paths.ts`), name 500, muted tabular metadata `deviceLabel · lastSeen`; one
**danger** button "Revoke" (`Revoke Pixel 8` accessible name) that opens `Dialog` with a
**danger-solid** "Revoke" confirmation. Revoked devices show a `Badge` "Revoked" and no action.
Empty state (`EmptyState`, `smartphone` icon): "No phones yet" / "Install the Squirrel app and
sign in to back up your photos." Errors: `Alert` error tone. Phones: the button moves into the
row's actions sheet as other rows do.

**E2E:** create a device through the API (`request.post('/api/auth/device/login', …)`), open
Settings › Devices, see it, revoke it, confirm the bearer now gets 401.
**Run** `pnpm check && pnpm test:unit && SQUIRREL_E2E_PORT=8199 pnpm test:e2e`.
**Commit** — `feat(web): list and revoke devices in settings`.

---

## Milestone D — Mobile foundation

### Project layout (created across Tasks 14–20)

```
mobile/
  settings.gradle.kts          include(":shared", ":theme", ":ui", ":androidApp")
  build.gradle.kts             plugins declared `apply false`
  gradle.properties            org.gradle.jvmargs, kotlin.code.style=official, android.useAndroidX=true,
                               org.gradle.java.installations.auto-download=false
  gradle/libs.versions.toml    every version in this plan, nowhere else
  gradle/wrapper/              gradle-wrapper.jar + .properties (9.7.1, distributionSha256Sum pinned)
  gradlew, gradlew.bat
  shared/                      KMP library, no UI: targets jvm() (tests), android, iosArm64, iosSimulatorArm64
    src/commonMain/kotlin/app/squirrel/shared/
      api/        ApiFactory.kt (auth + refresh), ChunkUploader.kt, UploadsFacade.kt, ApiError.kt
                  (the generated client, package app.squirrel.api, lives in build/generated/openapi)
      auth/       TokenStore.kt (interface), Session.kt (refresh single-flight)
      db/         (SQLDelight generated) + Database.kt
      hash/       Sha256.kt (expect), Hashing.kt
      sync/       AssetSource.kt, Queue.kt, Uploader.kt, SyncEngine.kt, SyncState.kt, Backoff.kt
      settings/   BackupSettings.kt
    src/commonMain/sqldelight/app/squirrel/shared/db/  LocalAsset.sq, ScanState.sq, Setting.sq, Secret.sq
    src/jvmMain, androidMain             Sha256 actuals, drivers, Keystore token store
    src/iosMain/kotlin/app/squirrel/shared/ios/
      photos/     PhotoKitSource.kt, AssetExporter.kt, PhotoAccess.kt, LibraryObserver.kt
      transfer/   BackgroundTransport.kt (NSURLSession delegate), TransportStore.kt
      background/ BackgroundTasks.kt (BGTaskScheduler), IosConditions.kt (NWPathMonitor, battery)
      KeychainTokenStore.kt, Sha256.ios.kt, Driver.kt, IosEntry.kt (functions Swift calls)
    src/commonTest, jvmTest, iosTest
  theme/                       KMP + Compose Multiplatform, the one design-system module for both platforms:
                               generated Tokens.kt and icons, SquirrelTheme, type, shapes, motion, shadows,
                               components (Button, TextField, ListRow, Card, Well, Alert, …)
    src/commonMain/composeResources/font/Geist[wght].ttf (+ OFL.txt)
  ui/                          KMP + Compose Multiplatform: screens, view models, navigation (depends on theme,
                               shared); iosMain: MainViewController, file opening/sharing; exports the iOS
                               framework `SquirrelKit` (shared + theme + ui)
  androidApp/                  com.android.application: Application, MainActivity, workers, MediaStore source
  iosApp/                      Xcode project: SquirrelApp.swift + AppDelegate.swift (entry and glue only),
                               Info.plist, iosAppTests/ (XCTest, test code only)
```

### Task 14: Gradle build, wrapper and an empty app that launches

**Files:**
- Create: everything under `mobile/` listed above except source code, `mobile/.gitignore`
  (`.gradle/`, `build/`, `local.properties`, `.kotlin/`, `iosApp/**/xcuserdata/`, `*.iml`, `.idea/`)
- Modify: root `.gitignore` if it ignores `*.jar` (the wrapper jar must be committed)

**Steps:**
1. Generate the wrapper **without** a global Gradle: download `gradle-9.7.1-bin.zip` and its
   published `.sha256` from `services.gradle.org/distributions/` into the scratchpad, check the
   hash, unzip it there, and from `mobile/` run
   `<scratch>/gradle-9.7.1/bin/gradle wrapper --gradle-version 9.7.1 --gradle-distribution-sha256-sum <sha>`.
   That writes `gradlew`, `gradlew.bat`, `gradle/wrapper/gradle-wrapper.jar` and the properties
   (with `distributionSha256Sum`); the unzipped distribution is then deleted. The download (~140 MB)
   is the only one Milestone D needs besides Maven dependencies: **ask before running it.**
2. `libs.versions.toml`:

```toml
[versions]
kotlin = "2.4.21"
agp = "9.3.3"
compose-multiplatform = "1.12.2"
jb-navigation = "2.10.0"
jb-lifecycle = "2.11.0"
ktor = "3.6.1"
serialization = "1.13.0"
coroutines = "1.11.0"
datetime = "0.8.0"
sqldelight = "2.4.1"
work = "2.12.0"
activity-compose = "1.13.0"
core-ktx = "1.19.1"
coil = "3.6.3"
turbine = "1.2.1"
robolectric = "4.18"
androidx-test = "1.7.0"
androidx-test-ext = "1.3.0"
openapi-generator = "7.27.0"

[plugins]
kotlin-multiplatform = { id = "org.jetbrains.kotlin.multiplatform", version.ref = "kotlin" }
kotlin-android = { id = "org.jetbrains.kotlin.android", version.ref = "kotlin" }
kotlin-serialization = { id = "org.jetbrains.kotlin.plugin.serialization", version.ref = "kotlin" }
compose-compiler = { id = "org.jetbrains.kotlin.plugin.compose", version.ref = "kotlin" }
compose-multiplatform = { id = "org.jetbrains.compose", version.ref = "compose-multiplatform" }
android-application = { id = "com.android.application", version.ref = "agp" }
android-kmp-library = { id = "com.android.kotlin.multiplatform.library", version.ref = "agp" }
sqldelight = { id = "app.cash.sqldelight", version.ref = "sqldelight" }
openapi-generator = { id = "org.openapi.generator", version.ref = "openapi-generator" }

[libraries]
ktor-client-core = { module = "io.ktor:ktor-client-core", version.ref = "ktor" }
ktor-client-okhttp = { module = "io.ktor:ktor-client-okhttp", version.ref = "ktor" }
ktor-client-darwin = { module = "io.ktor:ktor-client-darwin", version.ref = "ktor" }
ktor-client-content-negotiation = { module = "io.ktor:ktor-client-content-negotiation", version.ref = "ktor" }
ktor-serialization-json = { module = "io.ktor:ktor-serialization-kotlinx-json", version.ref = "ktor" }
ktor-client-mock = { module = "io.ktor:ktor-client-mock", version.ref = "ktor" }
serialization-json = { module = "org.jetbrains.kotlinx:kotlinx-serialization-json", version.ref = "serialization" }
coroutines-core = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-core", version.ref = "coroutines" }
coroutines-android = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-android", version.ref = "coroutines" }
coroutines-test = { module = "org.jetbrains.kotlinx:kotlinx-coroutines-test", version.ref = "coroutines" }
datetime = { module = "org.jetbrains.kotlinx:kotlinx-datetime", version.ref = "datetime" }
sqldelight-android = { module = "app.cash.sqldelight:android-driver", version.ref = "sqldelight" }
sqldelight-native = { module = "app.cash.sqldelight:native-driver", version.ref = "sqldelight" }
sqldelight-jvm = { module = "app.cash.sqldelight:sqlite-driver", version.ref = "sqldelight" }
sqldelight-coroutines = { module = "app.cash.sqldelight:coroutines-extensions", version.ref = "sqldelight" }
jb-navigation-compose = { module = "org.jetbrains.androidx.navigation:navigation-compose", version.ref = "jb-navigation" }
jb-lifecycle-viewmodel-compose = { module = "org.jetbrains.androidx.lifecycle:lifecycle-viewmodel-compose", version.ref = "jb-lifecycle" }
jb-lifecycle-runtime-compose = { module = "org.jetbrains.androidx.lifecycle:lifecycle-runtime-compose", version.ref = "jb-lifecycle" }
coil-compose = { module = "io.coil-kt.coil3:coil-compose", version.ref = "coil" }
coil-network-ktor3 = { module = "io.coil-kt.coil3:coil-network-ktor3", version.ref = "coil" }
androidx-work = { module = "androidx.work:work-runtime-ktx", version.ref = "work" }
androidx-work-testing = { module = "androidx.work:work-testing", version.ref = "work" }
androidx-activity-compose = { module = "androidx.activity:activity-compose", version.ref = "activity-compose" }
androidx-core-ktx = { module = "androidx.core:core-ktx", version.ref = "core-ktx" }
turbine = { module = "app.cash.turbine:turbine", version.ref = "turbine" }
robolectric = { module = "org.robolectric:robolectric", version.ref = "robolectric" }
androidx-test-core = { module = "androidx.test:core", version.ref = "androidx-test" }
androidx-test-runner = { module = "androidx.test:runner", version.ref = "androidx-test" }
androidx-test-ext-junit = { module = "androidx.test.ext:junit", version.ref = "androidx-test-ext" }
```

   (`androidx.work:work-testing` 2.12.0 exists in Google Maven, checked.)
3. Minimal `shared` (one `expect fun platformName(): String` and a `jvmTest` asserting it),
   minimal `theme` (an empty `SquirrelTheme {}`), minimal `ui` (`App()` composable showing "Squirrel"), `androidApp` (`MainActivity` with
   `setContent { App() }`, `minSdk = 29`, `compileSdk = 36`, `targetSdk = 36`,
   `applicationId = "app.squirrel.android"`, `versionName = "0.1.0"`).
4. **Run:** `(cd mobile && ./gradlew :shared:jvmTest :androidApp:assembleDebug)` → BUILD SUCCESSFUL;
   install on the emulator (`adb install -r androidApp/build/outputs/apk/debug/androidApp-debug.apk`)
   and launch it. If AGP or Kotlin fail on JDK 25, apply Open question 1's fallback and note it in
   the README.
5. **Commit** — `chore(mobile): Gradle build with shared, ui and Android app modules`.

---

### Task 15: MASTER.md phone additions, generated tokens, theme

**Files:**
- Modify: `MASTER.md` (a "Phone app" subsection, Open question 9, **after the user validates it**)
- Create: `web/scripts/native-tokens.mjs`, `web/scripts/native-tokens.test.ts`,
  `mobile/theme/src/commonMain/kotlin/app/squirrel/theme/{Tokens.kt (generated),Theme.kt,Type.kt,Shapes.kt,Motion.kt,Shadow.kt}`,
  `mobile/theme/src/commonMain/composeResources/font/` (Geist variable TTF + `OFL.txt`)
- Modify: `web/package.json` (script `gen:native-tokens`)

**Step 1: Failing test** (`native-tokens.test.ts`, Vitest): feeding a small CSS sample with
`:root { --accent: #9a3fd0; --space-2: 8px; --dur-fast: 160ms; }` and a dark block yields Kotlin
with `val accent = Color(0xFF9A3FD0)`, `val space2 = 8.dp`, `const val durFast = 160`; `rgb(20 20 30 / .045)`
becomes `Color(red = 20, green = 20, blue = 30, alpha = 11)` (rounded alpha × 255); a two-stop
`linear-gradient` becomes a `GradientTokens` pair; `color-mix(in srgb, X N%, Y)` is resolved to a
literal; an unknown construct **fails** with the token name (never silently dropped).
**Step 2: Implement** the script (regex over custom-property declarations inside `:root`, the dark
media query and `[data-theme="dark"]` blocks), **run** it to write `Tokens.kt`
(header: `// Generated from web/src/lib/styles/tokens.css by web/scripts/native-tokens.mjs. Do not edit.`).
**Step 3:** `Theme.kt` maps the tokens to a `SquirrelTheme` (`CompositionLocal` of `Tokens.Light`
or `Tokens.Dark` by `isSystemInDarkTheme()`; no Material colour scheme is used for anything
visible); `Type.kt` builds the scale from `--text-*` with Geist weights 400/500/600 and the
MASTER.md tracking; `Shapes.kt` the radii (`RoundedCornerShape(percent = 50)` for pills);
`Motion.kt` the durations and `CubicBezierEasing(0.22f, 1f, 0.36f, 1f)`, returning 0 when
reduced motion is on; `Shadow.kt` `Modifier.cardShadow()` / `popShadow()` over `Modifier.dropShadow`.
**Step 4: Run** `pnpm test:unit` and `(cd mobile && ./gradlew :theme:jvmTest)` (a smoke test that
`SquirrelTheme` provides the light and dark token sets). **Commit** — `feat(mobile): derive the native theme from tokens.css`.

---

### Task 16: Generated API client (openapi-generator)

**Files:**
- Modify: `mobile/gradle/libs.versions.toml` (plugin `openapi-generator`), `mobile/shared/build.gradle.kts`
- Create: `mobile/shared/openapi/config.yaml` (generator options), `mobile/shared/openapi/.openapi-generator-ignore`,
  `mobile/shared/src/commonTest/kotlin/app/squirrel/shared/api/GeneratedClientTest.kt`

**Generation** (Gradle plugin `org.openapi.generator` **7.27.0**, from the Gradle Plugin Portal,
checked 2026-11-16; Maven Central's copy stops at 7.14.0):

```kotlin
// mobile/shared/build.gradle.kts
openApiGenerate {
    generatorName.set("kotlin")
    library.set("multiplatform")                    // Ktor client + kotlinx.serialization
    inputSpec.set(rootProject.file("../openapi.json").path)   // the committed contract
    outputDir.set(layout.buildDirectory.dir("generated/openapi").get().asFile.path)
    packageName.set("app.squirrel.api")
    configOptions.set(mapOf(
        "dateLibrary" to "kotlinx-datetime",
        "enumUnknownDefaultCase" to "true",          // a newer server can add enum values
        "omitGradleWrapper" to "true",
        "omitGradlePluginVersions" to "true",
    ))
    generateApiTests.set(false); generateModelTests.set(false)
    generateApiDocumentation.set(false); generateModelDocumentation.set(false)
    validateSpec.set(true)
}
kotlin.sourceSets.commonMain {
    kotlin.srcDir(layout.buildDirectory.dir("generated/openapi/src/commonMain/kotlin"))
}
tasks.matching { it.name.startsWith("compileKotlin") || it.name.endsWith("SourcesJar") }
    .configureEach { dependsOn("openApiGenerate") }
```

- **Not committed:** the output lives in `shared/build/generated/openapi/` and is regenerated on
  every build from the committed `openapi.json`, which is already the reviewed contract (the server
  commits it, and `tests/openapi.rs` refuses a stale one). Committing ~100 generated files would
  only duplicate that diff.
- **Only the sources are used.** The generator's own `build.gradle.kts` (which pins Ktor 3.5.1,
  kotlinx.serialization 1.11.0, Kotlin 2.4.0 and adds `js` and `iosX64` targets) is ignored
  (`.openapi-generator-ignore`); the generated code compiles inside `shared` against **our**
  catalogue: Ktor 3.6.1, kotlinx.serialization 1.13.0, Kotlin 2.4.21. Ktor keeps source
  compatibility within 3.x, so this is expected to compile; **verified in this task** by compiling
  and by the MockEngine tests below. If 3.6 breaks a generated call, first fix it with a minimal
  template override (`templateDir`, only the file concerned); pinning the whole app to Ktor 3.5.x
  is the last resort and needs recording here.

**What is hand-written, and why** (`mobile/shared/src/commonMain/kotlin/app/squirrel/shared/api/`):
1. `ApiFactory.kt`, **auth and refresh.** The generated `ApiClient` only holds a static token
   (`setBearerToken`); it cannot refresh on 401 or deduplicate concurrent refreshes. The factory
   builds the generated API classes with our own `HttpClient` configuration
   (`httpClientEngine` + `httpClientConfig` constructor parameters): Ktor's `Auth` plugin with a
   `bearer { loadTokens; refreshTokens; sendWithoutRequest { true } }` provider reading the
   `Session`/`TokenStore` (Task 18). Ktor's provider already serialises refreshes; the refresh call
   itself uses the generated `DevicesApi.refreshDeviceToken` on a **second** client without the
   plugin (no recursion). `HttpTimeout` 30 s; `Json { ignoreUnknownKeys = true }`.
2. `ChunkUploader.kt`, **chunk PUTs.** The generated method takes the whole body as a value and
   only knows the bearer scheme. The app must stream an 8 MiB slice straight from an
   `AssetReader` with an exact `Content-Length`, authenticate with either the bearer (Android) or
   `Authorization: Upload <token>` (iOS background transfers), and on iOS hand the request to
   `NSURLSession` instead of Ktor (Task 41). So `PUT /api/uploads/{id}/chunks/{n}` is one hand-written
   Ktor call (`setBody(object : OutgoingContent.WriteChannelContent() { … })`) with a 5-minute timeout
   (the server's `CHUNK_TIMEOUT`). The path and headers come from the same `openapi.json`
   operation, checked by test 4 below.
3. `UploadsFacade.kt`, **one status-dependent response.** `POST /api/uploads` answers 200
   `DeduplicatedUpload` or 201 `UploadSessionDto`; the generator types an operation by one success
   schema. The facade calls the generated method, reads the raw status from its `HttpResponse`
   wrapper, and decodes the body with the generated serializer for that status into
   `UploadOutcome.Deduplicated` / `UploadOutcome.Session`.
4. `ApiError.kt`: maps non-2xx responses' `{error:{code,message}}` envelope (the generated
   `ErrorBody`) to `ApiError(status, code, message)`.

Everything else (health, setup status, device login/refresh/logout, devices/current, sync check
and folder, upload status/complete/abort, photos, albums, nodes, storage, thumbnails and content
URLs) is the generated code.

**Step 1: Failing tests** (`MockEngine`): device login sends the generated request and decodes
`DeviceLoginResponse`; a data call carries `Authorization: Bearer sqa_…`; a 401 triggers **one**
refresh and a retry, and two concurrent 401s cause one refresh; a failed refresh surfaces
`SignedOut`; `createUpload` maps 200/201 to the two outcomes; the error envelope maps to `ApiError`;
`ChunkUploader` sends `PUT /api/uploads/{id}/chunks/2` with `application/octet-stream`, the exact
`Content-Length` and the bytes of slice 2, under either auth scheme (test 4 also asserts that
`openapi.json` declares that operation with an `application/octet-stream` body and both security
schemes).
**Step 2: Implement.** **Step 3: Run** `(cd mobile && ./gradlew :shared:openApiGenerate :shared:jvmTest)` → PASS.
**Commit** — `feat(mobile): generate the API client from openapi.json`.

---

### Task 17: API drift check

**Files:** `mobile/shared/build.gradle.kts` (task `checkApiClient`), `.github/workflows/ci.yml` (Task 46 adds it to CI)

The equivalent of the web's `pnpm gen:api && git diff --exit-code`: since the output is not
committed, drift shows up as **the app no longer compiling or its API tests failing against a
freshly generated client**. `checkApiClient` = `openApiValidate` (the spec is valid OpenAPI) →
`openApiGenerate` from a clean output directory → `compileKotlinJvm` → the API tests of Task 16.
A server change that renames a field the app uses, drops an operation, or changes a security scheme
breaks this task in the same pull request that changes `openapi.json`. Run it locally after every
`openapi.json` regeneration (the conventions line above). Prove it once: rename a property in a
scratch copy of `openapi.json` passed with `-PopenapiSpec=…`, see it fail, discard the copy.
**Commit** — `build(mobile): check the generated client against openapi.json`.

---

### Task 18: Database, settings and the token store

**Files:**
- Create: `mobile/shared/src/commonMain/sqldelight/app/squirrel/shared/db/{LocalAsset.sq,ScanState.sq,Setting.sq,Secret.sq}`,
  `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/{db/Database.kt,settings/BackupSettings.kt,auth/TokenStore.kt,auth/Session.kt}`,
  `mobile/shared/src/{jvmMain,androidMain,iosMain}/kotlin/app/squirrel/shared/db/Driver.kt`,
  `mobile/shared/src/androidMain/kotlin/app/squirrel/shared/auth/KeystoreTokenStore.kt`,
  `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/auth/KeychainTokenStore.kt`,
  tests in `commonTest` (with `expect fun testDriver(): SqlDriver`: `JdbcSqliteDriver(IN_MEMORY)` on
  JVM, `inMemoryDriver(...)` on iOS)

```sql
-- LocalAsset.sq: one row per phone asset the app knows about.
CREATE TABLE local_asset (
    device_asset_id TEXT NOT NULL PRIMARY KEY,
    kind TEXT NOT NULL,                    -- 'photo' | 'video'
    name TEXT NOT NULL,
    mime TEXT,
    size INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,          -- epoch ms, change detection
    taken_at TEXT,                         -- RFC 3339 with offset, sent with the upload
    album TEXT,                            -- bucket / album name, for the settings filter
    sha256 TEXT,                           -- NULL until hashed for (size, modified_at)
    state TEXT NOT NULL DEFAULT 'new',     -- new | hashed | missing | uploading | done | skipped | failed
    upload_id TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);
CREATE INDEX local_asset_state ON local_asset (state, next_attempt_at);

upsertScanned:
INSERT INTO local_asset (device_asset_id, kind, name, mime, size, modified_at, taken_at, album)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (device_asset_id) DO UPDATE SET
    name = excluded.name, mime = excluded.mime, album = excluded.album, taken_at = excluded.taken_at,
    -- A changed file is hashed and checked again.
    sha256 = CASE WHEN size = excluded.size AND modified_at = excluded.modified_at THEN sha256 ELSE NULL END,
    state  = CASE WHEN size = excluded.size AND modified_at = excluded.modified_at THEN state ELSE 'new' END,
    size = excluded.size, modified_at = excluded.modified_at;
```

(plus named queries `nextToHash`, `setHash`, `hashedBatch(limit)`, `markMissing`, `markDone`,
`markFailed`, `counts` grouped by state, `bytesLeft`).

**Failing tests:** upsert keeps the hash of an unchanged file and resets a changed one;
`counts()`; `BackupSettings` round-trips (`wifiOnly = true`, `chargingOnly = false`,
`includeVideos = true`, `albums = AllAlbums | Selected(set)`) through `Setting.sq`; `Session`
stores the refresh token through `TokenStore`, keeps the access token in memory only, and clears
everything on `signOut()`.
**Implement:** Android `KeystoreTokenStore` (key alias `squirrel-refresh`, `KeyGenParameterSpec`
`PURPOSE_ENCRYPT|DECRYPT`, `BLOCK_MODE_GCM`, `ENCRYPTION_PADDING_NONE`, 256-bit, no user auth;
IV stored with the ciphertext in `secret`); iOS `KeychainTokenStore` (`SecItemAdd`/`SecItemCopyMatching`/`SecItemDelete`
via `platform.Security`, service `app.squirrel`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`).
These two are covered by the instrumented/iOS tests of Tasks 31 and 43 (the JVM test uses an
in-memory `TokenStore`). Android: `android:allowBackup="false"` and `dataExtractionRules` excluding
the database (the keystore key does not travel, so a restored ciphertext is useless anyway).
**Run** `:shared:jvmTest` → PASS. **Commit** — `feat(mobile): local database, backup settings and token storage`.

---

### Task 19: Server and sign-in screens

**Files:**
- Create: `mobile/theme/src/commonMain/kotlin/app/squirrel/theme/components/*`,
  `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/{signin/ServerScreen.kt,signin/SignInScreen.kt,signin/SignInViewModel.kt,App.kt,AppGraph.kt}`,
  `mobile/ui/src/commonTest/kotlin/app/squirrel/ui/signin/SignInViewModelTest.kt`

**Components** (in the `theme` module: Compose versions of `web/src/lib/components/ui/`, same
names and states, used unchanged by both platforms):
`Button` (primary/secondary/quiet/danger, sm 36 / md 44 / lg 48, 44 dp minimum touch target),
`TextField` (pill, edge, invalid state with icon and message), `Card`, `Well`, `ListRow`, `Alert`,
`Badge`, `Progress`, `EmptyState`, `Toggle`, `IconButton`, the floating `BottomRail`.
Icons: the same Lucide paths as the web (`web/src/lib/components/icons/paths.ts`), converted to
`ImageVector`s by the token script's sibling `web/scripts/native-icons.mjs` (same drift check).

**Screens:** *Server*: wordmark on the `--ground` gradient, one white card: "Your Squirrel's
address" field (`https://` prefilled), "Continue" primary. On continue the view model normalises
the URL (adds `https://`, strips a trailing `/`, refuses `http://` outside debug builds), calls
`healthz` then `setup/status`; errors in an error `Alert` ("No Squirrel answered at this address",
"This Squirrel is not set up yet: finish the setup in a browser first"). *Sign in*: username,
password, "Sign in" primary, muted line "Have an invitation? Open the link in your browser, then
sign in here." The device name defaults to the phone model (Android `Build.MODEL`, iOS
`UIDevice.current.model`), editable later in Backup › This phone.

**Failing tests** (view model with a `MockEngine` API): invalid URL → field error; healthz fails →
alert; success → `SignedIn` state with the account; `invalid_credentials` → "Wrong username or
password"; `rate_limited` → "Too many attempts. Try again in a minute."
**Run** `:shared:jvmTest :ui:jvmTest`. **Commit** — `feat(mobile): server and sign-in screens`.

---

### Task 20: Android wiring and a signed-in shell

**Files:**
- Create: `mobile/androidApp/src/main/{AndroidManifest.xml,res/xml/network_security_config.xml,res/xml/data_extraction_rules.xml}`,
  `mobile/androidApp/src/main/kotlin/app/squirrel/android/{SquirrelApp.kt,MainActivity.kt,AndroidGraph.kt}`,
  `mobile/androidApp/src/debug/res/xml/network_security_config.xml` (clear text to `10.0.2.2`, `localhost`)

The shell after sign-in: bottom rail Photos / Files / Backup, each an `EmptyState` placeholder
until Milestones F–G; the account menu shows the server, the user's display name and "Sign out"
(calls `device/logout`, clears the store).
**Verify on the emulator** against the dev server (`cargo run -p squirrel -- serve --worker` with
`SQUIRREL_…` dev settings, reached at `http://10.0.2.2:8080`): sign in, see the shell, the device
appears in the web's Settings › Devices, revoking it there sends the app back to sign-in on its
next request. **Commit** — `feat(android): sign in and the app shell`.

---

## Milestone E — Shared sync engine

### Task 21: Streaming SHA-256

**Files:**
- Create: `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/hash/{Sha256.kt,Hashing.kt}`,
  `mobile/shared/src/{jvmMain,androidMain}/kotlin/app/squirrel/shared/hash/Sha256.jvm.kt` (shared
  via an intermediate `jvmCommon` source set if AGP's KMP plugin allows; else two small copies),
  `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/hash/Sha256.ios.kt`,
  `mobile/shared/src/commonTest/kotlin/app/squirrel/shared/hash/Sha256Test.kt`

```kotlin
expect class Sha256() {
    fun update(bytes: ByteArray, offset: Int = 0, length: Int = bytes.size)
    /** Lowercase hex, 64 characters. The instance cannot be reused afterwards. */
    fun digestHex(): String
}

/** Hashes [reader] in [bufferSize] steps without holding the file in memory. */
suspend fun hashAsset(reader: AssetReader, bufferSize: Int = 1 shl 20, onProgress: (Long) -> Unit = {}): String
```

**Tests:** NIST vectors (`""`, `"abc"`, the 448-bit and 896-bit messages, one million `'a'`s fed in
odd-sized pieces); `hashAsset` over a fake 20 MiB reader equals hashing it at once; progress is
monotonic and ends at the size; cancellation stops reading. **Run** on JVM (and iOS from Task 43).
**Commit** — `feat(mobile): streaming SHA-256 on both platforms`.

### Task 22: `AssetSource`, the scanner and the queue

**Files:** `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/sync/{AssetSource.kt,Scanner.kt,Queue.kt}` + tests

```kotlin
interface AssetSource {
    /** Assets changed since [token] (null: everything visible), and the token to store next. */
    suspend fun changes(token: String?, filter: SourceFilter): ScanResult
    suspend fun open(deviceAssetId: String): AssetReader?   // null: gone from the phone
    fun access(): MediaAccess                                // Full | Partial(count) | None
}
interface AssetReader : AutoCloseable {
    val size: Long
    suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int  // -1 at end
}
```

**Tests** with an in-memory `FakeAssetSource`: a first scan inserts every asset; a second scan with
the stored token applies only the changes; a full rescan (token `null`, e.g. after the user changed
album filters) also marks assets that are no longer selected as `skipped` (never deleted from the
server); `includeVideos = false` skips videos. **Commit** — `feat(mobile): scan phone assets into the queue`.

### Task 23: Chunked, resumable uploader

**Files:** `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/sync/{Uploader.kt,ChunkTransport.kt,Backoff.kt}` + tests

```kotlin
/** Sends one chunk. Ktor on Android; background URLSession on iOS (Milestone H). */
interface ChunkTransport {
    suspend fun put(session: UploadSession, index: Int, reader: AssetReader): ChunkResult
}
```

`Uploader.upload(asset)`:
1. `POST /api/sync/folder {year}` (cached per year for the run) → `parent_id`.
2. `POST /api/uploads {parent_id, name, size, sha256, taken_at, device_asset_id}` → `Deduplicated`
   (done) or `Session(upload_id, chunk_size, chunk_count, received)`. The returned `chunk_size`
   is authoritative.
3. Send the chunks not in `received`, at most 2 at a time, through `ChunkTransport`; after a
   process restart, `GET /api/uploads/{id}` gives `received` again.
4. `POST /api/uploads/{id}/complete` → done (record the node id).

Errors: 401 → one refresh then retry (Task 16); 404 on a chunk or complete (session expired or
aborted) → start again from step 2; 409 `upload_incomplete` → re-read status and send what is
missing; 422 `hash_mismatch` → the file changed: clear the hash, back to `new`; 507
`quota_exceeded`/`storage_full` → stop the whole run with a visible reason; 429/503 → back off
(`Backoff`: 5 s · 2^n, max 15 min, jitter 0.8–1.2) and retry; network errors → retry with backoff;
after 8 attempts the asset is `failed` with its last error (shown in the UI, retried on the next
"Back up now"). An asset that disappeared from the phone (`open` → null) becomes `skipped`.
**Tests** (MockEngine + fake transport): fresh upload of a 20 MiB file sends 3 chunks and
completes; resume after "process death" sends only missing chunks; dedup path sends no chunk;
each error branch above. **Commit** — `feat(mobile): resumable chunked uploads`.

### Task 24: Sync engine

**Files:** `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/sync/{SyncEngine.kt,SyncState.kt,Conditions.kt}` + tests

```kotlin
class SyncEngine(
    private val source: AssetSource,
    private val api: SquirrelApi,
    private val queue: Queue,
    private val uploader: Uploader,
    private val conditions: Conditions,      // network unmetered? charging? battery ok? (platform)
    private val settings: BackupSettings,
    private val clock: Clock,
) {
    val state: StateFlow<SyncState>          // Idle | Scanning | Hashing(n, of) | Checking | Uploading(name, bytes, of) | Paused(reason) | Error(reason)

    /** One pass: scan → hash → check (≤ 500 per call) → upload missing. Stops early when
     *  conditions no longer hold or [deadline] passes; everything done so far is kept. */
    suspend fun runOnce(deadline: Instant? = null): RunSummary
}
```

**Tests:** a pass over 1 200 fake assets checks them in three batches of ≤ 500 and uploads only
the missing ones; assets already `done` are never re-checked; conditions turning false mid-run
(Wi-Fi lost) stop after the current chunk with `Paused(WifiRequired)`; the deadline stops cleanly;
`quota_exceeded` gives `Error(QuotaExceeded)` and leaves the rest untouched; two concurrent
`runOnce` calls do not overlap (a `Mutex`; the second returns at once); `SignedOut` (refresh
failed) stops and clears. State emissions checked with Turbine. **Commit** — `feat(mobile): the sync engine`.

### Task 25: Backup status model

**Files:** `mobile/shared/src/commonMain/kotlin/app/squirrel/shared/sync/BackupStatus.kt` + tests

`BackupStatus(total, backedUp, left, bytesLeft, failed, lastRun, access: MediaAccess, paused)`
derived from `queue.counts()` and the engine state as a `Flow`; formatting helpers (`"1 204 left"`,
`"3.2 GB"`, `"Backed up 2 min ago"`) shared by both apps (tabular figures in the UI).
**Commit** — `feat(mobile): backup status for the screens`.

**Milestone E check:** `(cd mobile && ./gradlew :shared:jvmTest)` green; coverage of `shared`
≥ 80 % (Kover is not added; the JVM test report is enough, or run the IDE coverage once).

---

## Milestone F — Android backup end to end

### Task 26: Photo permissions, including partial access

**Files:**
- Create: `mobile/androidApp/src/main/kotlin/app/squirrel/android/media/{MediaPermissions.kt,MediaAccessReader.kt}`,
  `mobile/androidApp/src/test/kotlin/app/squirrel/android/media/MediaPermissionsTest.kt` (Robolectric)
- Modify: `AndroidManifest.xml`

Manifest: `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, `READ_MEDIA_VISUAL_USER_SELECTED`,
`READ_EXTERNAL_STORAGE` with `android:maxSdkVersion="32"`, `ACCESS_MEDIA_LOCATION`,
`POST_NOTIFICATIONS`, `INTERNET`, `ACCESS_NETWORK_STATE`, `FOREGROUND_SERVICE`,
`FOREGROUND_SERVICE_DATA_SYNC`, `RECEIVE_BOOT_COMPLETED` (WorkManager re-arms its jobs).

`MediaPermissions.request(launcher)` asks for the set that fits the API level (33+: images, video,
user-selected on 34+, media location; ≤ 32: storage, media location). `MediaAccessReader.current()`
→ `Full` when `READ_MEDIA_IMAGES` is granted, `Partial` when only `READ_MEDIA_VISUAL_USER_SELECTED`
is (34+), else `None`. `ACCESS_MEDIA_LOCATION` denied → a separate warning "Photo locations won't be
backed up" (the original bytes can't be read without it; the app then reads the redacted bytes and
the hash will not match the original's: accepted and stated).
**Tests (Robolectric, `@Config(sdk = [32, 33, 34, 35])`):** the requested set per level; the access
mapping per grant combination. **Commit** — `feat(android): photo permissions with partial access`.

### Task 27: MediaStore asset source

**Files:**
- Create: `mobile/androidApp/src/main/kotlin/app/squirrel/android/media/{MediaStoreSource.kt,MediaStoreReader.kt,Albums.kt}`,
  `mobile/androidApp/src/androidTest/kotlin/app/squirrel/android/media/MediaStoreSourceTest.kt`

- Query `MediaStore.Images.Media.getContentUri(VOLUME_EXTERNAL)` and the video equivalent with
  projection `_ID, DISPLAY_NAME, MIME_TYPE, SIZE, DATE_TAKEN, DATE_MODIFIED, BUCKET_DISPLAY_NAME,
  GENERATION_MODIFIED (API 30+)`, excluding `IS_PENDING = 1` and `IS_TRASHED = 1`
  (`QUERY_ARG_MATCH_TRASHED` left at its default, i.e. excluded).
- Change token: `"<volume version>:<max generation>"` on API 30+ (`MediaStore.getVersion`,
  `getGeneration`; a different volume version means a full rescan); on API 29 the max
  `DATE_MODIFIED` (seconds) minus a 2 s overlap.
- `taken_at`: `DATE_TAKEN` (epoch ms) at the device zone's offset for that instant
  (`TimeZone.getDefault().getOffset(ms)`), formatted RFC 3339; `null` if `DATE_TAKEN` is 0/null.
- `open(id)`: `MediaStore.setRequireOriginal(uri)` when `ACCESS_MEDIA_LOCATION` is granted, then
  `contentResolver.openFileDescriptor(uri, "r")` → `FileInputStream(fd).channel` with positional
  `read(ByteBuffer, position)`; `FileNotFoundException`/`SecurityException` → `null` (gone or no longer
  visible).
- Albums for the settings screen: distinct `BUCKET_ID`/`BUCKET_DISPLAY_NAME` with counts.

**Instrumented test** (emulator): insert two images and one video through `MediaStore` with
`IS_PENDING` dance, scan finds them with the right sizes, names and buckets; the token makes the next
scan return only a newly inserted image; `open` reads bytes equal to what was written; a deleted
image's `open` returns null. Run `(cd mobile && ./gradlew :androidApp:connectedDebugAndroidTest)`
with `Medium_Phone_API_36.1` booted (`~/Library/Android/sdk/emulator/emulator -avd Medium_Phone_API_36.1 -no-snapshot-save &`;
the test grants permissions with `GrantPermissionRule`). **Commit** — `feat(android): read photos and videos from MediaStore`.

### Task 28: WorkManager scheduling

**Files:**
- Create: `mobile/androidApp/src/main/kotlin/app/squirrel/android/sync/{BackupWorker.kt,BackupScheduler.kt,AndroidConditions.kt,ForegroundInfoFactory.kt,MediaObserver.kt}`,
  `mobile/androidApp/src/test/kotlin/app/squirrel/android/sync/BackupSchedulerTest.kt` (Robolectric + `work-testing`)

- `BackupScheduler.apply(settings)`: unique periodic work `backup-periodic` (6 h, flex 1 h,
  `ExistingPeriodicWorkPolicy.UPDATE`), unique one-time work `backup-on-new-media` with
  `addContentUriTrigger(Images.EXTERNAL_CONTENT_URI, true)`, `addContentUriTrigger(Video…, true)`,
  `setTriggerContentUpdateDelay(30 s)`, `setTriggerContentMaxDelay(10 min)` (re-enqueued at the end
  of each run, `ExistingWorkPolicy.KEEP`); "Back up now" = expedited one-time work
  (`setExpedited(RUN_AS_NON_EXPEDITED_WORK_REQUEST)`). Constraints for all of them:
  `NetworkType.UNMETERED` if Wi-Fi only else `CONNECTED`, `setRequiresCharging(chargingOnly)`,
  `setRequiresBatteryNotLow(true)`.
- `BackupWorker` (`CoroutineWorker`): builds the engine from `AndroidGraph`, calls
  `setForeground(...)` with a `dataSync` notification ("Backing up 12 of 340") when more than
  20 assets or 200 MB are pending, runs `runOnce(deadline = now + 9 min)` for non-foreground runs
  (WorkManager's 10-minute limit), `Result.success()` even when paused (the constraints will
  re-trigger), `Result.retry()` on transient errors. Android 15's 6-hour daily limit for `dataSync`
  foreground services: on `onStopped` the engine keeps what it did and the next run continues.
- `MediaObserver`: a `ContentObserver` registered while `MainActivity` is started, debounced 5 s,
  enqueuing the one-time work.
- `AndroidConditions`: `ConnectivityManager.getNetworkCapabilities(active).hasCapability(NET_CAPABILITY_NOT_METERED)`,
  `BatteryManager.isCharging`, checked before each chunk.
- Android 16+: WorkManager runtime quotas apply even to work started in the foreground, so a run
  can be stopped early; the engine's deadline and resumability make that harmless (see "Platform
  risks").

**Tests:** the scheduler enqueues the expected requests with the expected constraints for each
setting combination (`WorkManagerTestInitHelper`, `TestDriver.setAllConstraintsMet`); the worker
with a fake engine returns `success`/`retry` per outcome. **Commit** — `feat(android): schedule backups with WorkManager`.

### Task 29: Backup screen

**Files:** `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/backup/{BackupScreen.kt,BackupViewModel.kt}` + test

Layout (phone, MASTER.md): page title "Backup" (`--text-2xl`), then the screen's **one graphite
card** (`--rail` gradient, `--rail-ink` text, `--radius-card`) holding the featured figure
"1 204 left" (`--text-display`, tabular) with "3.2 GB · Wi-Fi only" muted (`--rail-muted`) and a
`Progress` bar while uploading; below it a primary "Back up now" button (disabled while running,
label "Backing up…"). Then a `Well` of row cards: "Backed up" (count), "Waiting" (count), "Failed"
(count, `--danger` text, opens a sheet listing names and reasons with "Try again"). Pause reasons
appear as an `info` Alert ("Waiting for Wi-Fi", "Waiting for charging", "Storage on your Squirrel is
full", in plain words). Partial access: an `info` Alert "Squirrel can see 42 selected photos. Only
these are backed up." with secondary "Allow all photos" and quiet "Change selection". No access:
`EmptyState` with the `images` icon, "Allow access to your photos", primary "Allow access".
Arrival animations: none on phones (MASTER.md "Motion").
**ViewModel tests:** state mapping for each `BackupStatus`/`SyncState` combination (strings exact).
**Commit** — `feat(mobile): backup status screen`.

### Task 30: Backup settings screen

**Files:** `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/backup/{BackupSettingsScreen.kt,BackupSettingsViewModel.kt}` + test

Rows in a `Well`, each a `Toggle` row: "Back up automatically" (on), "Only on Wi-Fi" (on), "Only
while charging" (off), "Include videos" (on). "Albums" row → a sheet: "All photos" (default) or a
list of albums with counts and checkboxes. "This phone" row: device name, editable (PATCH
`/api/devices/current`); muted "Backups go to Photos/<name>/<year>". Changing a filter re-runs the
scheduler and marks the next scan as full. **Tests:** each toggle persists and calls
`scheduler.apply`; album selection persists; renaming validates 1–64 characters.
**Commit** — `feat(mobile): backup settings`.

### Task 31: Android end-to-end test against the dev server

**Files:**
- Create: `mobile/androidApp/src/androidTest/kotlin/app/squirrel/android/BackupEndToEndTest.kt`,
  `scripts/mobile-e2e-server.sh` (starts `squirrel serve --worker` on port 8188 with a fresh
  database `squirrel_mobile_e2e` on 55432, creates a user through the setup API, prints credentials;
  `check` subcommand for database assertions)

The test (skipped unless the instrumentation argument `squirrelServer` is set): inserts 3 images
(one with EXIF, one without, one duplicate of the first's bytes) and a short video into MediaStore,
signs in, runs `SyncEngine.runOnce()` in-process, then asserts through the API: 3 new files under
`Photos/<device>/<year>`, the duplicate deduplicated, `sync/check` now reports nothing missing,
and (after the worker ran) the EXIF-less image's asset has `taken_source = 'device'`
(`scripts/mobile-e2e-server.sh check` queries `assets` with `psql` on the e2e database; the API does
not expose `taken_source`). Also verifies the Keystore token
store round-trips on a real Android runtime.
**Run:** `scripts/mobile-e2e-server.sh &` then
`(cd mobile && ./gradlew :androidApp:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.squirrelServer=http://10.0.2.2:8188 …)`.
**Commit** — `test(android): back up photos from the emulator end to end`.

### Task 32: Milestone F manual pass

On the emulator and (if available) a physical Android phone: first backup of ≥ 500 photos on
Wi-Fi with the screen off (foreground notification visible, survives the app being swiped away);
airplane mode mid-upload then back (resumes, no duplicate); switch to mobile data with "Only on
Wi-Fi" (pauses with "Waiting for Wi-Fi"); take a photo with the app closed (backed up within
~10 min by the content trigger); partial access on Android 14+ (notice shown, only selected
photos uploaded, "Allow all photos" works); revoke the device on the web (app returns to sign-in).
Record results in the plan's "As built" section later. **No commit unless fixes.**

---

## Milestone G — Android gallery and files

### Task 33: Authenticated image loading

**Files:** `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/images/{ImageLoader.kt,Blurhash.kt}` + tests

Coil 3 `ImageLoader` with `KtorNetworkFetcherFactory(httpClient)` using the **authenticated** Ktor
client (so the bearer and refresh logic are shared), a memory cache, and a disk cache in the app's
cache directory (thumbnails are immutable per `v`). `Blurhash.decode(hash, 32, 32)` → `ImageBitmap`
placeholder: port the web's `web/src/lib/photos/blurhash.ts` (hand-rolled there too) with the same
test vectors (the mean colour equals the DC colour). Sign-out clears both caches.
**Commit** — `feat(mobile): authenticated thumbnails with blurhash placeholders`.

### Task 34: Photos timeline and viewer

**Files:** `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/photos/{PhotosScreen.kt,PhotosViewModel.kt,JustifiedRows.kt,PhotoViewer.kt,AlbumsScreen.kt,AlbumScreen.kt}` + tests

- Months from `GET /api/photos/months`; a `LazyColumn` with a month heading (`--text-lg`,
  `--photo-month-head` block) per month and placeholder `--tint` rows sized from the counts until
  the month's items load (`GET /api/photos?month=…`, paginated by `next_cursor`) when it nears the
  viewport. Rows are justified at `--photo-row` (112 dp) with `--space-1` gaps: port
  `web/src/lib/photos/justified.ts` and its tests. Tiles: `--radius-tile`, blurhash then thumbnail,
  `image-off` icon for `unsupported`, "Preparing…" tiles for `pending` (`thumbnail` null) as on the web.
- Tap → `PhotoViewer`: the graphite stage (`--rail` gradient), `HorizontalPager` across the loaded
  items, the `preview` variant, pinch zoom, top controls as `--rail-well` circles (close, share,
  info). Info opens a bottom sheet with date, place, camera from `GET /api/photos/{id}`. Share
  downloads the original (`content`) to the cache and opens the system share sheet.
- Albums tab inside Photos (segmented control "Timeline | Albums"): album cards per MASTER.md
  (two columns), album page = the same grid over `GET /api/albums/{id}/items`.
- Empty: `EmptyState` "No photos yet" / "Backed-up photos appear here."
**Tests:** justified layout (ported vectors), month loading state machine, pager index mapping.
**Commit** — `feat(mobile): photo timeline, viewer and albums`.

### Task 35: File browser

**Files:** `mobile/ui/src/commonMain/kotlin/app/squirrel/ui/files/{FilesScreen.kt,FilesViewModel.kt,OpenFile.kt}` + tests;
`mobile/androidApp/src/main/kotlin/app/squirrel/android/files/AndroidFileOpener.kt`,
`mobile/androidApp/src/main/res/xml/file_paths.xml` (FileProvider for the cache directory)

Breadcrumbs (pills), row cards (`ListRow`: folder or file icon well, or the thumbnail when present;
name; muted size · date), keyset paging via `GET /api/nodes?parent=…&cursor=…`. Tapping a file
downloads it with progress (`Progress` in the row) into `cacheDir/open/` and opens it with
`Intent.ACTION_VIEW` through a `FileProvider` URI; files over 200 MB ask first ("Download 1.4 GB?").
Read-only: no rename, move, delete, upload (they stay on the web; the Backup tab handles uploads).
**Tests:** paging, breadcrumb path from `GET /api/nodes/{id}` (`path` segments), download state.
**Commit** — `feat(mobile): browse and open files`.

### Task 36: Milestone G manual pass

Scroll a 5 000-photo timeline on the emulator and a physical phone (no jank at 112 dp rows, memory
stable), open and swipe photos, open a PDF and a video file from Files with the system apps, dark
mode, font scale 1.3, TalkBack labels on rail items, tiles ("Photo taken 14 July 2024") and buttons.
**No commit unless fixes.**

---

## Milestone H — iOS

### Task 37: Server: per-upload tokens for background transfers

**Files:**
- Create: `crates/squirrel-db/migrations/20261116000003_upload_tokens.sql`,
  `crates/squirrel-api/tests/upload_tokens.rs`
- Modify: `crates/squirrel-api/src/{files_dto.rs,extract.rs}`, `crates/squirrel-api/src/routes/uploads/{mod,chunks}.rs`,
  `crates/squirrel-db/src/uploads.rs`, `openapi.json`, `web/src/lib/api/schema.d.ts`

```sql
-- A capability for one upload's chunks, given to devices so background transfers outlive
-- their 15-minute access token. Stored hashed; dies with the session.
ALTER TABLE upload_sessions ADD COLUMN upload_token_hash BYTEA UNIQUE;
```

**Failing tests:**
1. A bearer `POST /api/uploads` → 201 with `upload_token` (`squ_…`); a session caller gets none;
   resuming the same session issues a **new** token (the old one stops working).
2. `PUT …/chunks/{n}` and `GET /api/uploads/{id}` with `Authorization: Upload squ_…` → accepted
   without any bearer or cookie, even after the device's access token expired.
3. The upload token is refused on every other route (route matrix from Task 5 gains this column),
   on another upload's id (404), after completion/abort/expiry (404), and once the device is
   revoked (401).
4. Completion still needs the bearer (it creates the node).
5. `openapi.json` declares an `upload_token` scheme (`http`, scheme `Upload`, description
   "Per-upload token `squ_…` from `POST /api/uploads`, for this upload's chunks and status only"),
   and those two operations list it besides `device_bearer` and the session pair; the Task 5
   document-matches-behaviour test covers it. (The generated client ignores a non-bearer HTTP
   scheme; the hand-written `ChunkUploader` sends it, Task 16.)
**Implement** an `UploadCaller` extractor for those two routes (session, bearer, or upload token
bound to the path's upload id; the device must be live). **Commit** — `feat(api): per-upload tokens for background chunk transfers`.

### Task 38: iOS targets, framework and the Xcode project

**Files:**
- Modify: `mobile/{shared,theme,ui}/build.gradle.kts` (`iosArm64()`, `iosSimulatorArm64()`; in `ui`:
  `binaries.framework { baseName = "SquirrelKit"; isStatic = true; export(project(":shared")) }`)
- Create: `mobile/ui/src/iosMain/kotlin/app/squirrel/ui/MainViewController.kt`
  (`fun MainViewController(): UIViewController = ComposeUIViewController { App(IosGraph.instance) }`),
  `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/ios/IosEntry.kt` (the functions Swift calls:
  `onLaunch()`, `onBackgroundUrlSessionEvents(identifier, completionHandler)`),
  `mobile/iosApp/iosApp.xcodeproj` (Xcode's iOS App template, bundle id `app.squirrel.ios`,
  deployment target 17.0, Swift 6), `mobile/iosApp/iosApp/{SquirrelApp.swift,AppDelegate.swift,Info.plist}`

The only Swift in the app:

```swift
import SwiftUI
import SquirrelKit

@main
struct SquirrelApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene {
        WindowGroup { ComposeView().ignoresSafeArea() }   // Compose handles insets itself
    }
}

struct ComposeView: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> UIViewController { MainViewControllerKt.MainViewController() }
    func updateUIViewController(_ controller: UIViewController, context: Context) {}
}
```

```swift
import UIKit
import SquirrelKit

/// Glue only: these two callbacks reach the app delegate and nothing else.
final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        IosEntryKt.onLaunch()   // registers BGTask handlers: must happen before launch finishes
        return true
    }
    func application(_ application: UIApplication,
                     handleEventsForBackgroundURLSession identifier: String,
                     completionHandler: @escaping () -> Void) {
        IosEntryKt.onBackgroundUrlSessionEvents(identifier: identifier, completionHandler: completionHandler)
    }
}
```

Build phase "Compile Kotlin" before "Compile Sources": `cd "$SRCROOT/.." && ./gradlew :ui:embedAndSignAppleFrameworkForXcode`;
framework search path `$(SRCROOT)/../ui/build/xcode-frameworks/$(CONFIGURATION)/$(SDK_NAME)`.
`Info.plist`: `NSPhotoLibraryUsageDescription` ("Squirrel backs up your photos and videos to your
own server."), `PHPhotoLibraryPreventAutomaticLimitedAccessAlert = YES`,
`BGTaskSchedulerPermittedIdentifiers = [app.squirrel.backup.processing, app.squirrel.backup.refresh, app.squirrel.backup.now]`,
`UIBackgroundModes = [fetch, processing]`, `CADisableMinimumFrameDurationOnPhone = YES` (120 Hz
scrolling for Compose on ProMotion screens). Release builds keep App Transport Security on; debug
adds an `NSExceptionDomains` entry for `localhost` only (the simulator reaches the host as `localhost`).
**Verify:** `xcodebuild -project mobile/iosApp/iosApp.xcodeproj -scheme iosApp -destination 'platform=iOS Simulator,name=<a device from xcrun simctl list devices available>' build`,
run it: the shared server and sign-in screens work against `http://localhost:8080`;
`(cd mobile && ./gradlew :shared:iosSimulatorArm64Test)` runs the Sha256 vectors and the database on
Kotlin/Native. **Measure and record** the app's size (Xcode's App Thinning Size Report for an
`iPhone` variant), see "Risks of Compose Multiplatform on iOS".
**Commit** — `feat(ios): Xcode entry point hosting the shared Compose UI`.

### Task 39: Keychain token store and the iOS graph (Kotlin)

`KeychainTokenStore` (Task 18, `platform.Security`: `SecItemAdd`, `SecItemCopyMatching`,
`SecItemUpdate`, `SecItemDelete` with a `CFDictionary` built through `CFDictionaryCreateMutable`,
service `app.squirrel`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`). `IosGraph` (Kotlin)
wires the API, database, token store, source, transport and engine. Tested in `iosTest` on the
simulator (store, read, overwrite, delete, accessibility attribute). **Note:** Keychain calls from
the Kotlin/Native test executable may fail with `errSecMissingEntitlement` because it is not a
signed app; if so, these tests move to the XCTest target (Task 44), calling the Kotlin class.
**Commit** — `feat(ios): keychain token storage`.

### Task 40: PhotoKit asset source (Kotlin/Native)

**Files:** `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/ios/photos/{PhotoKitSource.kt,AssetExporter.kt,PhotoAccess.kt,LibraryObserver.kt,AssetMapping.kt}`,
`mobile/shared/src/iosTest/kotlin/app/squirrel/shared/ios/photos/AssetMappingTest.kt`,
`mobile/iosApp/iosAppTests/PhotoKitSourceTests.swift` (test code)

`PhotoKitSource : AssetSource`, all through `platform.Photos`:
- `changes(token = null)` → `PHAsset.fetchAssetsWithOptions` (images, and videos when enabled;
  `includeHiddenAssets = false`; album filter via `PHAssetCollection` fetches). With a token →
  `PHPhotoLibrary.sharedPhotoLibrary().fetchPersistentChangesSinceToken(token, error)` and each
  change's `changeDetailsForObjectType(PHObjectTypeAsset)` inserted/updated identifiers; an expired
  token (`PHPhotosErrorPersistentChangeTokenExpired`) → full rescan. Token = `NSKeyedArchiver`
  data, base64. Deleted identifiers are ignored (backup only).
- Name and type from the primary `PHAssetResource` (`.fullSizePhoto` for an edited photo, what the
  user sees; else `.photo` / `.video`): `originalFilename`, `uniformTypeIdentifier` → MIME. The
  size is known only after export, so the source reports `size = -1` and iOS change detection uses
  `modificationDate` alone. Live Photos: the still only.
- `taken_at`: `creationDate` with `NSTimeZone.localTimeZone.secondsFromGMTForDate(date)`.
- `open(id)` (`AssetExporter`): `PHAssetResourceManager.defaultManager().writeDataForAssetResource(resource, toFile = url, options, completionHandler)`
  wrapped in `suspendCancellableCoroutine`, `networkAccessAllowed = true` (iCloud originals are
  downloaded; only on Wi-Fi when "Only on Wi-Fi"), into `Caches/export/<sha-of-id>`; reads it with
  `NSFileHandle.fileHandleForReadingFromURL` + `seekToOffset`/`readDataUpToLength`. Exports are
  deleted after a successful upload and by a size-capped sweep (2 GB).
- `PhotoAccess`: `PHPhotoLibrary.authorizationStatusForAccessLevel(PHAccessLevelReadWrite)` → Full /
  Partial (count under limited access) / None; `requestAuthorizationForAccessLevel`, and
  `presentLimitedLibraryPickerFromViewController` (the Compose host's view controller).
- `LibraryObserver`: a Kotlin class extending `NSObject` and implementing
  `PHPhotoLibraryChangeObserverProtocol`, registered while the app is active, debounced 5 s.

**Tests:** `AssetMappingTest` (pure functions: resource choice, MIME, `taken_at` formatting, token
round trip) on the K/N simulator runner. PhotoKit itself needs a host app with photo permission, so
`PhotoKitSourceTests.swift` (XCTest, test code only) grants it
(`xcrun simctl privacy booted grant photos app.squirrel.ios`), adds photos
(`xcrun simctl addmedia booted <files>`), and calls the Kotlin `PhotoKitSource`: a full scan finds
them, a persistent-change scan after adding one more returns only it, export produces the original
bytes. **Commit** — `feat(ios): read the photo library with PhotoKit from Kotlin`.

### Task 41: Background URLSession chunk transport (Kotlin/Native)

**Files:** `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/ios/transfer/{BackgroundTransport.kt,TransportDelegate.kt,TransportStore.kt}`,
`mobile/shared/src/iosTest/kotlin/app/squirrel/shared/ios/transfer/TransportStoreTest.kt`,
`mobile/iosApp/iosAppTests/BackgroundTransportTests.swift` (test code)

`BackgroundTransport : ChunkTransport`. One `NSURLSession.sessionWithConfiguration(NSURLSessionConfiguration.backgroundSessionConfigurationWithIdentifier("app.squirrel.upload"), delegate = TransportDelegate, delegateQueue = null)`;
`sessionSendsLaunchEvents = true`; `discretionary = false` for "Back up now" (true for scheduled
runs); `allowsExpensiveNetworkAccess` / `allowsConstrainedNetworkAccess = !wifiOnly`.
`put(session, index, reader)` writes chunk `index` to `Caches/chunks/<upload>-<index>` and starts
`uploadTaskWithRequest(request, fromFile = url)` (`PUT /api/uploads/{id}/chunks/{index}`,
`Authorization: Upload <upload_token>`, `Content-Length`), `taskDescription = "<upload_id>:<index>"`.
`TransportDelegate` (Kotlin `NSObject` implementing `NSURLSessionTaskDelegateProtocol` and
`NSURLSessionDelegateProtocol`; overloaded selectors annotated with `@ObjCSignatureOverride`):
`URLSession(_, task, didCompleteWithError)` records the result in `TransportStore` (SQLDelight),
deletes the chunk file and resumes the waiting coroutine if the engine is running;
`URLSessionDidFinishEventsForBackgroundURLSession` calls the completion handler that
`onBackgroundUrlSessionEvents` stored (on the main queue), after the engine scheduled the next
chunks and completed finished uploads.
**Tests:** `TransportStoreTest` (K/N): results recorded and consumed exactly once. XCTest: with a
`URLProtocol` stub (foreground session in tests, same Kotlin delegate) the chunk file is written,
headers are right, the completion is recorded and the file removed; a 401 on an upload token marks
the upload for a fresh `POST /api/uploads`. **Commit** — `feat(ios): background chunk uploads from Kotlin`.

### Task 42: Background tasks (Kotlin/Native)

**Files:** `mobile/shared/src/iosMain/kotlin/app/squirrel/shared/ios/background/{BackgroundTasks.kt,IosConditions.kt}`

- `onLaunch()` → `BGTaskScheduler.sharedScheduler.registerForTaskWithIdentifier(id, usingQueue = null) { task -> … }`
  for `app.squirrel.backup.processing` (`BGProcessingTaskRequest`: `requiresNetworkConnectivity = true`,
  `requiresExternalPower = chargingOnly`, `earliestBeginDate` +1 h) and `app.squirrel.backup.refresh`
  (`BGAppRefreshTaskRequest`, +6 h). Each handler launches `SyncEngine.runOnce(deadline)` in a
  coroutine, sets `expirationHandler` to cancel it, calls `setTaskCompletedWithSuccess`, and
  re-submits itself.
- "Back up now": on iOS 26+ `BGContinuedProcessingTaskRequest("app.squirrel.backup.now", title, subtitle)`
  with progress reported through the task's `progress` (available in the iOS 26.4 SDK Kotlin 2.4.21
  bundles; guarded by an OS version check); on 17–25 the run happens in the foreground with
  `UIApplication.beginBackgroundTaskWithExpirationHandler` for the last seconds, and the background
  session carries the queued chunks.
- `IosConditions`: `nw_path_monitor` (`platform.Network`) for "expensive"/"constrained" paths,
  `UIDevice.currentDevice.batteryState` for charging.
**Verify** in the debugger:
`e -l objc -- (void)[[BGTaskScheduler sharedScheduler] _simulateLaunchForTaskWithIdentifier:@"app.squirrel.backup.processing"]`.
**Commit** — `feat(ios): schedule backups in the background from Kotlin`.

### Task 43: iOS specifics of the shared screens

All screens are the shared Compose ones; only these iOS pieces are added, in Kotlin
(`ui/src/iosMain`): the permission flow calls `PhotoAccess`; "Allow all photos" opens
`UIApplicationOpenSettingsURLString`; "Change selection" presents the limited picker; "Open file"
uses `UIDocumentInteractionController`, share uses `UIActivityViewController` (both presented from
the Compose host view controller). Opt in to CMP's **native iOS text input** for the server,
username and password fields if the 1.12 API allows it per field (so password autofill, the iOS
selection loupe and keyboard behave natively); otherwise keep Compose's input and enable the
credential autofill prompts. Safe areas: the floating rail respects the home indicator (MASTER.md
"safe-area aware"). Dynamic Type maps to the font scale. **Check with VoiceOver** that every rail
item, tile and button reads its name and that focus order follows the layout; fix with
`Modifier.semantics` (shared with TalkBack).
**Commit** — `feat(ios): photo access, sharing and native text input`.

### Task 44: iOS end-to-end on the simulator

`iosAppTests/BackupEndToEndTests.swift` (XCTest, test code only; skipped unless `SQUIRREL_SERVER`
is set in the scheme's test environment): with `scripts/mobile-e2e-server.sh` running on the host
and three photos added with `xcrun simctl addmedia`, it calls the Kotlin `IosGraph` to sign in and
run the engine (foreground transport in tests), then asserts through the API as in Task 31. Run:
`xcodebuild test -project mobile/iosApp/iosApp.xcodeproj -scheme iosApp -destination 'platform=iOS Simulator,name=<device>'`.
**Commit** — `test(ios): back up photos from the simulator end to end`.

### Task 45: Milestone H manual pass (physical iPhone)

Needs your Apple team for signing (Open question 15). First backup of ≥ 500 photos including HEIC,
Live Photos and an edited photo; lock the phone mid-backup (uploads continue in the background
session; the next processing task completes them); "Limited Access" with 10 photos; iCloud-only
originals on Wi-Fi; revoke on the web. **No commit unless fixes.**

---

## Milestone I — CI, docs and device checklist

### Task 46: CI jobs

**Files:** `.github/workflows/ci.yml`

- `mobile-tokens` (in the existing `web` job): `pnpm gen:native-tokens && git diff --exit-code mobile/theme/src/commonMain/kotlin/app/squirrel/theme/Tokens.kt` (and the icons file).
- `mobile-android` (`ubuntu-latest`): `actions/setup-java@v4` (Temurin 21),
  `gradle/actions/setup-gradle@v4` (wrapper validation included), then
  `./gradlew :shared:jvmTest :theme:jvmTest :ui:jvmTest :androidApp:testDebugUnitTest :androidApp:lintDebug :androidApp:assembleDebug`
  in `mobile/`, preceded by `./gradlew :shared:checkApiClient` (the generated-client drift check,
  Task 17; the mobile jobs' path filter includes `openapi.json`, so a server API change runs it);
  upload the debug APK as an artifact.
- `mobile-ios` (`macos-latest`, or the newest image with Xcode ≥ the version Kotlin supports;
  select it with `sudo xcode-select -s /Applications/Xcode_<ver>.app`): Temurin 21, then
  `./gradlew :shared:iosSimulatorArm64Test` and `xcodebuild build -project mobile/iosApp/iosApp.xcodeproj -scheme iosApp -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO`.
- Path filters: the mobile jobs run when `mobile/**`, `openapi.json`, `web/src/lib/styles/tokens.css`
  or the workflow change (they are slow: ~10 min Android, ~20 min iOS).
- Emulator-based instrumented tests stay local in Phase 6 (they need KVM and a server; revisit with
  `reactivecircus/android-emulator-runner` later).
**Commit** — `ci: build and test the phone apps`.

### Task 47: Docs

- `mobile/README.md`: prerequisites (JDK 21 toolchain, Android SDK path, Xcode), commands, how to
  point the app at the dev server (emulator `10.0.2.2`, simulator `localhost`), signing for your
  own devices, where tokens are stored.
- Root `README.md`: a "Phone app" section; `docs/plans/2026-11-13-squirrel-home-drive-design.md`
  "Mobile app (KMP)" gets a **"Phase 6 as built"** paragraph like Phase 1's.
**Commit** — `docs: phone app setup and usage`.

### Task 48: "As built" section

As Phases 1–3 did: add "As built" at the top of this plan (commits per milestone, deviations,
final gate, deferred manual checks). **Commit** — `docs: record Phase 6 as built`.

---

## Testing summary

| Layer | Tool | What |
|---|---|---|
| Server unit/integration | `cargo test` + `sqlx::test` on 55432 | Token rotation and reuse, extractor, route matrix, sync check scoping and limits, uploads with `taken_at`/`device_asset_id`, capture chain step 3, sync folders, upload tokens, maintenance |
| Web | Vitest, Playwright | Device helpers; Settings › Devices list and revoke |
| Shared (KMP) | kotlin.test on `jvm()` (every task) and `iosSimulatorArm64` (from H), Ktor `MockEngine`, SQLDelight in-memory drivers, Turbine | Generated client wiring, refresh single-flight, chunk streaming under both schemes, drift check (`checkApiClient`), queue SQL, hashing vectors, uploader error branches, engine batching/pausing/deadlines, status formatting |
| UI view models | kotlin.test on `jvm()` | Sign-in, backup, settings, photos and files state machines |
| Android | Robolectric (permissions, scheduler, worker), instrumented on the AVD (MediaStore source, Keystore, end to end against the dev server) | — |
| iOS | Kotlin/Native `iosTest` on the simulator (hashing, database, mappings, transport store); XCTest as test code only where a host app is needed (the Kotlin PhotoKit source with `simctl addmedia`, the Kotlin transport with a `URLProtocol` stub, end to end) | — |
| Manual | Tasks 32, 36, 45 | Real devices, background behaviour, partial access, revocation |

## Risks of Compose Multiplatform on iOS

The UI is shared Compose on iOS by decision (key decisions, "UI toolkit"). These are the costs,
checked against JetBrains' docs on 2026-11-16, and what the plan does about each.

| Risk | Status today | Mitigation in this plan |
|---|---|---|
| **Stability** | Compose Multiplatform for iOS has been **Stable** since 1.8.0 (May 2025); this plan pins 1.12.2 (1.13 is in beta). Kotlin/Native, the layer under it, ties the app to the Xcode versions each Kotlin release supports (Open question 17). | Pin exact versions in `libs.versions.toml`; upgrade CMP and Kotlin together, deliberately; the iOS milestone comes last, so Android is unaffected by iOS toolchain lag. |
| **iOS feel: scrolling** | Compose draws its own lists (Skia via Skiko), so scroll physics, overscroll bounce and the scroll-to-top tap on the status bar are Compose's iOS imitations, not UIKit's. | 120 Hz enabled (`CADisableMinimumFrameDurationOnPhone`); the 5 000-photo timeline is checked on a real iPhone (Task 45); no custom scroll physics. |
| **iOS feel: text fields** | Compose's own text input is the default; CMP 1.11 added an **opt-in native iOS text input** and credential autofill prompts. | Sign-in fields opt in to native input where the API allows (Task 43); the app has only three text fields and no long-form editing. |
| **Accessibility (VoiceOver)** | Compose semantics are exposed to the iOS accessibility tree, but the whole UI lives in one `UIViewController`, so behaviour depends on the semantics we write; UI testing tools see less than with UIKit. | Every interactive element gets semantics (shared with TalkBack); a VoiceOver pass is part of Tasks 43 and 45; MASTER.md rules 2 and 3 (focus, contrast) apply as on the web. |
| **Binary size** | Shipping the Skia renderer and the Kotlin/Native runtime makes the app larger than an equivalent SwiftUI one (JetBrains publishes no current figure). | Measured in Task 38 (App Thinning Size Report) and recorded in "As built"; static framework, release builds with dead-code elimination; acceptable for a sideloaded personal app (Open question 16). |
| **Interop gaps** | Some Objective-C delegate protocols have overloaded selectors that clash in Kotlin; app-delegate callbacks are only delivered to a Swift/Objective-C app delegate. | `@ObjCSignatureOverride`; the two app-delegate callbacks are forwarded by `AppDelegate.swift` (key decisions, "iOS through Kotlin/Native"). |

## Platform risks (Android)

| Risk | Mitigation |
|---|---|
| Android 16 applies JobScheduler/WorkManager runtime quotas also to work started in the foreground or alongside a foreground service. | Runs are bounded (`deadline`) and resumable; the first big backup may take several runs. If it is too slow, "Back up now" moves to a **user-initiated data transfer job** (JobScheduler, API 34+), as Google recommends for user-started transfers (follow-up, not in Phase 6). |
| Android 17 (target API 37) requires the `ACCESS_LOCAL_NETWORK` runtime permission to reach a server on the LAN, and turns on certificate transparency by default. | Phase 6 targets API 36 (what Play requires). Moving to 37 means requesting `ACCESS_LOCAL_NETWORK` for LAN addresses; Tailscale addresses and publicly trusted certificates are unaffected by certificate transparency. |
| Partial photo access expires ("access to selected items expires eventually"). | The access state is re-read before each run; the backup screen shows the notice again when it shrinks. |

## Security considerations

| Threat | Mitigation in this plan |
|---|---|
| **Stolen phone or token** | Tokens revocable per device from the web (immediate: opaque tokens looked up on every request); access tokens live 15 min and only in memory; the refresh token is Keystore-encrypted / Keychain `ThisDeviceOnly` (not in backups, not migrated); `logout-all` and disabling the user revoke devices. |
| **Refresh token theft and replay** | Rotation on every refresh; retired tokens are remembered (hashed) and presenting one revokes the device and logs a warning. |
| **Privilege of phone tokens** | Bearer callers reach the data routes only; invitations, admin, account and device management stay session-only, enforced by a route matrix test that fails for any unclassified route. Upload tokens work only for one upload's chunk PUTs and status. |
| **CSRF via the new auth path** | Bearer requests are not cookie-authenticated (the cookie is ignored when `Authorization` is present); no CORS, so a web page cannot add the header cross-origin. Session requests keep the synchronizer CSRF check. |
| **Credential stuffing on device login** | Same per-IP/per-username limits, lockout and timing equalisation as the web login (shared `authenticate`); refresh has its own bucket; generic errors. |
| **Cross-user content probing** | `sync/check` and upload dedup consider only the caller's own nodes (and the device's own records); another user's content is reported missing and uploaded again, never linked. |
| **Token leakage in logs** | Tokens are never logged (only device and user ids); prefixes let secret scanners spot accidental commits; the app never logs tokens (a debug-only HTTP log filter redacts `Authorization`). |
| **Network** | HTTPS only in release builds, system CAs only, no user-installed CAs, no certificate pinning (Tailscale/Let's Encrypt rotate); clear text only in debug builds to `10.0.2.2`/`localhost`. |
| **Shared UI on iOS** | Compose renders inside one view controller with no web view and no remote content; the password field is a secure entry (native iOS input where CMP allows it, Task 43, so iOS treats it as a password field for autofill and the keyboard); no token or password is ever kept in Compose state beyond the sign-in call. |
| **Malicious server** | The app sends only what the user chose to back up to the URL they typed; responses are parsed with bounded sizes (Ktor `ignoreUnknownKeys`, no reflection), image decoding by the platform. |
| **Location privacy** | GPS stays in the bytes only when the user grants `ACCESS_MEDIA_LOCATION` (it is their own server); nothing is sent anywhere else; no analytics or crash reporting SDK. |
| **Local data at rest** | The queue database holds names, hashes and ids (no photo bytes beyond transient exports/chunks in the cache, deleted after upload); Android `allowBackup=false`; iOS data protection default (complete until first unlock) suits background work. |
| **Resource abuse from a phone** | Sync check batches ≤ 500 with a per-device rate limit and 256 KiB bodies; uploads keep the existing per-user creation rate, open-session cap, disk-space share and quota checks. |

## Not in Phase 6

- Video processing, posters and playback in the app — **Phase 4** (videos are backed up now and
  processed by its backfill). ML search and faces — **Phase 5**.
- Joining with an invitation inside the app (Open question 5), QR-code pairing.
- Editing on the phone: delete, move, rename, album editing, selection, uploads of arbitrary files
  from the phone (share-sheet "Save to Squirrel"), offline copies.
- Live Photo motion parts, iOS edit history (only the current rendition is backed up), RAW+JPEG
  pairs as one asset, burst sets, Android "Motion photo" extraction.
- Custom CAs, client certificates, certificate pinning; store distribution; push notifications;
  widgets; a Wear/watch app; tablets beyond what the phone layout gives.
- Instrumented Android tests and iOS UI tests in CI (local only for now).
- Targeting Android API 37 (`ACCESS_LOCAL_NETWORK`), user-initiated data transfer jobs.

## Done when

- `cargo test --workspace`, `pnpm check`, `pnpm test:unit`, `SQUIRREL_E2E_PORT=8199 pnpm test:e2e`,
  `(cd mobile && ./gradlew :shared:jvmTest :theme:jvmTest :ui:jvmTest :androidApp:testDebugUnitTest :androidApp:lintDebug :shared:iosSimulatorArm64Test)`
  and the iOS `xcodebuild test` all green; `openapi.json`, `schema.d.ts` and `Tokens.kt` regenerated
  with no diff.
- The Android end-to-end test (Task 31) and the iOS one (Task 44) pass against a local server.
- Manual: Tasks 32, 36 and 45 done on real phones (≥ 500 photos each, background, partial access,
  revocation), with any deferred item listed in "As built".
- Settings › Devices shows both phones; revoking one signs it out within its next request.
- Then add an **"As built"** section at the top of this plan, as Phases 1–3 did.

## As built: Milestones A–C (2026-11-16)

Server and web parts of Phase 6 (Tasks 1–13) are done; Milestones D–I are not started.

- **Contract:** `openapi.json` declares `session_cookie`, `csrf_token` and `device_bearer`, and every
  operation lists the ones it accepts (`[{}]` for public routes). `tests/route_auth_matrix.rs` checks
  them against the router (anonymous, bearer only, session + CSRF) and fails on an unlisted route.
  New schemas: `DeviceDto`, `DevicePlatform`, `TokenPair`, `AccountDto`, `DeviceLoginRequest`/`Response`,
  `RefreshRequest`/`Response`, `CurrentDeviceResponse`, `RenameDeviceRequest`, `SyncCheckRequest`/`Item`/
  `Response`, `SyncFolderRequest`/`Response`; device and sync operations have explicit `operation_id`s.
- **Deviations from the plan text:**
  - `device_retired_refresh` gained `retired_at`; maintenance prunes retired tokens older than the
    refresh lifetime (otherwise one row per refresh would accumulate forever). Maintenance also
    deletes devices revoked or expired for 30 days (`device_retention`) and expired access tokens.
  - `upload_sessions_device_asset_shape` is `device_id IS NULL OR device_asset_id IS NOT NULL`, not an
    equality: `device_id … ON DELETE SET NULL` would otherwise make deleting a device fail.
  - `Principal::Session` carries no session record (nothing reads it); `DeviceCaller` replaces
    `Caller::require_device`. A revoked device's refresh answers `Unknown` (401).
  - The sync check's per-device limiter runs in the handler (`ApiConfig::sync_check_rate`, 2/s, burst 20);
    `POST /api/sync/folder` serialises on the owner lock and reuses a live folder of the device's name;
    a device name that cannot name a folder (`/` → `-`, `.`/`..` → "Phone") is adapted.
  - `taken_at` accepts `Z` (offset 0); offsets beyond ±18 h are refused.
  - The web's Settings › Devices is reached from the account menu ("Devices"); the e2e project
    `devices` covers listing, revoking (desktop) and the phone actions sheet.

### Security review fixes (2026-11-17)

- **Migrations:** `20261116000003_refresh_grace.sql` and `20261116000004_deleted_hashes.sql` are
  taken, so Milestone H's upload tokens become `20261116000005_upload_tokens.sql`.
- **Other `Authorization` schemes** (`Basic` from a proxy) fall back to the session cookie, with
  CSRF still checked. A `Bearer` header never does: if it is malformed, the answer is 401.
- **Refresh grace:**
  - Presenting the previous refresh token within 30 s of its rotation (`devices.rotated_at`)
    adds an access token and returns the same current refresh token. The device does not rotate
    again, so two refreshes racing, or a retry after a lost response, end up holding one token.
  - The server can return that token without storing it in clear because it is derived:
    `sqr_` + SHA-256(context, presented token, `rotation_salt`).
  - After 30 s, the previous token counts as reuse and revokes the device, like a retired one.
  - The app must still make its refreshes single-flight.
- **Never re-uploading deleted content:**
  - Deleting forever, emptying the trash and the 30-day trash purge write a tombstone
    (`deleted_hashes`, per owner and SHA-256) for every file whose content a device of that owner
    had recorded in `device_assets`.
  - The sync check reports tombstoned hashes as present and records them for the asking device.
    This survives sign-out, a reinstall, a new device id and the deletion of old devices.
  - Maintenance never deletes tombstones.
  - Trashed (not purged) content was already present: the library lookup includes the trash.
- **Session-only deletion:** `DELETE /api/nodes/{id}` (trash), `DELETE /api/trash/{id}` and
  `DELETE /api/trash` are session-only now. A phone token reads and backs up but cannot destroy
  anything. The route matrix accepts `METHOD /path` entries for this.
- **Logout everywhere** deletes sessions and revokes devices in one transaction.
- **Sync check recording:** the sync check writes its records with one statement, in
  `device_asset_id` order, so overlapping checks cannot deadlock.
- **Device deleted mid-upload:** recording a device asset locks the device row
  (`FOR KEY SHARE`), so a device deleted in the meantime records nothing instead of failing the
  completion.
- **Same-named phones:**
  - A phone without a live backup folder takes the first of `<name>`, `<name> (2)`, … under
    `Photos` that is free, or a folder no other signed-in device of the owner uses.
  - So two phones signed in at once with the same name get separate folders, and a phone that
    signed out and signs in again gets its folder back.
  - A reinstall without signing out first gets `<name> (2)` until the old device expires.
- **Redacted Debug:** `LoginRequest`, `SetupRequest`, `AccountRequest`, `DeviceLoginRequest`,
  `RefreshRequest` and `TokenPair` show `<redacted>` instead of secrets in `Debug` output.

### Task 37 as built: per-upload tokens (2026-11-17)

- **Migration:** `20261116000005_upload_tokens.sql` adds `upload_sessions.upload_token_hash`
  (SHA-256, unique) and `upload_token_device_id`, the device the token was issued to.
- **Issuing:**
  - A bearer `POST /api/uploads` that opens or resumes a session returns `upload_token` (`squ_…`).
    Each open or resume issues a new one, and the old one stops working.
  - Browsers and deduplicated (200) uploads get no token.
- **Accepting:**
  - `UploadCaller` takes a session, a device bearer, or `Authorization: Upload squ_…`, on
    `PUT …/chunks/{n}` and `GET /api/uploads/{id}` only.
  - The token is looked up by its hash. It works only while the session is unexpired and its
    device is signed in (not revoked, refresh lifetime not over) and the owner is not disabled.
- **Refusals:**
  - Another upload's id gets 404.
  - A dead token (upload completed, aborted or expired, device revoked, unknown or malformed)
    gets 401, not 404: once its session row is gone the server cannot tell it from a wrong token,
    and 401 tells the client to drop it.
  - On every other route, including complete and abort, the `Upload` scheme is not a bearer, so
    only a session cookie could apply: 401 without one.
- **Contract:**
  - OpenAPI's `http` security type only accepts registered schemes, so `upload_token` is
    declared as an `apiKey` in the `Authorization` header, whose value is `Upload squ_…`.
  - The two operations list it between `device_bearer` and the session pair.
  - The route matrix gains an upload-token column: both its tests check it.
