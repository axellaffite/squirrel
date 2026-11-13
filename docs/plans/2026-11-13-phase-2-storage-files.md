# Phase 2 — Storage, Uploads, File Browser & Trash Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Each user gets a private drive (`Photos/`, `Documents/`, `Files/`) where they can upload any file through a resumable, deduplicating chunked protocol, browse folders, create/rename/move/delete, restore from a 30-day trash, download or preview files (PDF, images, video, text) in the browser, and see their storage usage against an optional quota.

**Architecture:** A new `squirrel-storage` crate owns the content-addressed blob store on disk (`<data_dir>/blobs/ab/cd/<sha256>`, uploads assembled in place under `<data_dir>/tmp/uploads/<id>`, verified, then atomically renamed). Postgres holds the logical tree (`nodes`), the blob catalogue (`blobs`, with `ref_count` maintained by a trigger) and upload sessions (`upload_sessions` + `upload_chunks`). `squirrel-api` orchestrates both: DB transactions serialize per-user tree changes and quota checks with `SELECT … FOR UPDATE` on the user row, and per-blob store/GC with Postgres advisory locks. Chunk uploads, completion and downloads live in a new `transfer_routes` group (large body limit, no 30 s timeout). The SPA hashes files in a Web Worker (hash-wasm), uploads missing chunks with retries, and renders previews from the inline content URL.

**Tech Stack:** Phase 1 stack, plus: `infer` 0.22 (content sniffing), `mime_guess` 2, `http-range-header` 1.1 (Range parsing), `percent-encoding` 2 (RFC 5987 filenames), `tokio-util` 0.7 `io` (`ReaderStream`), `rustix` 1 `fs` (`statvfs`), `tempfile` 3 (tests); `sha2` 0.10 is already in the workspace; web: `hash-wasm` 4.

**Design reference:** `docs/plans/2026-11-13-squirrel-home-drive-design.md` (sections "Storage and sync", "Errors and observability", "Testing", "Delivery phases").
**Phase 1 reference:** `docs/plans/2026-11-13-phase-1-foundation-auth.md` — read its "As built" section; the code is the source of truth.

## Conventions / environment

- Run commands from the repo root unless stated. Never `cd` outside the worktree; never `git stash` (use a WIP commit).
- **Dev Postgres is `localhost:55432` only** (`deploy/docker-compose.dev.yml`, `DATABASE_URL` in `.cargo/config.toml`). **Never use port 5433** — it is a real database proxy on this machine.
- Node/pnpm commands are prefixed with `source ~/.nvm/nvm.sh && nvm use 22 >/dev/null &&` and run in `web/`.
- After each Rust task, before committing:
  `cargo fmt --all && rtk proxy cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- E2E: `cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && SQUIRREL_E2E_PORT=8199 pnpm test:e2e` (8099 is taken on this machine).
- Tests that touch the filesystem use `tempfile::tempdir()` — never a fixed path, never the repo.
- `sqlx` runtime queries (`query_as::<_, T>`) as in Phase 1; no compile-time macros.
- The initial migration is **frozen**. All schema changes go in `crates/squirrel-db/migrations/20261113000002_files.sql`; once committed that file is frozen too (CI refuses edits).
- Every API change: regenerate the contract in the same task, or `tests/openapi.rs` fails:
  `cargo run -q -p squirrel -- openapi > openapi.json && (cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm gen:api)`
- Commit messages follow the repo pattern `<type>(<scope>): <description>` (scopes used so far: `core`, `db`, `api`, `web`, …), short body optional, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Keep files under ~400 lines; split modules rather than growing one.
- Verified while writing this plan: the code of Tasks 1–4 and 18 was compiled and its tests run (Rust 1.94) against the real crates (`infer` 0.22, `http-range-header` 1.1, `percent-encoding` 2, `rustix` 1, `tempfile` 3); the migration SQL was applied to Postgres 17 (pgvector image) and its constraints, trigger and keyset query plan checked. Other code follows APIs checked in the registry sources (axum 0.8.9, axum-test 17.3, sqlx 0.8.6, utoipa 5.5, tokio-util 0.7.20) but has not been compiled: treat compiler feedback as authoritative.

## Key decisions (read before starting)

| Topic | Decision | Why |
|---|---|---|
| Name conflicts on upload completion | **Auto-rename** to `name (1).ext`, `name (2).ext`… | Sync clients (Phase 6) and drag-and-drop must never fail at the last step; the response carries the final name. Explicit operations (create folder, rename, move) return **409 `name_conflict`** instead, because the user chose the name. Restore also auto-renames. |
| Roots | `Photos`, `Documents`, `Files` are the only nodes with `parent_id IS NULL`; they cannot be renamed, moved or trashed (422), and nothing else can be created at the top level. | Phase 3 relies on `Photos/`; keeps the root stable for sync. |
| Unique names | Partial unique index `(owner_id, parent_id, name) NULLS NOT DISTINCT WHERE deleted_at IS NULL` (Postgres ≥ 15). Case-sensitive, NFC-normalized. | One index covers roots (NULL parent) and children; trashed nodes never block a name. |
| Trash | Deleting sets `deleted_at` and `trash_root = <deleted node id>` on the node and its whole live subtree in one statement. The trash lists nodes where `trash_root = id`. Restore puts the batch back; if the original parent is no longer live it goes to `Files/`; name clashes auto-rename. | One batch id makes restore and purge exact without recursive "is my ancestor deleted" checks. |
| Quota | `users.quota_bytes` (NULL = unlimited) vs `SUM(size)` of the user's **live** file nodes (trash excluded, as the design says). Checked at upload creation (also counting the user's open upload sessions, so parallel sessions cannot overcommit), at completion, on dedup links and on restore, each time under `SELECT … FOR UPDATE` of the user row. 507 `quota_exceeded`. | Completion is authoritative; the creation check fails fast before bytes are sent. |
| Disk space | Upload creation refuses (507 `storage_full`) when `size > available - min_free_bytes` (default 1 GiB). `ENOSPC`/`EDQUOT` anywhere maps to 507 `storage_full`. | Never fill the disk the database lives on. |
| Ref counts | `blobs.ref_count` is maintained by an `AFTER INSERT/DELETE/UPDATE OF blob_sha256` trigger on `nodes`; `zero_since` is set when it reaches 0. | Cascading deletes (purging a folder) stay correct without application bookkeeping. |
| Blob/GC race | Completion holds `pg_advisory_xact_lock(hash key)` from before the rename until commit; GC holds the session-level `pg_advisory_lock` across "delete row → unlink file", and only deletes blobs at 0 refs for longer than the grace period (24 h). | Without it, GC could unlink a file a concurrent upload has just "deduplicated" against. |
| Dedup probe | `POST /api/uploads` answers `200 {deduplicated: true, node}` only if the hash is referenced by a node **owned by the caller** (live or in their trash). Otherwise `201` — even if another user has the blob; the bytes are then uploaded, verified and the existing blob reused silently at completion. | Never reveals other users' content. |
| Resume | `POST /api/uploads` returns the caller's existing open session for the same `(parent, name, sha256, size)` with its `received_chunks`. | Resume after a reload or crash without client-side storage. |
| Chunk storage | Per-session sparse file, chunk `n` written at `n * chunk_size` with `pwrite` (`FileExt::write_all_at`) + `fdatasync` in `spawn_blocking`; no reassembly. Completion streams the file through SHA-256. | Any order, idempotent, O(1) extra disk. |
| Chunk size | 8 MiB (`ApiConfig::upload_chunk_size`; tests use 1 KiB). Chunk body limit = chunk size + 64 KiB. | Design value. |
| Route groups | JSON upload endpoints (create/status/abort) in `json_routes`. `PUT …/chunks/{n}` in `transfer_routes` with the chunk body limit and a 5-minute per-request timeout (slow-loris bound). `POST …/complete` and `GET /api/files/{id}/content` in `transfer_routes` without timeout (hashing a 10 GB file or streaming a film takes longer than 30 s). All still require session + CSRF. | Matches the "reserved transfer_routes" comment in `squirrel-api/src/lib.rs`. |
| Ranges | `http-range-header` parses; only a single range is honoured (multi-range → full 200, allowed by RFC 9110). 416 with `Content-Range: bytes */size`. | Enough for video seeking and PDF viewers. |
| Download safety | Type comes from the stored, server-sniffed MIME. Inline only for an allowlist (PDF, raster images, audio/video, `text/plain`); active types (HTML, SVG, XML, JS…) are served as `application/octet-stream` + attachment. Every content response: `nosniff`, `Cross-Origin-Resource-Policy: same-origin`, `X-Frame-Options: SAMEORIGIN`, CSP `sandbox; default-src 'none'; …; frame-ancestors 'self'` (PDFs get a non-sandboxed CSP, see Task 25). | Content must never run as same-origin HTML/JS. |
| Caching | Content responses: `ETag: "<sha256>"`, `Cache-Control: private, no-cache` (revalidate every time; 304 on `If-None-Match`). The `/api` no-store middleware keeps a handler's own `Cache-Control`. | Revalidation goes through auth, so logging out still protects content. |
| Hashing in the browser | `hash-wasm` incremental SHA-256 in a module Web Worker, 4 MiB slices. CSP gains `'wasm-unsafe-eval'` in `script-src`. | SubtleCrypto cannot hash incrementally; pure-JS SHA-256 is ~3× slower. |
| Quota administration | `squirrel admin set-quota <username> <bytes|unlimited>` CLI subcommand (the design's `admin` subcommand). No quota UI in Phase 2. | Needed by the E2E test and operators; admin user management UI is a later concern. |

## API added in Phase 2

| Method & path | Group | Purpose |
|---|---|---|
| `GET /api/nodes?parent_id&cursor&limit` | json | List the roots (no `parent_id`) or a folder's live children, folders first, keyset-paginated by name |
| `GET /api/nodes/{id}` | json | A live node and its ancestor path (breadcrumb) |
| `POST /api/folders` | json | Create a folder `{parent_id, name}` → 201 / 409 |
| `PATCH /api/nodes/{id}` | json | Rename and/or move `{name?, parent_id?}` → 200 / 409 / 422 (cycle, root) |
| `DELETE /api/nodes/{id}` | json | Move to trash (subtree) → 204 |
| `GET /api/trash?cursor&limit` | json | Trash entries, newest first |
| `POST /api/trash/{id}/restore` | json | Restore → 200 node (maybe renamed/relocated) / 507 |
| `DELETE /api/trash/{id}` | json | Delete forever → 204 |
| `DELETE /api/trash` | json | Empty trash → 204 |
| `GET /api/storage` | json | `{used_bytes, trash_bytes, quota_bytes}` |
| `POST /api/uploads` | json | Start/resume an upload, or dedup → 200 / 201 / 507 |
| `GET /api/uploads/{id}` | json | Session status with `received_chunks` |
| `DELETE /api/uploads/{id}` | json | Abort → 204 |
| `PUT /api/uploads/{id}/chunks/{n}` | transfer (chunk) | Raw chunk bytes → 204 |
| `POST /api/uploads/{id}/complete` | transfer | Verify hash, store blob, create node → 201 / 409 / 422 / 507 |
| `GET /api/files/{id}/content[?download=true]` | transfer | Bytes with Range/ETag/Disposition/CSP |

New error codes (envelope `{error: {code, message}}`): `name_conflict` (409), `upload_incomplete` (409), `quota_exceeded` (507), `storage_full` (507), `hash_mismatch` (422), `range_not_satisfiable` (416).

## Milestones

- **A — Core types & storage crate** (Tasks 1–4)
- **B — Schema & repositories** (Tasks 5–10)
- **C — Upload API** (Tasks 11–15)
- **D — Tree, trash, downloads & storage API** (Tasks 16–20)
- **E — Web UI** (Tasks 21–26)
- **F — E2E, CI & docs** (Tasks 27–29)

---
## Milestone A — Core types & storage crate

### Task 1: `NodeName` domain type

**Files:**
- Create: `crates/squirrel-core/src/node_name.rs`
- Modify: `crates/squirrel-core/src/lib.rs`

**Step 1: Write the failing tests** — create `node_name.rs` containing only this test module (plus `use super::*;` inside it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_normalizes_to_nfc() {
        assert_eq!(NodeName::parse("  e\u{301}t\u{e9}.txt ").unwrap().as_str(), "\u{e9}t\u{e9}.txt");
    }

    #[test]
    fn rejects_empty_dot_and_dotdot() {
        assert_eq!(NodeName::parse("   "), Err(NodeNameError::Empty));
        assert_eq!(NodeName::parse("."), Err(NodeNameError::Reserved));
        assert_eq!(NodeName::parse(".."), Err(NodeNameError::Reserved));
        assert!(NodeName::parse("...").is_ok());
        assert!(NodeName::parse(".bashrc").is_ok());
    }

    #[test]
    fn rejects_slashes_and_control_characters() {
        assert_eq!(NodeName::parse("a/b"), Err(NodeNameError::Slash));
        assert_eq!(NodeName::parse("a\0b"), Err(NodeNameError::Control));
        assert_eq!(NodeName::parse("a\nb"), Err(NodeNameError::Control));
        assert_eq!(NodeName::parse("invoice\u{202E}fdp.exe"), Err(NodeNameError::Control));
    }

    #[test]
    fn allows_backslashes_emoji_and_zero_width_joiner() {
        assert!(NodeName::parse("a\\b").is_ok());
        assert!(NodeName::parse("family \u{1F468}\u{200D}\u{1F469}.jpg").is_ok());
    }

    #[test]
    fn limits_length_in_bytes_after_normalization() {
        assert!(NodeName::parse(&"a".repeat(255)).is_ok());
        assert_eq!(NodeName::parse(&"a".repeat(256)), Err(NodeNameError::TooLong));
        assert_eq!(NodeName::parse(&"\u{e9}".repeat(128)), Err(NodeNameError::TooLong));
    }

    #[test]
    fn variants_insert_a_counter_before_the_extension() {
        let name = NodeName::parse("report.pdf").unwrap();
        assert_eq!(name.variant(0).as_str(), "report.pdf");
        assert_eq!(name.variant(1).as_str(), "report (1).pdf");
        assert_eq!(NodeName::parse("archive.tar.gz").unwrap().variant(2).as_str(), "archive.tar (2).gz");
        assert_eq!(NodeName::parse("README").unwrap().variant(1).as_str(), "README (1)");
        assert_eq!(NodeName::parse(".bashrc").unwrap().variant(1).as_str(), ".bashrc (1)");
    }

    #[test]
    fn variants_stay_within_the_byte_limit() {
        let long = NodeName::parse(&format!("{}.txt", "\u{e9}".repeat(125))).unwrap();
        let variant = long.variant(12);
        assert!(variant.as_str().len() <= NodeName::MAX_BYTES);
        assert!(variant.as_str().ends_with(" (12).txt"));
    }
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-core node_name` → FAIL: `cannot find type NodeName`.

**Step 3: Implement** — put this above the test module:

```rust
use std::fmt;

use unicode_normalization::UnicodeNormalization;

/// A validated file or folder name: trimmed, NFC-normalized, 1–255 UTF-8 bytes,
/// no `/`, no control or bidirectional-override characters, not `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeNameError {
    #[error("name must not be empty")]
    Empty,
    #[error("name must be at most 255 bytes")]
    TooLong,
    #[error("name must not be \".\" or \"..\"")]
    Reserved,
    #[error("name must not contain \"/\"")]
    Slash,
    #[error("name must not contain control or text-direction characters")]
    Control,
}

impl NodeName {
    pub const MAX_BYTES: usize = 255;

    pub fn parse(raw: &str) -> Result<Self, NodeNameError> {
        let name: String = raw.trim().nfc().collect();
        if name.is_empty() {
            return Err(NodeNameError::Empty);
        }
        if name.len() > Self::MAX_BYTES {
            return Err(NodeNameError::TooLong);
        }
        if name == "." || name == ".." {
            return Err(NodeNameError::Reserved);
        }
        if name.contains('/') {
            return Err(NodeNameError::Slash);
        }
        if name.chars().any(|c| c.is_control() || is_bidi_control(c)) {
            return Err(NodeNameError::Control);
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `n`-th alternative for a name that is taken: `report (1).pdf`,
    /// `report (2).pdf`… The suffix goes before the last extension; dotfiles and
    /// names without an extension get it at the end. `n == 0` is the name itself.
    /// The stem is shortened (on a character boundary) to stay within 255 bytes.
    pub fn variant(&self, n: u32) -> Self {
        if n == 0 {
            return self.clone();
        }
        let (stem, ext) = match self.0.rfind('.') {
            Some(dot) if dot > 0 => self.0.split_at(dot),
            _ => (self.0.as_str(), ""),
        };
        let suffix = format!(" ({n}){ext}");
        let budget = Self::MAX_BYTES.saturating_sub(suffix.len());
        let mut cut = stem.len().min(budget);
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        Self(format!("{}{suffix}", &stem[..cut]))
    }
}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Bidirectional controls that can make `invoice\u{202E}fdp.exe` render as `invoice.exe.pdf`.
/// Other format characters (e.g. the zero-width joiner in emoji) stay allowed.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

```

`lib.rs` — add the module and export:
```rust
mod node_name;
pub use node_name::{NodeName, NodeNameError};
```

Notes for the implementer:
- Trimming uses `str::trim` (Unicode whitespace), like `DisplayName`.
- Unlike `DisplayName`, the zero-width joiner and other format characters stay allowed (emoji file names); only bidi controls are refused (`invoice‮fdp.exe` spoofing).
- Backslashes are allowed (valid on Linux, used by some macOS names).

**Step 4: Run tests** — `cargo test -p squirrel-core node_name` → PASS (7 tests).

**Step 5: Commit**
```bash
git add crates/squirrel-core
git commit -m "feat(core): add validated NodeName with conflict variants" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `ContentHash`, `UploadPlan`, `NodeKind`

**Files:**
- Create: `crates/squirrel-core/src/content_hash.rs`, `crates/squirrel-core/src/upload_plan.rs`, `crates/squirrel-core/src/node_kind.rs`
- Modify: `crates/squirrel-core/src/lib.rs`

**Step 1: Write the failing tests** (one test module per file, as in Task 1)

`content_hash.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn accepts_64_lowercase_hex() {
        assert_eq!(ContentHash::parse(EMPTY).unwrap().as_str(), EMPTY);
    }

    #[test]
    fn rejects_uppercase_short_and_path_like_input() {
        assert!(ContentHash::parse(&EMPTY.to_uppercase()).is_err());
        assert!(ContentHash::parse(&EMPTY[..63]).is_err());
        assert!(ContentHash::parse(&format!("../{}", &EMPTY[3..])).is_err());
        assert!(ContentHash::parse(&format!("{EMPTY}0")).is_err());
    }

    #[test]
    fn formats_a_digest_as_lowercase_hex() {
        let mut digest = [0u8; 32];
        digest[0] = 0xab;
        digest[31] = 0x0f;
        let hash = ContentHash::from_digest(digest);
        assert!(hash.as_str().starts_with("ab00"));
        assert!(hash.as_str().ends_with("0f"));
        assert_eq!(ContentHash::parse(hash.as_str()), Ok(hash));
    }

    #[test]
    fn shards_by_the_first_two_bytes() {
        assert_eq!(ContentHash::parse(EMPTY).unwrap().shard(), ("e3", "b0"));
    }

    #[test]
    fn lock_key_is_stable_and_distinct() {
        let a = ContentHash::parse(EMPTY).unwrap();
        let b = ContentHash::parse(&"0".repeat(64)).unwrap();
        assert_eq!(a.lock_key(), a.clone().lock_key());
        assert_ne!(a.lock_key(), b.lock_key());
    }
}
```

`upload_plan.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_into_full_chunks_and_a_short_tail() {
        let plan = UploadPlan::new(10, 4);
        assert_eq!(plan.chunk_count(), 3);
        assert_eq!(plan.offset(2), Some(8));
        assert_eq!(plan.chunk_len(0), Some(4));
        assert_eq!(plan.chunk_len(2), Some(2));
        assert_eq!(plan.chunk_len(3), None);
    }

    #[test]
    fn exact_multiple_has_no_short_tail() {
        let plan = UploadPlan::new(8, 4);
        assert_eq!(plan.chunk_count(), 2);
        assert_eq!(plan.chunk_len(1), Some(4));
    }

    #[test]
    fn empty_file_has_no_chunks() {
        let plan = UploadPlan::new(0, 4);
        assert_eq!(plan.chunk_count(), 0);
        assert_eq!(plan.offset(0), None);
    }

    #[test]
    fn huge_files_do_not_overflow() {
        let plan = UploadPlan::new(u64::MAX, 8 << 20);
        let last = plan.chunk_count() - 1;
        assert_eq!(plan.offset(last).unwrap() + plan.chunk_len(last).unwrap(), u64::MAX);
    }
}
```

`node_kind.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for kind in [NodeKind::Folder, NodeKind::File] {
            assert_eq!(kind.as_str().parse::<NodeKind>().unwrap(), kind);
        }
        assert!("symlink".parse::<NodeKind>().is_err());
    }
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-core` → FAIL (unknown types).

**Step 3: Implement**

`content_hash.rs`:
```rust
use std::fmt;

/// A SHA-256 digest as 64 lowercase hex characters: the blob identifier.
/// Parsing is strict so a hash can be used safely as a path component.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("sha256 must be 64 lowercase hexadecimal characters")]
pub struct ContentHashError;

impl ContentHash {
    pub const HEX_LEN: usize = 64;

    pub fn parse(raw: &str) -> Result<Self, ContentHashError> {
        let valid = raw.len() == Self::HEX_LEN
            && raw.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid { Ok(Self(raw.to_owned())) } else { Err(ContentHashError) }
    }

    pub fn from_digest(digest: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let hex = digest
            .iter()
            .flat_map(|b| [HEX[usize::from(b >> 4)] as char, HEX[usize::from(b & 0xf)] as char])
            .collect();
        Self(hex)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two levels of fan-out directories: `ab/cd` for `abcd…`.
    pub fn shard(&self) -> (&str, &str) {
        (&self.0[0..2], &self.0[2..4])
    }

    /// A stable 64-bit key (the first 8 bytes) for Postgres advisory locks.
    pub fn lock_key(&self) -> i64 {
        let prefix = u64::from_str_radix(&self.0[..16], 16).expect("validated hex");
        i64::from_ne_bytes(prefix.to_ne_bytes())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

```

`upload_plan.rs`:
```rust
/// How a file of `size` bytes is split into fixed-size chunks (the last one may
/// be shorter). An empty file has no chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadPlan {
    size: u64,
    chunk_size: u64,
}

impl UploadPlan {
    /// `chunk_size` must be > 0.
    pub fn new(size: u64, chunk_size: u64) -> Self {
        assert!(chunk_size > 0, "chunk size must be positive");
        Self { size, chunk_size }
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn chunk_count(&self) -> u64 {
        self.size.div_ceil(self.chunk_size)
    }

    /// Byte offset of chunk `index`, or `None` past the last chunk.
    pub fn offset(&self, index: u64) -> Option<u64> {
        (index < self.chunk_count()).then(|| index * self.chunk_size)
    }

    /// Exact length chunk `index` must have, or `None` past the last chunk.
    pub fn chunk_len(&self, index: u64) -> Option<u64> {
        self.offset(index)
            .map(|offset| (self.size - offset).min(self.chunk_size))
    }
}

```

`node_kind.rs` (mirrors `role.rs`):
```rust
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Folder,
    File,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown node kind: {0}")]
pub struct UnknownNodeKind(pub String);

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Folder => "folder",
            NodeKind::File => "file",
        }
    }
}

impl FromStr for NodeKind {
    type Err = UnknownNodeKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "folder" => Ok(NodeKind::Folder),
            "file" => Ok(NodeKind::File),
            other => Err(UnknownNodeKind(other.to_owned())),
        }
    }
}
```

`lib.rs` — final shape:
```rust
mod content_hash;
mod display_name;
mod node_kind;
mod node_name;
mod password;
mod role;
mod upload_plan;
mod username;

pub use content_hash::{ContentHash, ContentHashError};
pub use display_name::{DisplayName, DisplayNameError};
pub use node_kind::{NodeKind, UnknownNodeKind};
pub use node_name::{NodeName, NodeNameError};
pub use password::{Password, PasswordError, normalize_password};
pub use role::{Role, UnknownRole};
pub use upload_plan::UploadPlan;
pub use username::{Username, UsernameError};
```

**Step 4: Run tests** — `cargo test -p squirrel-core` → PASS (all previous tests + 10 new).

**Step 5: Commit**
```bash
git add crates/squirrel-core
git commit -m "feat(core): add ContentHash, UploadPlan and NodeKind" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `squirrel-storage` crate — `BlobStore`

**Files:**
- Modify: `Cargo.toml` (workspace dependencies + member path dep)
- Create: `crates/squirrel-storage/Cargo.toml`, `crates/squirrel-storage/src/lib.rs`, `src/error.rs`, `src/fsutil.rs`, `src/store.rs`

**Step 1: Dependencies**

Root `Cargo.toml`, `[workspace.dependencies]` (keep alphabetical order):
```toml
http-range-header = "1.1"
infer = "0.22"
mime_guess = "2"
percent-encoding = "2"
rustix = { version = "1", features = ["fs"] }
tempfile = "3"
tokio-util = { version = "0.7", features = ["io"] }
squirrel-storage = { path = "crates/squirrel-storage" }
```
(`http-range-header`, `percent-encoding`, `tokio-util` are used by `squirrel-api` from Task 11 on; adding them now keeps the manifest change in one place. `base64`, `sha2` and `uuid` are already workspace dependencies.)

`crates/squirrel-storage/Cargo.toml`:
```toml
[package]
name = "squirrel-storage"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
infer.workspace = true
mime_guess.workspace = true
rustix.workspace = true
sha2.workspace = true
squirrel-core.workspace = true
thiserror.workspace = true
uuid.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

**Step 2: Write the failing tests** — `store.rs` with only its test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn store() -> (tempfile::TempDir, BlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_the_layout() {
        let (dir, _store) = store();
        for sub in ["blobs", "tmp", "tmp/uploads"] {
            assert!(dir.path().join(sub).is_dir(), "{sub}");
        }
    }

    #[test]
    fn put_reader_stores_content_under_its_hash() {
        let (_dir, store) = store();
        let (hash, size) = store.put_reader(&b"hello"[..]).unwrap();
        assert_eq!(hash.as_str(), HELLO);
        assert_eq!(size, 5);
        let path = store.blob_path(&hash);
        assert!(path.ends_with(format!("2c/f2/{HELLO}")));
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn storing_the_same_content_twice_is_idempotent() {
        let (dir, store) = store();
        let first = store.put_reader(&b"hello"[..]).unwrap();
        let second = store.put_reader(&b"hello"[..]).unwrap();
        assert_eq!(first, second);
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("tmp"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "uploads")
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind");
    }

    #[test]
    fn open_blob_reports_missing_blobs() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HELLO).unwrap();
        assert!(matches!(store.open_blob(&hash), Err(StorageError::NotFound)));
        store.put_reader(&b"hello"[..]).unwrap();
        let (mut file, size) = store.open_blob(&hash).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!((content.as_str(), size), ("hello", 5));
    }

    #[test]
    fn remove_blob_is_idempotent() {
        let (_dir, store) = store();
        let (hash, _) = store.put_reader(&b"hello"[..]).unwrap();
        store.remove_blob(&hash).unwrap();
        store.remove_blob(&hash).unwrap();
        assert!(!store.contains(&hash));
    }

    #[test]
    fn reports_available_space() {
        let (_dir, store) = store();
        assert!(store.available_space().unwrap() > 0);
    }

    #[test]
    fn no_space_errors_are_recognised() {
        let err: StorageError = io::Error::from(io::ErrorKind::StorageFull).into();
        assert!(matches!(err, StorageError::NoSpace));
    }
}
```
`lib.rs` for now:
```rust
//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

mod error;
mod fsutil;
mod store;

pub use error::{Result, StorageError};
pub use store::{Adopted, BlobStore};
```

**Step 3: Run to verify failure** — `cargo test -p squirrel-storage` → FAIL (missing items).

**Step 4: Implement**

`error.rs`:
```rust
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// ENOSPC or EDQUOT: the disk (or the filesystem quota) is full.
    #[error("not enough disk space")]
    NoSpace,
    #[error("blob not found")]
    NotFound,
    #[error("{0} must be on the same filesystem as the blobs directory")]
    CrossDevice(String),
    #[error(transparent)]
    Io(io::Error),
}

impl From<io::Error> for StorageError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Self::NoSpace,
            _ => Self::Io(err),
        }
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
```

`fsutil.rs`:
```rust
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// Makes a rename or create in `dir` durable.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

pub fn device_of(path: &Path) -> io::Result<u64> {
    Ok(fs::metadata(path)?.dev())
}

/// Blobs are immutable once stored.
pub fn make_read_only(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o444))
}

/// Removes a file, treating "already gone" as success.
pub fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
```

`store.rs` (above the tests):
```rust
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;

use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::fsutil::{device_of, make_read_only, remove_if_exists, sync_dir};

const COPY_BUFFER: usize = 1024 * 1024;

/// Content-addressed blob store rooted at the data directory:
///
/// ```text
/// <data_dir>/blobs/ab/cd/<sha256>   immutable, read-only blobs
/// <data_dir>/tmp/                   in-progress writes (same filesystem, so renames are atomic)
/// <data_dir>/tmp/uploads/<id>       chunked upload sessions
/// ```
///
/// All methods block: call them from `spawn_blocking`.
#[derive(Debug, Clone)]
pub struct BlobStore {
    blobs: PathBuf,
    tmp: PathBuf,
    uploads: PathBuf,
}

/// Whether [`BlobStore::adopt`] stored new content or found it already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adopted {
    Stored,
    AlreadyPresent,
}

impl BlobStore {
    /// Creates the directory layout if needed and checks that `tmp/` and `blobs/`
    /// share a filesystem (otherwise `rename` would not be atomic).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let store = Self {
            blobs: data_dir.join("blobs"),
            tmp: data_dir.join("tmp"),
            uploads: data_dir.join("tmp").join("uploads"),
        };
        for dir in [&store.blobs, &store.tmp, &store.uploads] {
            fs::create_dir_all(dir)?;
        }
        if device_of(&store.tmp)? != device_of(&store.blobs)? {
            return Err(StorageError::CrossDevice(store.tmp.display().to_string()));
        }
        Ok(store)
    }

    pub fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        let (a, b) = hash.shard();
        self.blobs.join(a).join(b).join(hash.as_str())
    }

    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.blob_path(hash).is_file()
    }

    /// Moves a fully written, verified temporary file into place under `hash`.
    /// Idempotent: if the blob already exists the temporary file is removed.
    pub fn adopt(&self, tmp: &Path, hash: &ContentHash) -> Result<Adopted> {
        let dest = self.blob_path(hash);
        if dest.is_file() {
            remove_if_exists(tmp)?;
            return Ok(Adopted::AlreadyPresent);
        }
        File::open(tmp)?.sync_all()?;
        make_read_only(tmp)?;
        let dir = dest.parent().expect("blob paths have a parent");
        fs::create_dir_all(dir)?;
        fs::rename(tmp, &dest)?;
        sync_dir(dir)?;
        Ok(Adopted::Stored)
    }

    /// Streams `reader` into a temporary file while hashing it, then adopts it.
    pub fn put_reader(&self, mut reader: impl Read) -> Result<(ContentHash, u64)> {
        let tmp = self.tmp.join(format!("put-{}", Uuid::now_v7()));
        let result = (|| {
            let mut file = File::create_new(&tmp)?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0; COPY_BUFFER];
            let mut size = 0u64;
            loop {
                let read = match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => return Err(err.into()),
                };
                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read])?;
                size += read as u64;
            }
            let hash = ContentHash::from_digest(hasher.finalize().into());
            self.adopt(&tmp, &hash)?;
            Ok((hash, size))
        })();
        if result.is_err() {
            let _ = remove_if_exists(&tmp);
        }
        result
    }

    /// Opens a blob for reading, with its size.
    pub fn open_blob(&self, hash: &ContentHash) -> Result<(File, u64)> {
        let file = File::open(self.blob_path(hash)).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => StorageError::NotFound,
            _ => err.into(),
        })?;
        let size = file.metadata()?.len();
        Ok((file, size))
    }

    /// Deletes a blob; a missing blob is not an error.
    pub fn remove_blob(&self, hash: &ContentHash) -> Result<()> {
        Ok(remove_if_exists(&self.blob_path(hash))?)
    }

    /// Free bytes available to this process on the data filesystem.
    pub fn available_space(&self) -> Result<u64> {
        let stat = rustix::fs::statvfs(&self.blobs).map_err(io::Error::from)?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    pub(crate) fn uploads_dir(&self) -> &Path {
        &self.uploads
    }
}

```

Notes:
- `adopt` fsyncs the file *before* the rename and the directory *after*, so a crash leaves either no blob or a complete one.
- Blobs are made read-only (`0o444`) before the rename. Deleting still works (it depends on the directory's permissions).
- `open` fails with `CrossDevice` if `tmp/` is a separate mount: `rename` would then copy, which is neither atomic nor fast. Startup surfaces this (Task 11).
- `available_space` uses `statvfs` (`f_bavail * f_frsize`): space available to unprivileged processes.

**Step 5: Run tests** — `cargo test -p squirrel-storage` → PASS (7 tests).

**Step 6: Commit**
```bash
git add Cargo.toml Cargo.lock crates/squirrel-storage
git commit -m "feat(storage): add content-addressed blob store" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Upload files and MIME detection in `squirrel-storage`

**Files:**
- Create: `crates/squirrel-storage/src/uploads.rs`, `crates/squirrel-storage/src/mime.rs`
- Modify: `crates/squirrel-storage/src/lib.rs`

**Step 1: Write the failing tests**

`uploads.rs` test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn store() -> (tempfile::TempDir, BlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn chunks_written_in_any_order_reassemble_in_place() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        let hashed = store.hash_upload(id).unwrap();
        assert_eq!(hashed.hash.as_str(), HELLO);
        assert_eq!(hashed.size, 5);
        assert_eq!(hashed.head, b"hello");
    }

    #[test]
    fn rewriting_a_chunk_is_idempotent() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        assert_eq!(store.hash_upload(id).unwrap().hash.as_str(), HELLO);
    }

    #[test]
    fn recreating_an_upload_keeps_received_bytes() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        assert_eq!(store.hash_upload(id).unwrap().hash.as_str(), HELLO);
    }

    #[test]
    fn adopting_moves_the_upload_into_the_store() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hello").unwrap();
        let hash = store.hash_upload(id).unwrap().hash;
        assert_eq!(store.adopt_upload(id, &hash).unwrap(), Adopted::Stored);
        assert!(store.contains(&hash));
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn adopting_existing_content_discards_the_upload() {
        let (_dir, store) = store();
        store.put_reader(&b"hello"[..]).unwrap();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hello").unwrap();
        let hash = store.hash_upload(id).unwrap().hash;
        assert_eq!(store.adopt_upload(id, &hash).unwrap(), Adopted::AlreadyPresent);
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn lists_and_discards_uploads() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 1).unwrap();
        assert_eq!(store.list_uploads().unwrap(), vec![id]);
        store.discard_upload(id).unwrap();
        store.discard_upload(id).unwrap();
        assert!(store.list_uploads().unwrap().is_empty());
    }
}
```

`mime.rs` test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_wins_over_the_extension() {
        assert_eq!(detect(b"%PDF-1.7\n", "notes.txt"), "application/pdf");
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR", "x.jpg"), "image/png");
    }

    #[test]
    fn falls_back_to_the_extension_then_octet_stream() {
        assert_eq!(detect(b"hello", "notes.txt"), "text/plain");
        assert_eq!(detect(b"hello", "notes"), "application/octet-stream");
    }

    #[test]
    fn sniffs_html_even_when_named_otherwise() {
        assert_eq!(detect(b"<!DOCTYPE html><html>", "photo.png"), "text/html");
    }
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-storage` → FAIL.

**Step 3: Implement**

`mime.rs`:
```rust
/// Bytes of the file start used for content sniffing.
pub const SNIFF_LEN: usize = 8192;
const FALLBACK: &str = "application/octet-stream";

/// The MIME type of a file, from its leading bytes first (magic numbers via
/// `infer`), then from the name's extension, else `application/octet-stream`.
/// Never derived from what the client claims.
pub fn detect(head: &[u8], name: &str) -> String {
    if let Some(kind) = infer::get(head) {
        return kind.mime_type().to_owned();
    }
    mime_guess::from_path(name)
        .first_raw()
        .unwrap_or(FALLBACK)
        .to_owned()
}

```

`uploads.rs`:
```rust
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;

use uuid::Uuid;

use crate::error::Result;
use crate::mime::SNIFF_LEN;
use crate::fsutil::remove_if_exists;
use crate::store::{Adopted, BlobStore};

const HASH_BUFFER: usize = 1024 * 1024;

/// What [`BlobStore::hash_upload`] learned about a fully received upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashedUpload {
    pub hash: ContentHash,
    pub size: u64,
    /// The first bytes, for MIME sniffing.
    pub head: Vec<u8>,
}

impl BlobStore {
    fn upload_path(&self, id: Uuid) -> PathBuf {
        self.uploads_dir().join(id.to_string())
    }

    /// Creates the (sparse) file that chunks of upload `id` are written into.
    /// Calling it again for the same id keeps what was already received.
    pub fn create_upload(&self, id: Uuid, size: u64) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.upload_path(id))?;
        file.set_len(size)?;
        Ok(())
    }

    /// Writes one chunk at `offset` and makes it durable before returning, so a
    /// chunk recorded as received in the database is really on disk.
    pub fn write_chunk(&self, id: Uuid, offset: u64, data: &[u8]) -> Result<()> {
        let file = OpenOptions::new().write(true).open(self.upload_path(id))?;
        file.write_all_at(data, offset)?;
        file.sync_data()?;
        Ok(())
    }

    /// Streams the whole upload file through SHA-256.
    pub fn hash_upload(&self, id: Uuid) -> Result<HashedUpload> {
        let mut file = File::open(self.upload_path(id))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; HASH_BUFFER];
        let mut head = Vec::with_capacity(SNIFF_LEN);
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if head.len() < SNIFF_LEN {
                let take = (SNIFF_LEN - head.len()).min(read);
                head.extend_from_slice(&buffer[..take]);
            }
            hasher.update(&buffer[..read]);
            size += read as u64;
        }
        let hash = ContentHash::from_digest(hasher.finalize().into());
        Ok(HashedUpload { hash, size, head })
    }

    /// Moves a verified upload into the blob store (idempotent, see [`BlobStore::adopt`]).
    pub fn adopt_upload(&self, id: Uuid, hash: &ContentHash) -> Result<Adopted> {
        self.adopt(&self.upload_path(id), hash)
    }

    pub fn discard_upload(&self, id: Uuid) -> Result<()> {
        Ok(remove_if_exists(&self.upload_path(id))?)
    }

    /// Ids of every upload file on disk, to find files whose session is gone.
    pub fn list_uploads(&self) -> Result<Vec<Uuid>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.uploads_dir())? {
            let name = entry?.file_name();
            if let Some(id) = name.to_str().and_then(|n| Uuid::parse_str(n).ok()) {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

```

`lib.rs` — final shape:
```rust
//! Content-addressed blob storage on the local filesystem. Everything here
//! blocks; async callers use `spawn_blocking`.

mod error;
mod fsutil;
pub mod mime;
mod store;
mod uploads;

pub use error::{Result, StorageError};
pub use store::{Adopted, BlobStore};
pub use uploads::HashedUpload;
```

Also extend `write_chunk` so a missing upload file (aborted session) is reported as `StorageError::NotFound` rather than a generic I/O error — the API maps it to 404:
```rust
let file = OpenOptions::new()
    .write(true)
    .open(self.upload_path(id))
    .map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => StorageError::NotFound,
        _ => err.into(),
    })?;
```
(import `crate::error::StorageError`), and add a test `writing_to_a_discarded_upload_is_not_found`.

**Step 4: Run tests** — `cargo test -p squirrel-storage` → PASS (17 tests). Then the full Rust gate (fmt, clippy, workspace tests).

**Step 5: Commit**
```bash
git add crates/squirrel-storage
git commit -m "feat(storage): add chunked upload files and MIME sniffing" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Milestone B — Schema & repositories

All repository code lives in `squirrel-db`. Conventions inherited from Phase 1: runtime `query_as::<_, T>` with a `*_COLUMNS` constant, `DbError` for domain failures, `sqlx::test(migrator = "squirrel_db::MIGRATOR")` integration tests in `crates/squirrel-db/tests/`. Functions that must run inside a caller's transaction take `&mut PgConnection`; single-statement reads take `impl PgExecutor<'e>` so they accept a pool or a transaction.

Locking rules (enforced by every function below and relied on by the API):
1. Every tree change, quota check and upload creation/completion first calls `quota::lock_owner` (`SELECT … FROM users … FOR UPDATE`) in its transaction. This serializes them per user (no cycles from concurrent moves, no quota overcommit).
2. Code that stores or links a blob then takes `blobs::lock` (advisory xact lock on the hash). Order is always owner → blob.
3. Blob GC takes only the blob lock (session-level) and never an owner lock, so the order cannot invert.

### Task 5: Migration `20261113000002_files.sql`

**Files:**
- Create: `crates/squirrel-db/migrations/20261113000002_files.sql`
- Create: `crates/squirrel-db/tests/files_schema.rs`

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/files_schema.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

const INITIAL: &str = include_str!("../migrations/20261113000001_initial.sql");
const FILES: &str = include_str!("../migrations/20261113000002_files.sql");

async fn insert_user(pool: &PgPool, username: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (id, username, display_name, password_hash, role) \
         VALUES (gen_random_uuid(), $1, $1, 'x', 'user') RETURNING id",
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn root(pool: &PgPool, owner: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND name = $2")
        .bind(owner)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_blob(pool: &PgPool, fill: char, size: i64) -> String {
    let sha = fill.to_string().repeat(64);
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'text/plain')")
        .bind(&sha)
        .bind(size)
        .execute(pool)
        .await
        .unwrap();
    sha
}

async fn insert_node(
    pool: &PgPool,
    owner: Uuid,
    parent: Option<Uuid>,
    name: &str,
    blob: Option<(&str, i64)>,
) -> Result<Uuid, sqlx::Error> {
    let kind = if blob.is_some() { "file" } else { "folder" };
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .bind(kind)
    .bind(blob.map(|(sha, _)| sha.to_owned()))
    .bind(blob.map_or(0, |(_, size)| size))
    .bind(blob.map(|_| "text/plain"))
    .fetch_one(pool)
    .await
}

async fn ref_count(pool: &PgPool, sha: &str) -> (i64, bool) {
    sqlx::query_as("SELECT ref_count, zero_since IS NOT NULL FROM blobs WHERE sha256 = $1")
        .bind(sha)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn constraint_of(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.constraint().map(str::to_owned),
        _ => None,
    }
}

#[sqlx::test(migrations = false)]
async fn backfills_default_roots_for_existing_users(pool: PgPool) {
    sqlx::raw_sql(INITIAL).execute(&pool).await.unwrap();
    let alice = insert_user(&pool, "alice").await;
    sqlx::raw_sql(FILES).execute(&pool).await.unwrap();

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND kind = 'folder' ORDER BY name",
    )
    .bind(alice)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(names, ["Documents", "Files", "Photos"]);
}

#[sqlx::test(migrations = false)]
async fn live_sibling_names_are_unique_including_roots(pool: PgPool) {
    sqlx::raw_sql(INITIAL).execute(&pool).await.unwrap();
    let alice = insert_user(&pool, "alice").await;
    sqlx::raw_sql(FILES).execute(&pool).await.unwrap();
    let files = root(&pool, alice, "Files").await;

    let err = insert_node(&pool, alice, None, "Files", None).await.unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_live_name_key"));

    let first = insert_node(&pool, alice, Some(files), "a", None).await.unwrap();
    let err = insert_node(&pool, alice, Some(files), "a", None).await.unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_live_name_key"));

    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    insert_node(&pool, alice, Some(files), "a", None).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shape_constraints(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    sqlx::query("INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
                 SELECT gen_random_uuid(), $1, NULL, n, 'folder' \
                 FROM unnest(ARRAY['Photos','Documents','Files']) AS n")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let files = root(&pool, alice, "Files").await;
    let sha = insert_blob(&pool, 'a', 3).await;

    // A file needs a blob, a folder must not have one, roots are folders.
    let err = sqlx::query(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) VALUES (gen_random_uuid(), $1, $2, 'x', 'file')",
    )
    .bind(alice)
    .bind(files)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_file_has_blob"));
    let err = insert_node(&pool, alice, None, "loose.txt", Some((&sha, 3))).await.unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_roots_are_folders"));
    assert!(insert_node(&pool, alice, Some(files), &"a".repeat(256), None).await.is_err());
    assert!(sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ('../etc', 1, 'x')")
        .execute(&pool)
        .await
        .is_err());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ref_count_follows_file_nodes_including_cascades(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    let parent = insert_node(&pool, alice, None, "Files", None).await.unwrap();
    let sha = insert_blob(&pool, 'b', 5).await;
    assert_eq!(ref_count(&pool, &sha).await, (0, true));

    let folder = insert_node(&pool, alice, Some(parent), "dir", None).await.unwrap();
    insert_node(&pool, alice, Some(parent), "one.txt", Some((&sha, 5))).await.unwrap();
    insert_node(&pool, alice, Some(folder), "two.txt", Some((&sha, 5))).await.unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (2, false));

    sqlx::query("DELETE FROM nodes WHERE id = $1").bind(folder).execute(&pool).await.unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (1, false));
    sqlx::query("DELETE FROM nodes WHERE id = $1").bind(parent).execute(&pool).await.unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (0, true));
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-db --test files_schema` → FAIL: `include_str!` cannot find `20261113000002_files.sql`.

**Step 3: Write the migration** — `crates/squirrel-db/migrations/20261113000002_files.sql` (validated against Postgres 17 while writing this plan):

```sql
-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 2: content-addressed blobs, the per-user file tree, and upload sessions.

CREATE TABLE blobs (
    sha256      TEXT PRIMARY KEY CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    size        BIGINT NOT NULL CHECK (size >= 0),
    mime        TEXT NOT NULL CHECK (char_length(mime) BETWEEN 1 AND 255),
    -- Maintained by the nodes_blob_refcount trigger; never update by hand.
    ref_count   BIGINT NOT NULL DEFAULT 0 CHECK (ref_count >= 0),
    -- When ref_count last reached 0 (NULL while referenced). GC waits for a grace period.
    zero_since  TIMESTAMPTZ DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((ref_count = 0) = (zero_since IS NOT NULL))
);
CREATE INDEX blobs_unreferenced_idx ON blobs (zero_since) WHERE ref_count = 0;

CREATE TABLE nodes (
    id           UUID PRIMARY KEY,
    owner_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id    UUID REFERENCES nodes (id) ON DELETE CASCADE,
    name         TEXT NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 255),
    kind         TEXT NOT NULL CHECK (kind IN ('folder', 'file')),
    blob_sha256  TEXT REFERENCES blobs (sha256),
    size         BIGINT NOT NULL DEFAULT 0 CHECK (size >= 0),
    mime         TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at   TIMESTAMPTZ,
    -- Id of the node whose deletion put this one in the trash (itself for a trash entry).
    trash_root   UUID,
    CONSTRAINT nodes_file_has_blob CHECK ((kind = 'file') = (blob_sha256 IS NOT NULL)),
    CONSTRAINT nodes_folder_shape CHECK (kind = 'file' OR (size = 0 AND mime IS NULL)),
    CONSTRAINT nodes_roots_are_folders CHECK (parent_id IS NOT NULL OR kind = 'folder'),
    CONSTRAINT nodes_trash_consistent CHECK ((deleted_at IS NULL) = (trash_root IS NULL)),
    CONSTRAINT nodes_not_own_parent CHECK (parent_id <> id)
);
-- One live name per folder; NULLS NOT DISTINCT makes top-level (NULL parent) names unique too.
CREATE UNIQUE INDEX nodes_live_name_key ON nodes (owner_id, parent_id, name)
    NULLS NOT DISTINCT WHERE deleted_at IS NULL;
-- Folder listings: folders first, then names in byte order (keyset pagination).
CREATE INDEX nodes_children_idx ON nodes (owner_id, parent_id, kind DESC, name COLLATE "C")
    WHERE deleted_at IS NULL;
CREATE INDEX nodes_parent_idx ON nodes (parent_id);
CREATE INDEX nodes_owner_blob_idx ON nodes (owner_id, blob_sha256) WHERE blob_sha256 IS NOT NULL;
CREATE INDEX nodes_blob_idx ON nodes (blob_sha256) WHERE blob_sha256 IS NOT NULL;
CREATE INDEX nodes_trash_idx ON nodes (owner_id, deleted_at DESC, id) WHERE trash_root = id;

CREATE FUNCTION nodes_blob_refcount() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') AND OLD.blob_sha256 IS NOT NULL THEN
        UPDATE blobs
           SET ref_count = ref_count - 1,
               zero_since = CASE WHEN ref_count = 1 THEN now() ELSE NULL END
         WHERE sha256 = OLD.blob_sha256;
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') AND NEW.blob_sha256 IS NOT NULL THEN
        UPDATE blobs SET ref_count = ref_count + 1, zero_since = NULL
         WHERE sha256 = NEW.blob_sha256;
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER nodes_blob_refcount
    AFTER INSERT OR DELETE OR UPDATE OF blob_sha256 ON nodes
    FOR EACH ROW EXECUTE FUNCTION nodes_blob_refcount();

CREATE TABLE upload_sessions (
    id           UUID PRIMARY KEY,
    owner_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id    UUID NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    name         TEXT NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 255),
    sha256       TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    size         BIGINT NOT NULL CHECK (size >= 0),
    chunk_size   BIGINT NOT NULL CHECK (chunk_size > 0),
    chunk_count  INTEGER NOT NULL CHECK (chunk_count >= 0),
    -- 'completing' while one request verifies and stores the file.
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'completing')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Pushed back on every chunk: sessions expire after 24 h without activity.
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX upload_sessions_owner_idx ON upload_sessions (owner_id, parent_id, sha256);
CREATE INDEX upload_sessions_expires_idx ON upload_sessions (expires_at);
CREATE INDEX upload_sessions_parent_idx ON upload_sessions (parent_id);

CREATE TABLE upload_chunks (
    upload_id    UUID NOT NULL REFERENCES upload_sessions (id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL CHECK (chunk_index >= 0),
    PRIMARY KEY (upload_id, chunk_index)
);

-- Every existing user gets the default roots (new users get them from the application).
INSERT INTO nodes (id, owner_id, parent_id, name, kind)
SELECT gen_random_uuid(), u.id, NULL, r.name, 'folder'
  FROM users u CROSS JOIN (VALUES ('Photos'), ('Documents'), ('Files')) AS r (name);
```

Notes:
- `parent_id = $1` never matches NULL and `IS NOT DISTINCT FROM` cannot use an index; queries for the top level use `parent_id IS NULL` explicitly.
- `ON DELETE CASCADE` on `parent_id` means deleting a folder row deletes its subtree; the trigger fires per row, so ref counts stay exact.
- Upload sessions die with their destination folder (`ON DELETE CASCADE`); their files are swept by the orphan cleanup (Task 15).

**Step 4: Run tests** — `cargo test -p squirrel-db` → PASS (existing tests + 4 new). `ref_count_follows_file_nodes_including_cascades` creates its own "Files" root because new users created with raw SQL get no roots (only the migration backfill and the application create them).

**Step 5: Commit**
```bash
git add crates/squirrel-db/migrations/20261113000002_files.sql crates/squirrel-db/tests/files_schema.rs
git commit -m "feat(db): add blobs, nodes and upload session tables" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Node records, default roots and tree reads

**Files:**
- Create: `crates/squirrel-db/src/page.rs`, `crates/squirrel-db/src/nodes/mod.rs`, `crates/squirrel-db/src/nodes/read.rs`
- Create: `crates/squirrel-db/tests/common/mod.rs`, `crates/squirrel-db/tests/nodes.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `src/error.rs`, `src/users.rs`
- Modify: `crates/squirrel-api/src/error.rs` (map the new `DbError` variants so the workspace still builds)

**Step 1: Shared test helpers** — `crates/squirrel-db/tests/common/mod.rs`:

```rust
#![allow(dead_code)]

use sqlx::PgPool;
use squirrel_core::{ContentHash, DisplayName, Username};
use squirrel_db::invitations::{self, NewInvitation};
use squirrel_db::nodes::{self, NodeRecord};
use squirrel_db::users::{self, NewUser, UserRecord};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

pub async fn admin(pool: &PgPool) -> UserRecord {
    users::create_first_admin(pool, &new_user("alice")).await.unwrap()
}

/// A regular user invited by `inviter`, through the real join path.
pub async fn invited(pool: &PgPool, inviter: &UserRecord, name: &str) -> UserRecord {
    let token_hash = format!("token-{name}").into_bytes();
    invitations::create(
        pool,
        &NewInvitation {
            created_by: inviter.id,
            token_hash: &token_hash,
            email_hint: None,
            expires_at: OffsetDateTime::now_utc() + Duration::days(1),
        },
    )
    .await
    .unwrap();
    users::create_with_invitation(pool, &new_user(name), &token_hash)
        .await
        .unwrap()
}

pub async fn root(pool: &PgPool, owner: Uuid, name: &str) -> NodeRecord {
    nodes::list_children(pool, owner, None, None, 10)
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("no root named {name}"))
}

pub fn hash(fill: char) -> ContentHash {
    ContentHash::parse(&fill.to_string().repeat(64)).unwrap()
}

/// Inserts a folder with raw SQL (no validation, no lock), for read-side tests.
pub async fn raw_folder(pool: &PgPool, owner: Uuid, parent: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'folder') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Inserts a blob row (if missing) and a file node referencing it.
pub async fn raw_file(pool: &PgPool, owner: Uuid, parent: Uuid, name: &str, fill: char, size: i64) -> Uuid {
    let sha = hash(fill);
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'text/plain') ON CONFLICT DO NOTHING")
        .bind(sha.as_str())
        .bind(size)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, 'file', $4, $5, 'text/plain') RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .bind(sha.as_str())
    .bind(size)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn set_quota(pool: &PgPool, user: Uuid, quota: Option<i64>) {
    sqlx::query("UPDATE users SET quota_bytes = $2 WHERE id = $1")
        .bind(user)
        .bind(quota)
        .execute(pool)
        .await
        .unwrap();
}
```
Add `squirrel-core` usage is already a dependency; add `uuid` and `time` to `[dev-dependencies]` only if the compiler asks (they are normal dependencies of `squirrel-db`, so integration tests can use them).

**Step 2: Write the failing tests** — `crates/squirrel-db/tests/nodes.rs`:

```rust
mod common;

use sqlx::PgPool;
use squirrel_core::NodeKind;
use squirrel_db::nodes::{self, ListCursor};

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn every_new_user_gets_the_default_roots(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    for user in [&alice, &bob] {
        let page = nodes::list_children(&pool, user.id, None, None, 10).await.unwrap();
        let names: Vec<_> = page.items.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Documents", "Files", "Photos"]);
        assert!(page.items.iter().all(|n| n.is_root() && n.kind() == NodeKind::Folder));
        assert!(!page.has_more);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn children_are_listed_folders_first_and_paginated(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    common::raw_folder(&pool, alice.id, files.id, "Zeta").await;
    common::raw_folder(&pool, alice.id, files.id, "beta").await;
    common::raw_file(&pool, alice.id, files.id, "B.txt", 'b', 1).await;

    let mut seen = Vec::new();
    let mut cursor: Option<ListCursor> = None;
    loop {
        let page = nodes::list_children(&pool, alice.id, Some(files.id), cursor.as_ref(), 2)
            .await
            .unwrap();
        seen.extend(page.items.iter().map(|n| n.name.clone()));
        let Some(last) = page.items.last() else { break };
        cursor = Some(ListCursor { kind: last.kind(), name: last.name.clone() });
        if !page.has_more {
            break;
        }
    }
    // Folders first, then byte order ("B" < "a", "Z" < "b").
    assert_eq!(seen, ["Zeta", "beta", "B.txt", "a.txt"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn nodes_are_private_to_their_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;

    assert!(nodes::find_live(&pool, alice.id, file).await.unwrap().is_some());
    assert!(nodes::find_live(&pool, bob.id, file).await.unwrap().is_none());
    let bobs_view = nodes::list_children(&pool, bob.id, Some(files.id), None, 10).await.unwrap();
    assert!(bobs_view.items.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn find_live_folder_ignores_files_and_trash(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    assert!(nodes::find_live_folder(&pool, alice.id, file).await.unwrap().is_none());
    assert!(nodes::find_live_folder(&pool, alice.id, folder).await.unwrap().is_some());
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(folder)
        .execute(&pool)
        .await
        .unwrap();
    assert!(nodes::find_live_folder(&pool, alice.id, folder).await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn path_lists_ancestors_from_the_root(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let taxes = common::raw_folder(&pool, alice.id, docs.id, "Taxes").await;
    let year = common::raw_folder(&pool, alice.id, taxes, "2026").await;

    let path = nodes::path_to(&pool, alice.id, year).await.unwrap();
    let names: Vec<_> = path.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Documents", "Taxes"]);
    assert!(nodes::path_to(&pool, alice.id, docs.id).await.unwrap().is_empty());
}
```

**Step 3: Run to verify failure** — `cargo test -p squirrel-db --test nodes` → FAIL: unresolved `squirrel_db::nodes`.

**Step 4: Implement**

`src/page.rs`:
```rust
/// One page of a keyset-paginated listing.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Whether rows exist after the last item.
    pub has_more: bool,
}

impl<T> Page<T> {
    /// Builds a page from a query that fetched `limit + 1` rows.
    pub(crate) fn from_overfetch(mut rows: Vec<T>, limit: u32) -> Self {
        let limit = limit as usize;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        Self { items: rows, has_more }
    }
}
```

`src/error.rs` — add variants (before `Sqlx`) and a mapper:
```rust
    #[error("not found")]
    NotFound,
    #[error("a file or folder with this name already exists here")]
    NameConflict,
    #[error("top-level folders cannot be renamed, moved or deleted")]
    ProtectedRoot,
    #[error("a folder cannot be moved into itself or one of its subfolders")]
    MoveIntoSelf,
    #[error("this would exceed your storage quota")]
    QuotaExceeded,
    #[error("this upload is already being completed")]
    UploadBusy,
```
```rust
/// Maps a violation of the live-name unique index to `NameConflict`.
pub(crate) fn name_conflict_or(err: sqlx::Error) -> DbError {
    if is_unique_violation(&err, crate::nodes::LIVE_NAME_CONSTRAINT) {
        DbError::NameConflict
    } else {
        err.into()
    }
}
```

`src/nodes/mod.rs`:
```rust
//! The per-user file tree. Every function is scoped by `owner_id`: another
//! user's node behaves exactly like a missing one.

mod read;

pub use read::{ListCursor, PathSegment, find_live, find_live_folder, list_children, path_to};

use sqlx::{FromRow, PgConnection};
use squirrel_core::NodeKind;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

/// Top-level folders every user owns. They cannot be renamed, moved or trashed.
pub const DEFAULT_ROOTS: [&str; 3] = ["Photos", "Documents", "Files"];
/// Where restored items go when their original folder is no longer live.
pub const RESTORE_FALLBACK_ROOT: &str = "Files";
pub(crate) const LIVE_NAME_CONSTRAINT: &str = "nodes_live_name_key";
pub(crate) const NODE_COLUMNS: &str = "id, owner_id, parent_id, name, kind, blob_sha256, size, \
     mime, created_at, modified_at, deleted_at";

#[derive(Debug, Clone, FromRow)]
pub struct NodeRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    kind: String,
    pub blob_sha256: Option<String>,
    pub size: i64,
    pub mime: Option<String>,
    pub created_at: OffsetDateTime,
    pub modified_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

impl NodeRecord {
    /// The CHECK constraint guarantees a known kind. An unknown one is treated as
    /// a folder, for which no content is ever served.
    pub fn kind(&self) -> NodeKind {
        self.kind.parse().unwrap_or_else(|_| {
            tracing::warn!(node_id = %self.id, kind = %self.kind, "unknown node kind in database");
            NodeKind::Folder
        })
    }

    pub fn is_folder(&self) -> bool {
        self.kind() == NodeKind::Folder
    }

    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }
}

/// Creates the default roots for a new user, inside the caller's transaction.
pub(crate) async fn create_default_roots(conn: &mut PgConnection, owner_id: Uuid) -> Result<(), DbError> {
    for name in DEFAULT_ROOTS {
        sqlx::query(
            "INSERT INTO nodes (id, owner_id, parent_id, name, kind) VALUES ($1, $2, NULL, $3, 'folder')",
        )
        .bind(Uuid::now_v7())
        .bind(owner_id)
        .bind(name)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
```

`src/nodes/read.rs`:
```rust
use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgPool};
use squirrel_core::NodeKind;
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::{DbError, Page};

/// Where a folder listing resumes: after this (kind, name) in listing order
/// (folders first, then names in byte order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCursor {
    pub kind: NodeKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct PathSegment {
    pub id: Uuid,
    pub name: String,
}

/// Ancestor walks stop here, so a corrupted cycle cannot loop forever.
const MAX_DEPTH: i32 = 1000;

pub async fn find_live<'e>(
    executor: impl PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(executor)
        .await?)
}

pub async fn find_live_folder<'e>(
    executor: impl PgExecutor<'e>,
    owner_id: Uuid,
    id: Uuid,
) -> Result<Option<NodeRecord>, DbError> {
    Ok(find_live(executor, owner_id, id)
        .await?
        .filter(NodeRecord::is_folder))
}

/// Live children of `parent_id` (the roots when `None`), after `after`.
pub async fn list_children(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Option<Uuid>,
    after: Option<&ListCursor>,
    limit: u32,
) -> Result<Page<NodeRecord>, DbError> {
    let parent_filter = if parent_id.is_some() {
        "parent_id = $2"
    } else {
        "parent_id IS NULL AND $2::uuid IS NULL"
    };
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND {parent_filter} AND deleted_at IS NULL \
           AND ($3::text IS NULL OR kind < $3 OR (kind = $3 AND name COLLATE \"C\" > $4)) \
         ORDER BY kind DESC, name COLLATE \"C\" \
         LIMIT $5"
    );
    let rows = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(after.map(|c| c.kind.as_str()))
        .bind(after.map(|c| c.name.as_str()))
        .bind(i64::from(limit) + 1)
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// Ancestors of `id`, top-level folder first, excluding `id` itself.
pub async fn path_to(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<Vec<PathSegment>, DbError> {
    Ok(sqlx::query_as::<_, PathSegment>(
        "WITH RECURSIVE chain (id, parent_id, name, depth) AS ( \
             SELECT id, parent_id, name, 0 FROM nodes WHERE id = $1 AND owner_id = $2 \
           UNION ALL \
             SELECT n.id, n.parent_id, n.name, c.depth + 1 \
             FROM nodes n JOIN chain c ON n.id = c.parent_id \
             WHERE n.owner_id = $2 AND c.depth < $3 \
         ) \
         SELECT id, name FROM chain WHERE id <> $1 ORDER BY depth DESC",
    )
    .bind(id)
    .bind(owner_id)
    .bind(MAX_DEPTH)
    .fetch_all(pool)
    .await?)
}
```
("folder" > "file" in every collation, so `kind DESC` lists folders first.)

`src/lib.rs` — add `pub mod nodes; mod page;` and `pub use page::Page;`.

`src/users.rs` — create the roots in the same transaction as the user (`insert` is used by both `create_first_admin` and `create_with_invitation`):
```rust
async fn insert(
    conn: &mut PgConnection,
    new_user: &NewUser,
    role: Role,
    invited_by: Option<Uuid>,
) -> Result<UserRecord, DbError> {
    let sql = format!(/* unchanged */);
    let user = sqlx::query_as::<_, UserRecord>(&sql)
        /* binds unchanged */
        .fetch_one(&mut *conn)
        .await
        .map_err(/* unchanged */)?;
    crate::nodes::create_default_roots(conn, user.id).await?;
    Ok(user)
}
```

`crates/squirrel-api/src/error.rs` — two new variants and the mapping (the remaining storage variants arrive in Task 11):
```rust
    #[error("a file or folder with this name already exists here")]
    NameConflict,
    #[error("this would exceed your storage quota")]
    QuotaExceeded,
```
```rust
            ApiError::NameConflict => (StatusCode::CONFLICT, "name_conflict"),
            ApiError::QuotaExceeded => (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded"),
```
```rust
            // new arms in `impl From<DbError> for ApiError`, before `DbError::Sqlx`
            DbError::NotFound => ApiError::NotFound,
            DbError::NameConflict => ApiError::NameConflict,
            DbError::QuotaExceeded => ApiError::QuotaExceeded,
            other @ (DbError::ProtectedRoot | DbError::MoveIntoSelf) => {
                ApiError::Validation(other.to_string())
            }
            other @ DbError::UploadBusy => ApiError::Conflict(other.to_string()),
```

**Step 5: Run tests** — `cargo test -p squirrel-db` → PASS (5 new). `cargo test --workspace` → PASS (Phase 1 API tests are unaffected by the roots).

**Step 6: Commit**
```bash
git add crates/squirrel-db crates/squirrel-api/src/error.rs
git commit -m "feat(db): add node records, default roots and tree reads" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Owner lock, quota and tree mutations

**Files:**
- Create: `crates/squirrel-db/src/quota.rs`, `crates/squirrel-db/src/nodes/tree.rs`, `crates/squirrel-db/tests/tree.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `src/nodes/mod.rs`, `src/users.rs`

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/tree.rs`:

```rust
mod common;

use sqlx::PgPool;
use squirrel_core::{NodeName, Username};
use squirrel_db::DbError;
use squirrel_db::nodes::{self, NodeChange};
use squirrel_db::{quota, users};

fn name(raw: &str) -> NodeName {
    NodeName::parse(raw).unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_folders_and_refuses_duplicate_names(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = nodes::create_folder(&pool, alice.id, files.id, &name("Taxes")).await.unwrap();
    assert_eq!((folder.name.as_str(), folder.parent_id), ("Taxes", Some(files.id)));
    let err = nodes::create_folder(&pool, alice.id, files.id, &name("Taxes")).await.unwrap_err();
    assert!(matches!(err, DbError::NameConflict));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_cannot_be_created_in_files_or_foreign_folders(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    assert!(matches!(
        nodes::create_folder(&pool, alice.id, file, &name("x")).await,
        Err(DbError::NotFound)
    ));
    assert!(matches!(
        nodes::create_folder(&pool, bob.id, files.id, &name("x")).await,
        Err(DbError::NotFound)
    ));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renames_and_moves(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;

    let renamed = nodes::update(&pool, alice.id, file, &NodeChange { name: Some(name("b.txt")), parent_id: None })
        .await
        .unwrap();
    assert_eq!(renamed.name, "b.txt");
    let moved = nodes::update(&pool, alice.id, file, &NodeChange { name: None, parent_id: Some(docs.id) })
        .await
        .unwrap();
    assert_eq!((moved.name.as_str(), moved.parent_id), ("b.txt", Some(docs.id)));

    common::raw_file(&pool, alice.id, docs.id, "c.txt", 'c', 1).await;
    let clash = nodes::update(&pool, alice.id, file, &NodeChange { name: Some(name("c.txt")), parent_id: None }).await;
    assert!(matches!(clash, Err(DbError::NameConflict)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn moves_cannot_create_cycles(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_folder(&pool, alice.id, files.id, "a").await;
    let b = common::raw_folder(&pool, alice.id, a, "b").await;
    for target in [a, b] {
        let result = nodes::update(&pool, alice.id, a, &NodeChange { name: None, parent_id: Some(target) }).await;
        assert!(matches!(result, Err(DbError::MoveIntoSelf)), "into {target}");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn roots_are_protected_and_foreign_nodes_invisible(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let rename_root = nodes::update(&pool, alice.id, files.id, &NodeChange { name: Some(name("Stuff")), parent_id: None }).await;
    assert!(matches!(rename_root, Err(DbError::ProtectedRoot)));

    let file = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let bobs_files = common::root(&pool, bob.id, "Files").await;
    let steal = nodes::update(&pool, bob.id, file, &NodeChange { name: Some(name("mine.txt")), parent_id: None }).await;
    assert!(matches!(steal, Err(DbError::NotFound)));
    let push = nodes::update(&pool, alice.id, file, &NodeChange { name: None, parent_id: Some(bobs_files.id) }).await;
    assert!(matches!(push, Err(DbError::NotFound)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn usage_counts_live_files_and_trash_separately(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 100).await;
    let trashed = common::raw_file(&pool, alice.id, files.id, "b.txt", 'b', 30).await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(trashed)
        .execute(&pool)
        .await
        .unwrap();

    let usage = quota::usage(&pool, alice.id).await.unwrap();
    assert_eq!((usage.used_bytes, usage.trash_bytes, usage.quota_bytes), (100, 30, None));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ensure_room_compares_used_plus_extra_with_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 60).await;
    assert!(users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100)).await.unwrap());

    let mut tx = pool.begin().await.unwrap();
    let limit = quota::lock_owner(&mut tx, alice.id).await.unwrap();
    assert_eq!(limit, Some(100));
    quota::ensure_room(&mut tx, alice.id, limit, 40, quota::Pending::Ignore).await.unwrap();
    let err = quota::ensure_room(&mut tx, alice.id, limit, 41, quota::Pending::Ignore).await.unwrap_err();
    assert!(matches!(err, DbError::QuotaExceeded));
    quota::ensure_room(&mut tx, alice.id, None, i64::MAX, quota::Pending::Reserve).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn set_quota_reports_unknown_users(pool: PgPool) {
    common::admin(&pool).await;
    assert!(!users::set_quota(&pool, &Username::parse("nobody").unwrap(), Some(1)).await.unwrap());
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-db --test tree` → FAIL (missing `quota`, `NodeChange`, `create_folder`, `update`, `set_quota`).

**Step 3: Implement**

`src/quota.rs`:
```rust
//! Storage usage and the per-user lock that serializes tree changes.

use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::DbError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct Usage {
    /// Logical size of live files (what the quota limits).
    pub used_bytes: i64,
    /// Logical size of files in the trash (not counted against the quota).
    pub trash_bytes: i64,
    /// `None`: unlimited.
    pub quota_bytes: Option<i64>,
}

/// Whether the owner's open upload sessions count as already used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Ignore,
    /// Used when opening a session, so parallel sessions cannot overcommit.
    Reserve,
}

pub async fn usage(pool: &PgPool, owner_id: Uuid) -> Result<Usage, DbError> {
    sqlx::query_as::<_, Usage>(
        "SELECT u.quota_bytes, \
           COALESCE(SUM(n.size) FILTER (WHERE n.deleted_at IS NULL), 0)::bigint AS used_bytes, \
           COALESCE(SUM(n.size) FILTER (WHERE n.deleted_at IS NOT NULL), 0)::bigint AS trash_bytes \
         FROM users u LEFT JOIN nodes n ON n.owner_id = u.id AND n.kind = 'file' \
         WHERE u.id = $1 GROUP BY u.id",
    )
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    .ok_or(DbError::NotFound)
}

/// Locks the owner's row until the transaction ends and returns their quota.
/// Every tree change and quota check takes this lock first.
pub async fn lock_owner(conn: &mut PgConnection, owner_id: Uuid) -> Result<Option<i64>, DbError> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT quota_bytes FROM users WHERE id = $1 FOR UPDATE")
            .bind(owner_id)
            .fetch_optional(&mut *conn)
            .await?;
    row.map(|(quota,)| quota).ok_or(DbError::NotFound)
}

/// Fails with `QuotaExceeded` unless `extra` more bytes fit. Call after
/// [`lock_owner`] in the same transaction, passing the quota it returned.
pub async fn ensure_room(
    conn: &mut PgConnection,
    owner_id: Uuid,
    quota_bytes: Option<i64>,
    extra: i64,
    pending: Pending,
) -> Result<(), DbError> {
    let Some(quota_bytes) = quota_bytes else {
        return Ok(());
    };
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::bigint FROM nodes \
         WHERE owner_id = $1 AND kind = 'file' AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .fetch_one(&mut *conn)
    .await?;
    let reserved: i64 = match pending {
        Pending::Ignore => 0,
        Pending::Reserve => {
            sqlx::query_scalar(
                "SELECT COALESCE(SUM(size), 0)::bigint FROM upload_sessions \
                 WHERE owner_id = $1 AND expires_at > now()",
            )
            .bind(owner_id)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if used.saturating_add(reserved).saturating_add(extra) > quota_bytes {
        return Err(DbError::QuotaExceeded);
    }
    Ok(())
}
```

`src/nodes/tree.rs`:
```rust
use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord, find_live, find_live_folder};
use crate::DbError;
use crate::error::name_conflict_or;
use crate::quota::lock_owner;

/// A rename and/or a move; `None` keeps the current value.
#[derive(Debug, Clone, Default)]
pub struct NodeChange {
    pub name: Option<NodeName>,
    pub parent_id: Option<Uuid>,
}

pub async fn create_folder(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner_id).await?;
    find_live_folder(&mut *tx, owner_id, parent_id)
        .await?
        .ok_or(DbError::NotFound)?;
    let sql = format!(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
         VALUES ($1, $2, $3, $4, 'folder') RETURNING {NODE_COLUMNS}"
    );
    let node = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(owner_id)
        .bind(parent_id)
        .bind(name.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(name_conflict_or)?;
    tx.commit().await?;
    Ok(node)
}

/// Renames and/or moves a live, non-root node. A move target must be a live
/// folder of the same owner that is neither the node nor one of its descendants.
pub async fn update(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    change: &NodeChange,
) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    lock_owner(&mut tx, owner_id).await?;
    let node = find_live(&mut *tx, owner_id, id).await?.ok_or(DbError::NotFound)?;
    let Some(current_parent) = node.parent_id else {
        return Err(DbError::ProtectedRoot);
    };
    let parent_id = match change.parent_id {
        Some(target) if target != current_parent => {
            find_live_folder(&mut *tx, owner_id, target)
                .await?
                .ok_or(DbError::NotFound)?;
            if is_self_or_ancestor(&mut tx, id, target).await? {
                return Err(DbError::MoveIntoSelf);
            }
            target
        }
        _ => current_parent,
    };
    let name = change.name.as_ref().map_or(node.name.as_str(), NodeName::as_str);
    let sql = format!(
        "UPDATE nodes SET name = $3, parent_id = $4 WHERE id = $1 AND owner_id = $2 \
         RETURNING {NODE_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(name)
        .bind(parent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(name_conflict_or)?;
    tx.commit().await?;
    Ok(updated)
}

/// Whether `node` is `candidate` or one of its ancestors (moving `node` into
/// `candidate` would create a cycle). `UNION` stops on revisits.
async fn is_self_or_ancestor(conn: &mut PgConnection, node: Uuid, candidate: Uuid) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up (id, parent_id) AS ( \
             SELECT id, parent_id FROM nodes WHERE id = $2 \
           UNION \
             SELECT n.id, n.parent_id FROM nodes n JOIN up ON n.id = up.parent_id \
         ) \
         SELECT EXISTS (SELECT 1 FROM up WHERE id = $1)",
    )
    .bind(node)
    .bind(candidate)
    .fetch_one(conn)
    .await?)
}
```
`nodes/mod.rs`: `mod tree; pub use tree::{NodeChange, create_folder, update};`. `lib.rs`: `pub mod quota;`.

`src/users.rs`:
```rust
/// Sets (or clears, with `None`) a user's quota. Returns whether the user exists.
pub async fn set_quota(pool: &PgPool, username: &Username, quota_bytes: Option<i64>) -> Result<bool, DbError> {
    let result = sqlx::query("UPDATE users SET quota_bytes = $2 WHERE username = $1")
        .bind(username.as_str())
        .bind(quota_bytes)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}
```

**Step 4: Run tests** — `cargo test -p squirrel-db --test tree` → PASS (8 tests).

**Step 5: Commit**
```bash
git add crates/squirrel-db
git commit -m "feat(db): add quota checks, folder creation, rename and move" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Blob catalogue and file nodes

**Files:**
- Create: `crates/squirrel-db/src/blobs.rs`, `crates/squirrel-db/src/nodes/insert.rs`, `crates/squirrel-db/tests/blobs.rs`
- Modify: `crates/squirrel-db/src/lib.rs`, `src/nodes/mod.rs`

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/blobs.rs`:

```rust
mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::blobs;
use squirrel_db::nodes::{self, NewFile};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn link(pool: &PgPool, owner: Uuid, parent: Uuid, name: &str, fill: char) -> nodes::NodeRecord {
    let hash = common::hash(fill);
    let mut tx = pool.begin().await.unwrap();
    squirrel_db::quota::lock_owner(&mut tx, owner).await.unwrap();
    blobs::insert_if_absent(&mut tx, &hash, 5, "text/plain").await.unwrap();
    let node = nodes::insert_file(
        &mut tx,
        &NewFile {
            owner_id: owner,
            parent_id: parent,
            name: &NodeName::parse(name).unwrap(),
            blob: &hash,
            size: 5,
            mime: "text/plain",
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    node
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn insert_if_absent_is_idempotent(pool: PgPool) {
    let hash = common::hash('a');
    let mut conn = pool.acquire().await.unwrap();
    let first = blobs::insert_if_absent(&mut conn, &hash, 5, "text/plain").await.unwrap();
    let second = blobs::insert_if_absent(&mut conn, &hash, 5, "image/png").await.unwrap();
    assert_eq!((first.size, second.mime.as_str(), second.ref_count), (5, "text/plain", 0));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn file_nodes_get_free_names_and_count_references(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    let b = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    let c = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    assert_eq!([a.name, b.name, c.name], ["a.txt", "a (1).txt", "a (2).txt"]);
    assert_eq!(blobs::find(&pool, &common::hash('a')).await.unwrap().unwrap().ref_count, 3);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashed_files_do_not_block_names(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let first = link(&pool, alice.id, files.id, "a.txt", 'a').await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(link(&pool, alice.id, files.id, "a.txt", 'a').await.name, "a.txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn find_owned_only_sees_the_callers_content(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    link(&pool, alice.id, files.id, "a.txt", 'a').await;

    let mut conn = pool.acquire().await.unwrap();
    assert!(blobs::find_owned(&mut conn, alice.id, &common::hash('a')).await.unwrap().is_some());
    assert!(blobs::find_owned(&mut conn, bob.id, &common::hash('a')).await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_long_unreferenced_blobs_are_collectable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let kept = link(&pool, alice.id, files.id, "kept.txt", 'k').await;
    let dropped = link(&pool, alice.id, files.id, "gone.txt", 'g').await;
    sqlx::query("DELETE FROM nodes WHERE id = $1").bind(dropped.id).execute(&pool).await.unwrap();

    let past = OffsetDateTime::now_utc() - Duration::hours(1);
    let future = OffsetDateTime::now_utc() + Duration::hours(1);
    assert!(blobs::unreferenced_since(&pool, past, 10).await.unwrap().is_empty());
    assert_eq!(blobs::unreferenced_since(&pool, future, 10).await.unwrap(), [common::hash('g')]);

    let mut conn = pool.acquire().await.unwrap();
    assert!(!blobs::delete_if_unreferenced(&mut conn, &common::hash('g'), past).await.unwrap());
    assert!(!blobs::delete_if_unreferenced(&mut conn, &common::hash('k'), future).await.unwrap());
    assert!(blobs::delete_if_unreferenced(&mut conn, &common::hash('g'), future).await.unwrap());
    assert!(blobs::find(&pool, &common::hash('g')).await.unwrap().is_none());
    assert_eq!(blobs::find(&pool, &common::hash('k')).await.unwrap().unwrap().ref_count, 1);
    assert_eq!(kept.name, "kept.txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn session_locks_exclude_other_connections_until_released(pool: PgPool) {
    let hash = common::hash('a');
    let mut first = pool.acquire().await.unwrap();
    blobs::lock_session(&mut first, &hash).await.unwrap();
    let mut second = pool.acquire().await.unwrap();
    let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(hash.lock_key())
        .fetch_one(&mut *second)
        .await
        .unwrap();
    assert!(!taken, "another connection must not get the lock");
    blobs::unlock_session(&mut first, &hash).await.unwrap();
    let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(hash.lock_key())
        .fetch_one(&mut *second)
        .await
        .unwrap();
    assert!(taken);
}
```
**Step 2: Run to verify failure** — `cargo test -p squirrel-db --test blobs` → FAIL (missing `blobs`, `NewFile`, `insert_file`).

**Step 3: Implement**

`src/blobs.rs`:
```rust
//! The blob catalogue. `ref_count` is maintained by a trigger on `nodes`;
//! this module only inserts rows, looks them up and deletes unreferenced ones.

use sqlx::{FromRow, PgConnection, PgPool};
use squirrel_core::ContentHash;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const BLOB_COLUMNS: &str = "sha256, size, mime, ref_count, zero_since, created_at";

#[derive(Debug, Clone, FromRow)]
pub struct BlobRecord {
    pub sha256: String,
    pub size: i64,
    pub mime: String,
    pub ref_count: i64,
    pub zero_since: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

/// Serializes storing and linking a blob with deleting it, until the
/// transaction ends. Take it after `quota::lock_owner`.
pub async fn lock(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(hash.lock_key())
        .execute(conn)
        .await?;
    Ok(())
}

/// Session-level form for GC, which must hold the lock across a commit and the
/// file deletion. Always pair with [`unlock_session`] on the same connection.
pub async fn lock_session(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(hash.lock_key())
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn unlock_session(conn: &mut PgConnection, hash: &ContentHash) -> Result<(), DbError> {
    let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(hash.lock_key())
        .fetch_one(conn)
        .await?;
    if !released {
        tracing::warn!(%hash, "blob advisory lock was not held");
    }
    Ok(())
}

pub async fn find(pool: &PgPool, hash: &ContentHash) -> Result<Option<BlobRecord>, DbError> {
    let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE sha256 = $1");
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(hash.as_str())
        .fetch_optional(pool)
        .await?)
}

/// The blob if a node of `owner_id` (live or in their trash) references it.
/// This is the only dedup lookup: other users' content is never revealed.
pub async fn find_owned(
    conn: &mut PgConnection,
    owner_id: Uuid,
    hash: &ContentHash,
) -> Result<Option<BlobRecord>, DbError> {
    let sql = format!(
        "SELECT {BLOB_COLUMNS} FROM blobs b WHERE b.sha256 = $2 \
         AND EXISTS (SELECT 1 FROM nodes n WHERE n.owner_id = $1 AND n.blob_sha256 = b.sha256)"
    );
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(owner_id)
        .bind(hash.as_str())
        .fetch_optional(conn)
        .await?)
}

/// Records a stored blob; an existing row is returned unchanged.
pub async fn insert_if_absent(
    conn: &mut PgConnection,
    hash: &ContentHash,
    size: i64,
    mime: &str,
) -> Result<BlobRecord, DbError> {
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, $3) ON CONFLICT (sha256) DO NOTHING")
        .bind(hash.as_str())
        .bind(size)
        .bind(mime)
        .execute(&mut *conn)
        .await?;
    let sql = format!("SELECT {BLOB_COLUMNS} FROM blobs WHERE sha256 = $1");
    Ok(sqlx::query_as::<_, BlobRecord>(&sql)
        .bind(hash.as_str())
        .fetch_one(conn)
        .await?)
}

/// Blobs with no reference since before `cutoff`, oldest first.
pub async fn unreferenced_since(
    pool: &PgPool,
    cutoff: OffsetDateTime,
    limit: i64,
) -> Result<Vec<ContentHash>, DbError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT sha256 FROM blobs WHERE ref_count = 0 AND zero_since < $1 ORDER BY zero_since LIMIT $2",
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().filter_map(|sha| ContentHash::parse(sha).ok()).collect())
}

/// Deletes the row if it is still unreferenced since before `cutoff`. Run in
/// autocommit while holding [`lock_session`].
pub async fn delete_if_unreferenced(
    conn: &mut PgConnection,
    hash: &ContentHash,
    cutoff: OffsetDateTime,
) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM blobs WHERE sha256 = $1 AND ref_count = 0 AND zero_since < $2")
        .bind(hash.as_str())
        .bind(cutoff)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() == 1)
}
```

`src/nodes/insert.rs`:
```rust
use sqlx::PgConnection;
use squirrel_core::{ContentHash, NodeName};
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::DbError;

/// Give up looking for a free "name (n).ext" after this many tries.
const MAX_NAME_VARIANTS: u32 = 1000;

#[derive(Debug, Clone)]
pub struct NewFile<'a> {
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: &'a NodeName,
    pub blob: &'a ContentHash,
    pub size: i64,
    pub mime: &'a str,
}

/// The first free variant of `name` among the live children of `parent_id`
/// (`name`, `name (1)`, …). The caller holds the owner lock, so the answer
/// stays true until it writes.
pub async fn free_name(
    conn: &mut PgConnection,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<NodeName, DbError> {
    for n in 0..MAX_NAME_VARIANTS {
        let candidate = name.variant(n);
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM nodes \
             WHERE owner_id = $1 AND parent_id = $2 AND name = $3 AND deleted_at IS NULL)",
        )
        .bind(owner_id)
        .bind(parent_id)
        .bind(candidate.as_str())
        .fetch_one(&mut *conn)
        .await?;
        if !taken {
            return Ok(candidate);
        }
    }
    Err(DbError::NameConflict)
}

/// Inserts a file node under a free variant of its name. Preconditions: the
/// caller holds the owner lock, checked that the parent is a live folder of the
/// owner, and the blob row exists.
pub async fn insert_file(conn: &mut PgConnection, file: &NewFile<'_>) -> Result<NodeRecord, DbError> {
    let name = free_name(conn, file.owner_id, file.parent_id, file.name).await?;
    let sql = format!(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES ($1, $2, $3, $4, 'file', $5, $6, $7) RETURNING {NODE_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(file.owner_id)
        .bind(file.parent_id)
        .bind(name.as_str())
        .bind(file.blob.as_str())
        .bind(file.size)
        .bind(file.mime)
        .fetch_one(conn)
        .await?)
}
```
`nodes/mod.rs`: `mod insert; pub use insert::{NewFile, free_name, insert_file};`. `lib.rs`: `pub mod blobs;`.

**Step 4: Run tests** — `cargo test -p squirrel-db --test blobs` → PASS (6 tests).

**Step 5: Commit**
```bash
git add crates/squirrel-db
git commit -m "feat(db): add blob catalogue and file node insertion" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Upload sessions repository

**Files:**
- Create: `crates/squirrel-db/src/uploads.rs`, `crates/squirrel-db/tests/uploads.rs`
- Modify: `crates/squirrel-db/src/lib.rs` (`pub mod uploads;`)

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/uploads.rs`:

```rust
mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::DbError;
use squirrel_db::uploads::{self, NewUpload};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn file_name() -> NodeName {
    NodeName::parse("big.bin").unwrap()
}

async fn open(pool: &PgPool, owner: Uuid, parent: Uuid, size: i64) -> Result<uploads::UploadRecord, DbError> {
    let (name, sha) = (file_name(), common::hash('f'));
    uploads::create(
        pool,
        &NewUpload {
            owner_id: owner,
            parent_id: parent,
            name: &name,
            sha256: &sha,
            size,
            chunk_size: 4,
            chunk_count: i32::try_from(size.div_ceil(4)).unwrap(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(24),
        },
    )
    .await
}

fn later() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::hours(24)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_need_a_live_folder_of_the_owner(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    assert!(session.is_open());
    assert_eq!(session.chunk_count, 3);
    assert!(matches!(open(&pool, bob.id, files.id, 10).await, Err(DbError::NotFound)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn open_sessions_count_against_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::set_quota(&pool, alice.id, Some(100)).await;
    open(&pool, alice.id, files.id, 60).await.unwrap();
    assert!(matches!(open(&pool, alice.id, files.id, 60).await, Err(DbError::QuotaExceeded)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_are_recorded_once_and_listed_in_order(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    for index in [2, 0, 2] {
        assert!(uploads::record_chunk(&pool, alice.id, session.id, index, later()).await.unwrap());
    }
    assert_eq!(uploads::received_chunks(&pool, session.id).await.unwrap(), [0, 2]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn claiming_is_exclusive_until_released(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();

    let claimed = uploads::claim(&pool, alice.id, session.id, later()).await.unwrap();
    assert!(!claimed.is_open());
    assert!(matches!(uploads::claim(&pool, alice.id, session.id, later()).await, Err(DbError::UploadBusy)));
    assert!(!uploads::record_chunk(&pool, alice.id, session.id, 0, later()).await.unwrap());
    assert!(matches!(uploads::abort(&pool, alice.id, session.id).await, Err(DbError::UploadBusy)));

    uploads::release(&pool, session.id).await.unwrap();
    uploads::claim(&pool, alice.id, session.id, later()).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn resumable_sessions_match_the_exact_file(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    let (name, sha) = (file_name(), common::hash('f'));
    let found = uploads::find_resumable(&pool, alice.id, files.id, &name, &sha, 10).await.unwrap();
    assert_eq!(found.map(|s| s.id), Some(session.id));
    assert!(uploads::find_resumable(&pool, alice.id, files.id, &name, &sha, 11).await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn sessions_are_private_and_abortable(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    assert!(uploads::find(&pool, bob.id, session.id).await.unwrap().is_none());
    assert!(matches!(uploads::abort(&pool, bob.id, session.id).await, Err(DbError::NotFound)));
    uploads::abort(&pool, alice.id, session.id).await.unwrap();
    assert!(uploads::find(&pool, alice.id, session.id).await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_sessions_are_hidden_and_purged(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let session = open(&pool, alice.id, files.id, 10).await.unwrap();
    let live = open(&pool, alice.id, files.id, 4).await.unwrap();
    sqlx::query("UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1")
        .bind(session.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(uploads::find(&pool, alice.id, session.id).await.unwrap().is_none());
    assert_eq!(uploads::delete_expired(&pool).await.unwrap(), [session.id]);
    assert_eq!(uploads::existing(&pool, &[session.id, live.id]).await.unwrap(), [live.id]);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-db --test uploads` → FAIL.

**Step 3: Implement** — `src/uploads.rs`:
```rust
//! Upload sessions: which file is being uploaded where, and which chunks arrived.
//! The bytes themselves live in the blob store's upload area.

use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgPool};
use squirrel_core::{ContentHash, NodeName};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::nodes::find_live_folder;
use crate::quota::{self, Pending};

const UPLOAD_COLUMNS: &str = "id, owner_id, parent_id, name, sha256, size, chunk_size, \
     chunk_count, status, created_at, expires_at";
const STATUS_OPEN: &str = "open";

#[derive(Debug, Clone, FromRow)]
pub struct UploadRecord {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: String,
    pub sha256: String,
    pub size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    status: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

impl UploadRecord {
    /// `false` while a request is completing the upload.
    pub fn is_open(&self) -> bool {
        self.status == STATUS_OPEN
    }
}

#[derive(Debug, Clone)]
pub struct NewUpload<'a> {
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: &'a NodeName,
    pub sha256: &'a ContentHash,
    pub size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    pub expires_at: OffsetDateTime,
}

/// Opens a session after checking the destination and that the file fits the
/// quota together with the owner's other open sessions.
pub async fn create(pool: &PgPool, new: &NewUpload<'_>) -> Result<UploadRecord, DbError> {
    let mut tx = pool.begin().await?;
    let quota_bytes = quota::lock_owner(&mut tx, new.owner_id).await?;
    find_live_folder(&mut *tx, new.owner_id, new.parent_id)
        .await?
        .ok_or(DbError::NotFound)?;
    quota::ensure_room(&mut tx, new.owner_id, quota_bytes, new.size, Pending::Reserve).await?;
    let sql = format!(
        "INSERT INTO upload_sessions \
           (id, owner_id, parent_id, name, sha256, size, chunk_size, chunk_count, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {UPLOAD_COLUMNS}"
    );
    let record = sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.owner_id)
        .bind(new.parent_id)
        .bind(new.name.as_str())
        .bind(new.sha256.as_str())
        .bind(new.size)
        .bind(new.chunk_size)
        .bind(new.chunk_count)
        .bind(new.expires_at)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(record)
}

/// The owner's newest open, unexpired session for exactly this file and destination.
pub async fn find_resumable(
    pool: &PgPool,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
    sha256: &ContentHash,
    size: i64,
) -> Result<Option<UploadRecord>, DbError> {
    let sql = format!(
        "SELECT {UPLOAD_COLUMNS} FROM upload_sessions \
         WHERE owner_id = $1 AND parent_id = $2 AND name = $3 AND sha256 = $4 AND size = $5 \
           AND status = 'open' AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1"
    );
    Ok(sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(owner_id)
        .bind(parent_id)
        .bind(name.as_str())
        .bind(sha256.as_str())
        .bind(size)
        .fetch_optional(pool)
        .await?)
}

/// An unexpired session of the owner, in any state.
pub async fn find(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<Option<UploadRecord>, DbError> {
    let sql = format!(
        "SELECT {UPLOAD_COLUMNS} FROM upload_sessions \
         WHERE id = $1 AND owner_id = $2 AND expires_at > now()"
    );
    Ok(sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn received_chunks(pool: &PgPool, upload_id: Uuid) -> Result<Vec<i32>, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT chunk_index FROM upload_chunks WHERE upload_id = $1 ORDER BY chunk_index",
    )
    .bind(upload_id)
    .fetch_all(pool)
    .await?)
}

/// Marks a chunk as received and pushes the expiry back. `false` when the
/// session is gone, expired or being completed.
pub async fn record_chunk(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    index: i32,
    expires_at: OffsetDateTime,
) -> Result<bool, DbError> {
    let mut tx = pool.begin().await?;
    let open: Option<Uuid> = sqlx::query_scalar(
        "UPDATE upload_sessions SET expires_at = $3 \
         WHERE id = $1 AND owner_id = $2 AND status = 'open' AND expires_at > now() RETURNING id",
    )
    .bind(id)
    .bind(owner_id)
    .bind(expires_at)
    .fetch_optional(&mut *tx)
    .await?;
    if open.is_none() {
        return Ok(false);
    }
    sqlx::query("INSERT INTO upload_chunks (upload_id, chunk_index) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(index)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Moves an open session to `completing`, so concurrent completes, chunk writes
/// and aborts are refused. `NotFound` or `UploadBusy` otherwise.
pub async fn claim(
    pool: &PgPool,
    owner_id: Uuid,
    id: Uuid,
    expires_at: OffsetDateTime,
) -> Result<UploadRecord, DbError> {
    let sql = format!(
        "UPDATE upload_sessions SET status = 'completing', expires_at = $3 \
         WHERE id = $1 AND owner_id = $2 AND status = 'open' AND expires_at > now() \
         RETURNING {UPLOAD_COLUMNS}"
    );
    let claimed = sqlx::query_as::<_, UploadRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .bind(expires_at)
        .fetch_optional(pool)
        .await?;
    match claimed {
        Some(record) => Ok(record),
        None if find(pool, owner_id, id).await?.is_some() => Err(DbError::UploadBusy),
        None => Err(DbError::NotFound),
    }
}

/// Reopens a claimed session (after a failure the client can fix and retry).
pub async fn release(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE upload_sessions SET status = 'open' WHERE id = $1 AND status = 'completing'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes a session in any state; accepts a transaction (completion).
pub async fn delete<'e>(executor: impl PgExecutor<'e>, id: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM upload_sessions WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Aborts an open session of the owner; `UploadBusy` while it is completing.
pub async fn abort(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let deleted: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM upload_sessions WHERE id = $1 AND owner_id = $2 AND status = 'open' RETURNING id",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?;
    match deleted {
        Some(_) => Ok(()),
        None if find(pool, owner_id, id).await?.is_some() => Err(DbError::UploadBusy),
        None => Err(DbError::NotFound),
    }
}

/// Deletes expired sessions (any state) and returns their ids.
pub async fn delete_expired(pool: &PgPool) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar("DELETE FROM upload_sessions WHERE expires_at <= now() RETURNING id")
        .fetch_all(pool)
        .await?)
}

/// The subset of `ids` that still has a session (to find orphaned files).
pub async fn existing(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar("SELECT id FROM upload_sessions WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await?)
}
```
**Step 4: Run tests** — `cargo test -p squirrel-db --test uploads` → PASS (7 tests).

**Step 5: Commit**
```bash
git add crates/squirrel-db
git commit -m "feat(db): add upload sessions with chunk tracking" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Trash repository

**Files:**
- Create: `crates/squirrel-db/src/trash.rs`, `crates/squirrel-db/tests/trash.rs`
- Modify: `crates/squirrel-db/src/lib.rs` (`pub mod trash;`)

**Step 1: Write the failing tests** — `crates/squirrel-db/tests/trash.rs`:

```rust
mod common;

use sqlx::PgPool;
use squirrel_db::nodes;
use squirrel_db::{DbError, blobs, quota, trash};
use time::{Duration, OffsetDateTime};

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashing_a_folder_trashes_its_subtree_as_one_entry(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let inner = common::raw_folder(&pool, alice.id, folder, "inner").await;
    let a = common::raw_file(&pool, alice.id, folder, "a.txt", 'a', 10).await;
    let b = common::raw_file(&pool, alice.id, inner, "b.txt", 'b', 5).await;

    trash::trash(&pool, alice.id, folder).await.unwrap();

    let entries = trash::list(&pool, alice.id, None, 10).await.unwrap();
    assert_eq!(entries.items.iter().map(|n| n.id).collect::<Vec<_>>(), [folder]);
    for id in [folder, inner, a, b] {
        assert!(nodes::find_live(&pool, alice.id, id).await.unwrap().is_none());
    }
    let usage = quota::usage(&pool, alice.id).await.unwrap();
    assert_eq!((usage.used_bytes, usage.trash_bytes), (0, 15));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn roots_cannot_be_trashed(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    assert!(matches!(trash::trash(&pool, alice.id, files.id).await, Err(DbError::ProtectedRoot)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_returns_the_batch_where_it_was(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let a = common::raw_file(&pool, alice.id, folder, "a.txt", 'a', 10).await;
    trash::trash(&pool, alice.id, folder).await.unwrap();

    let restored = trash::restore(&pool, alice.id, folder).await.unwrap();
    assert_eq!((restored.name.as_str(), restored.parent_id), ("dir", Some(files.id)));
    assert!(nodes::find_live(&pool, alice.id, a).await.unwrap().is_some());
    assert!(trash::list(&pool, alice.id, None, 10).await.unwrap().items.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_renames_on_conflict_and_falls_back_to_files(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let docs = common::root(&pool, alice.id, "Documents").await;
    let folder = common::raw_folder(&pool, alice.id, docs.id, "dir").await;
    let note = common::raw_file(&pool, alice.id, folder, "note.txt", 'n', 1).await;
    trash::trash(&pool, alice.id, note).await.unwrap();
    trash::trash(&pool, alice.id, folder).await.unwrap();
    common::raw_file(&pool, alice.id, files.id, "note.txt", 'm', 1).await;

    // The note's folder is in the trash: it goes to Files/, renamed.
    let restored = trash::restore(&pool, alice.id, note).await.unwrap();
    assert_eq!((restored.name.as_str(), restored.parent_id), ("note (1).txt", Some(files.id)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn earlier_trashed_children_stay_in_the_trash(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let folder = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let note = common::raw_file(&pool, alice.id, folder, "note.txt", 'n', 1).await;
    trash::trash(&pool, alice.id, note).await.unwrap();
    trash::trash(&pool, alice.id, folder).await.unwrap();
    trash::restore(&pool, alice.id, folder).await.unwrap();
    assert!(nodes::find_live(&pool, alice.id, note).await.unwrap().is_none());
    assert_eq!(trash::list(&pool, alice.id, None, 10).await.unwrap().items.len(), 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_respects_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let old = common::raw_file(&pool, alice.id, files.id, "old.bin", 'o', 60).await;
    trash::trash(&pool, alice.id, old).await.unwrap();
    common::raw_file(&pool, alice.id, files.id, "new.bin", 'n', 60).await;
    common::set_quota(&pool, alice.id, Some(100)).await;
    assert!(matches!(trash::restore(&pool, alice.id, old).await, Err(DbError::QuotaExceeded)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_forever_and_empty_release_blobs(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    let b = common::raw_file(&pool, alice.id, files.id, "b.txt", 'b', 1).await;
    trash::trash(&pool, alice.id, a).await.unwrap();
    trash::trash(&pool, alice.id, b).await.unwrap();

    trash::delete_forever(&pool, alice.id, a).await.unwrap();
    assert!(matches!(trash::delete_forever(&pool, alice.id, a).await, Err(DbError::NotFound)));
    assert_eq!(trash::empty(&pool, alice.id).await.unwrap(), 1);
    for fill in ['a', 'b'] {
        assert_eq!(blobs::find(&pool, &common::hash(fill)).await.unwrap().unwrap().ref_count, 0);
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn purge_only_removes_entries_older_than_the_cutoff(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let old = common::raw_file(&pool, alice.id, files.id, "old.txt", 'o', 1).await;
    let recent = common::raw_file(&pool, alice.id, files.id, "recent.txt", 'r', 1).await;
    trash::trash(&pool, alice.id, old).await.unwrap();
    trash::trash(&pool, alice.id, recent).await.unwrap();
    sqlx::query("UPDATE nodes SET deleted_at = now() - interval '31 days' WHERE id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();

    let cutoff = OffsetDateTime::now_utc() - Duration::days(30);
    assert_eq!(trash::purge_older_than(&pool, cutoff, 100).await.unwrap(), 1);
    let left = trash::list(&pool, alice.id, None, 10).await.unwrap();
    assert_eq!(left.items.iter().map(|n| n.id).collect::<Vec<_>>(), [recent]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_is_private(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let files = common::root(&pool, alice.id, "Files").await;
    let a = common::raw_file(&pool, alice.id, files.id, "a.txt", 'a', 1).await;
    assert!(matches!(trash::trash(&pool, bob.id, a).await, Err(DbError::NotFound)));
    trash::trash(&pool, alice.id, a).await.unwrap();
    assert!(trash::list(&pool, bob.id, None, 10).await.unwrap().items.is_empty());
    assert!(matches!(trash::restore(&pool, bob.id, a).await, Err(DbError::NotFound)));
    assert!(matches!(trash::delete_forever(&pool, bob.id, a).await, Err(DbError::NotFound)));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_listing_paginates_newest_first(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let mut ids = Vec::new();
    for (i, fill) in ['a', 'b', 'c'].into_iter().enumerate() {
        let id = common::raw_file(&pool, alice.id, files.id, &format!("{i}.txt"), fill, 1).await;
        trash::trash(&pool, alice.id, id).await.unwrap();
        ids.push(id);
    }
    let first = trash::list(&pool, alice.id, None, 2).await.unwrap();
    assert!(first.has_more);
    let last = first.items.last().unwrap();
    let cursor = trash::TrashCursor { deleted_at: last.deleted_at.unwrap(), id: last.id };
    let second = trash::list(&pool, alice.id, Some(&cursor), 2).await.unwrap();
    let mut all: Vec<_> = first.items.iter().chain(&second.items).map(|n| n.id).collect();
    all.sort();
    ids.sort();
    assert_eq!(all, ids);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-db --test trash` → FAIL.

**Step 3: Implement** — `src/trash.rs`:
```rust
//! The trash. Deleting marks a node and its live subtree with
//! `trash_root = <node id>`; a trash entry is a node where `trash_root = id`.

use sqlx::{PgConnection, PgPool};
use squirrel_core::NodeName;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::nodes::{self, NODE_COLUMNS, NodeRecord, RESTORE_FALLBACK_ROOT};
use crate::quota::{self, Pending};
use crate::{DbError, Page};

/// Where a trash listing resumes (entries are ordered by `deleted_at DESC, id DESC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashCursor {
    pub deleted_at: OffsetDateTime,
    pub id: Uuid,
}

/// Moves a live, non-root node and its live subtree to the trash as one entry.
pub async fn trash(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    quota::lock_owner(&mut tx, owner_id).await?;
    let node = nodes::find_live(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    if node.is_root() {
        return Err(DbError::ProtectedRoot);
    }
    sqlx::query(
        "WITH RECURSIVE subtree (id) AS ( \
             SELECT $1::uuid \
           UNION \
             SELECT n.id FROM nodes n JOIN subtree s ON n.parent_id = s.id \
             WHERE n.deleted_at IS NULL \
         ) \
         UPDATE nodes SET deleted_at = now(), trash_root = $1 \
         WHERE owner_id = $2 AND deleted_at IS NULL AND id IN (SELECT id FROM subtree)",
    )
    .bind(id)
    .bind(owner_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Trash entries (what the user deleted, not their contents), newest first.
pub async fn list(
    pool: &PgPool,
    owner_id: Uuid,
    after: Option<&TrashCursor>,
    limit: u32,
) -> Result<Page<NodeRecord>, DbError> {
    let sql = format!(
        "SELECT {NODE_COLUMNS} FROM nodes \
         WHERE owner_id = $1 AND trash_root = id \
           AND ($2::timestamptz IS NULL OR (deleted_at, id) < ($2, $3)) \
         ORDER BY deleted_at DESC, id DESC LIMIT $4"
    );
    let rows = sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(owner_id)
        .bind(after.map(|c| c.deleted_at))
        .bind(after.map(|c| c.id))
        .bind(i64::from(limit) + 1)
        .fetch_all(pool)
        .await?;
    Ok(Page::from_overfetch(rows, limit))
}

/// Restores an entry with its whole batch: back into its folder if that is
/// live, else into `Files/`; a clashing name becomes "name (n)". The batch
/// must fit the quota.
pub async fn restore(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<NodeRecord, DbError> {
    let mut tx = pool.begin().await?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await?;
    let entry = find_entry(&mut tx, owner_id, id).await?.ok_or(DbError::NotFound)?;
    let batch_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::bigint FROM nodes \
         WHERE owner_id = $1 AND trash_root = $2 AND kind = 'file'",
    )
    .bind(owner_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    quota::ensure_room(&mut tx, owner_id, quota_bytes, batch_bytes, Pending::Ignore).await?;

    let parent_id = restore_target(&mut tx, owner_id, entry.parent_id).await?;
    let stored = NodeName::parse(&entry.name)
        .map_err(|err| DbError::Sqlx(sqlx::Error::Decode(Box::new(err))))?;
    let name = nodes::free_name(&mut tx, owner_id, parent_id, &stored).await?;
    sqlx::query(
        "UPDATE nodes SET deleted_at = NULL, trash_root = NULL, \
           parent_id = CASE WHEN id = $2 THEN $3 ELSE parent_id END, \
           name = CASE WHEN id = $2 THEN $4 ELSE name END \
         WHERE owner_id = $1 AND trash_root = $2",
    )
    .bind(owner_id)
    .bind(id)
    .bind(parent_id)
    .bind(name.as_str())
    .execute(&mut *tx)
    .await?;
    let node = nodes::find_live(&mut *tx, owner_id, id)
        .await?
        .ok_or(DbError::NotFound)?;
    tx.commit().await?;
    Ok(node)
}

async fn find_entry(conn: &mut PgConnection, owner_id: Uuid, id: Uuid) -> Result<Option<NodeRecord>, DbError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = $1 AND owner_id = $2 AND trash_root = id");
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(conn)
        .await?)
}

/// The original folder if it is still live, else the owner's `Files/` root.
async fn restore_target(conn: &mut PgConnection, owner_id: Uuid, original: Option<Uuid>) -> Result<Uuid, DbError> {
    if let Some(parent) = original {
        if nodes::find_live_folder(&mut *conn, owner_id, parent).await?.is_some() {
            return Ok(parent);
        }
    }
    sqlx::query_scalar(
        "SELECT id FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND name = $2 AND deleted_at IS NULL",
    )
    .bind(owner_id)
    .bind(RESTORE_FALLBACK_ROOT)
    .fetch_optional(conn)
    .await?
    .ok_or(DbError::NotFound)
}

/// Deletes an entry and everything below it for good. Blob references drop
/// through the trigger; blob GC removes the files later.
pub async fn delete_forever(pool: &PgPool, owner_id: Uuid, id: Uuid) -> Result<(), DbError> {
    let result = sqlx::query("DELETE FROM nodes WHERE id = $1 AND owner_id = $2 AND trash_root = id")
        .bind(id)
        .bind(owner_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}

/// Deletes every trash entry of the owner; returns how many entries went.
pub async fn empty(pool: &PgPool, owner_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM nodes WHERE owner_id = $1 AND trash_root = id")
        .bind(owner_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Deletes entries trashed before `cutoff`, `batch` at a time. The outer
/// `trash_root = id` re-check matters: a row restored while the statement waits
/// on its lock no longer matches and survives.
pub async fn purge_older_than(pool: &PgPool, cutoff: OffsetDateTime, batch: i64) -> Result<u64, DbError> {
    let mut total = 0;
    loop {
        let purged = sqlx::query(
            "DELETE FROM nodes WHERE trash_root = id AND deleted_at < $1 AND id IN ( \
                 SELECT id FROM nodes WHERE trash_root = id AND deleted_at < $1 LIMIT $2)",
        )
        .bind(cutoff)
        .bind(batch)
        .execute(pool)
        .await?
        .rows_affected();
        total += purged;
        if purged < u64::try_from(batch).unwrap_or(u64::MAX) {
            return Ok(total);
        }
    }
}
```

**Step 4: Run tests** — `cargo test -p squirrel-db` → PASS (10 new). Then the full Rust gate.

**Step 5: Commit**
```bash
git add crates/squirrel-db
git commit -m "feat(db): add trash with restore, permanent delete and purge" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Milestone C — Upload API

### Task 11: Wiring — data dir, `BlobStore` in state, transfer routes, errors, test harness

**Files:**
- Modify: `crates/squirrel-api/Cargo.toml`, `src/state.rs`, `src/error.rs`, `src/blocking.rs`, `src/input.rs`, `src/lib.rs`, `src/security_headers.rs`
- Modify: `crates/squirrel-api/tests/common/mod.rs`
- Modify: `crates/squirrel/Cargo.toml`, `crates/squirrel/src/main.rs`

**Step 1: Dependencies**

`crates/squirrel-api/Cargo.toml`:
```toml
[dependencies]
# … existing …
base64.workspace = true
http-range-header.workspace = true
percent-encoding.workspace = true
squirrel-storage.workspace = true
tokio-util.workspace = true

[dev-dependencies]
axum-test.workspace = true
sha2.workspace = true
tempfile.workspace = true
```
`crates/squirrel/Cargo.toml`: add `squirrel-storage.workspace = true`.

**Step 2: Write the failing tests** — bottom of `crates/squirrel-api/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_storage::StorageError;

    #[test]
    fn storage_failures_are_507_with_distinct_codes() {
        assert_eq!(
            ApiError::from(StorageError::NoSpace).status_and_code(),
            (StatusCode::INSUFFICIENT_STORAGE, "storage_full")
        );
        assert_eq!(
            ApiError::from(DbError::QuotaExceeded).status_and_code(),
            (StatusCode::INSUFFICIENT_STORAGE, "quota_exceeded")
        );
    }

    #[test]
    fn upload_failures_have_stable_codes() {
        assert_eq!(ApiError::HashMismatch.status_and_code(), (StatusCode::UNPROCESSABLE_ENTITY, "hash_mismatch"));
        assert_eq!(
            ApiError::UploadIncomplete("x".into()).status_and_code(),
            (StatusCode::CONFLICT, "upload_incomplete")
        );
        assert_eq!(ApiError::from(DbError::NameConflict).status_and_code(), (StatusCode::CONFLICT, "name_conflict"));
        assert_eq!(
            ApiError::RangeNotSatisfiable.status_and_code(),
            (StatusCode::RANGE_NOT_SATISFIABLE, "range_not_satisfiable")
        );
    }

    #[test]
    fn a_missing_blob_or_upload_file_is_not_found() {
        assert_eq!(ApiError::from(StorageError::NotFound).status_and_code().0, StatusCode::NOT_FOUND);
    }
}
```
Run `cargo test -p squirrel-api --lib` → FAIL (unknown variants).

**Step 3: Implement**

`src/error.rs` — new variants and their codes:
```rust
    #[error("the server is out of disk space")]
    StorageFull,
    #[error("the uploaded content does not match its SHA-256; upload it again")]
    HashMismatch,
    #[error("{0}")]
    UploadIncomplete(String),
    #[error("the requested range is not satisfiable")]
    RangeNotSatisfiable,
```
```rust
            ApiError::StorageFull => (StatusCode::INSUFFICIENT_STORAGE, "storage_full"),
            ApiError::HashMismatch => (StatusCode::UNPROCESSABLE_ENTITY, "hash_mismatch"),
            ApiError::UploadIncomplete(_) => (StatusCode::CONFLICT, "upload_incomplete"),
            ApiError::RangeNotSatisfiable => (StatusCode::RANGE_NOT_SATISFIABLE, "range_not_satisfiable"),
```
```rust
impl From<StorageError> for ApiError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::NoSpace => {
                tracing::error!("disk full while writing to the data directory");
                ApiError::StorageFull
            }
            StorageError::NotFound => ApiError::NotFound,
            other => ApiError::Internal(other.into()),
        }
    }
}
```
Also add `NodeNameError` and `ContentHashError` to the `validation_from!` list (both become 422 with their message).

`src/blocking.rs` — add:
```rust
/// Runs blocking filesystem work on the blocking pool and maps its errors.
pub async fn run_storage<T, F>(work: F) -> Result<T, ApiError>
where
    F: FnOnce() -> squirrel_storage::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| ApiError::Internal(err.into()))?
        .map_err(ApiError::from)
}
```

`src/input.rs` — add two extractors:
```rust
use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;

/// Query string; a malformed value is a 400 envelope.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    axum::extract::Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|_| ApiError::BadRequest("invalid query parameter".into()))
    }
}

/// Raw request body (upload chunks). Over the route's body limit → 413 envelope.
pub struct ApiBytes(pub Bytes);

impl<S: Send + Sync> FromRequest<S> for ApiBytes {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        Bytes::from_request(req, state).await.map(Self).map_err(|rejection| {
            if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::PayloadTooLarge
            } else {
                ApiError::BadRequest("the request body could not be read".into())
            }
        })
    }
}
```

`src/state.rs` — new config fields, the store in the state:
```rust
pub struct ApiConfig {
    // … existing fields …
    /// Chunk size announced to clients. Every chunk but the last has exactly this size.
    pub upload_chunk_size: u64,
    /// Upload sessions expire after this long without a chunk.
    pub upload_ttl: Duration,
    /// Uploads are refused when they would leave less than this free on the data disk.
    pub min_free_bytes: u64,
    /// Trash entries older than this are deleted for good.
    pub trash_retention: Duration,
    /// Blobs are deleted only after this long without any reference.
    pub blob_gc_grace: Duration,
}
// Default:
            upload_chunk_size: 8 * 1024 * 1024,
            upload_ttl: Duration::hours(24),
            min_free_bytes: 1024 * 1024 * 1024,
            trash_retention: Duration::days(30),
            blob_gc_grace: Duration::hours(24),

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub storage: BlobStore,
    pub config: Arc<ApiConfig>,
    pub hash_permits: Arc<Semaphore>,
}

impl AppState {
    pub fn new(pool: PgPool, storage: BlobStore, config: ApiConfig) -> Self {
        Self {
            pool,
            storage,
            hash_permits: Arc::new(Semaphore::new(config.hash_concurrency)),
            config: Arc::new(config),
        }
    }
}
```

`src/security_headers.rs` — `no_store_for_api` keeps a `Cache-Control` the handler chose (file content revalidates with ETags):
```rust
    let (mut parts, body) = response.into_parts();
    if !parts.headers.contains_key(CACHE_CONTROL) {
        parts.headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    Response::from_parts(parts, body)
```
(Update its doc comment: "API responses are never cached unless the handler set its own policy.")

`src/lib.rs` — split the route groups so transfer routes escape the 30 s timeout:
```rust
/// Room for framing beyond one chunk in a chunk upload body.
const CHUNK_BODY_MARGIN: usize = 64 * 1024;
/// One chunk (8 MiB by default) must arrive within this time (~28 KB/s at
/// worst): long enough for slow links, short enough to bound slow-loris bodies.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Builds every `/api` route. Routes are grouped by the limits they need:
///
/// - [`json_routes`]: ordinary JSON endpoints (64 KiB body, 30 s timeout). The default.
/// - [`credential_routes`]: passwords and one-time secrets (16 KiB body, 30 s timeout, rate limits).
/// - [`transfer_routes`]: upload chunks, upload completion and file downloads. No
///   30 s timeout (hashing or streaming a large file takes longer); chunks get a
///   body limit of one chunk and their own timeout.
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let timed = json_routes()
        .merge(credential_routes(config))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, API_TIMEOUT));
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(timed)
        .merge(transfer_routes(config))
}

/// Uploads and downloads. Still session- and CSRF-protected by their extractors.
fn transfer_routes(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let chunk_limit = usize::try_from(config.upload_chunk_size)
        .unwrap_or(usize::MAX)
        .saturating_add(CHUNK_BODY_MARGIN);
    let chunks = OpenApiRouter::new()
        // Task 13: .routes(routes!(routes::uploads::put_chunk))
        .layer(DefaultBodyLimit::max(chunk_limit))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, CHUNK_TIMEOUT));
    let long_running = OpenApiRouter::new()
        // Task 14: complete_upload; Task 19: file_content
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
    chunks.merge(long_running)
}
```
Add tags to `ApiDoc`: `(name = "files"), (name = "uploads"), (name = "trash"), (name = "storage")`.

`crates/squirrel/src/main.rs` — the data directory is required for `serve`:
```rust
    /// Where blobs and in-progress uploads live (created if missing). `tmp/` and
    /// `blobs/` inside it must be on the same filesystem.
    #[arg(long, env = "SQUIRREL_DATA_DIR")]
    data_dir: PathBuf,
```
```rust
async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let pool = connect_and_migrate(&args.database).await?;
    let storage = BlobStore::open(&args.data_dir)
        .with_context(|| format!("opening data directory {}", args.data_dir.display()))?;
    // … config unchanged …
    let state = AppState::new(pool.clone(), storage, config);
    let app = squirrel_api::router(state.clone()).layer(TraceLayer::new_for_http());
```
(`use std::path::PathBuf; use squirrel_storage::BlobStore;`.)

`crates/squirrel-api/tests/common/mod.rs` — every test instance gets its own temporary data directory, kept alive by the router:
```rust
use std::path::Path;
use std::sync::Arc;

use squirrel_storage::BlobStore;
use tempfile::TempDir;

/// Small chunks keep multi-chunk tests fast.
pub const TEST_CHUNK_SIZE: u64 = 1024;

pub fn test_config() -> ApiConfig {
    ApiConfig {
        cookie_secure: false,
        login_rate_limit: None,
        join_check_rate_limit: None,
        setup_token: Some(SetupToken::new(SETUP_TOKEN)),
        upload_chunk_size: TEST_CHUNK_SIZE,
        min_free_bytes: 0,
        ..ApiConfig::default()
    }
}

/// One server: database, data directory and state. Each [`Instance::browser`]
/// has its own cookie jar but shares the storage, like real browsers.
pub struct Instance {
    pub state: AppState,
    data: Arc<TempDir>,
}

impl Instance {
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, test_config())
    }

    pub fn with_config(pool: PgPool, config: ApiConfig) -> Self {
        let data = Arc::new(tempfile::tempdir().expect("temporary data dir"));
        let storage = BlobStore::open(data.path()).expect("blob store");
        Self { state: AppState::new(pool, storage, config), data }
    }

    pub fn data_dir(&self) -> &Path {
        self.data.path()
    }

    /// A real `ConnectInfo` is inserted because tower_governor's key extractor
    /// only reads `ConnectInfo<SocketAddr>`. The router holds the data directory,
    /// so it lives as long as any browser does.
    pub fn browser(&self) -> TestServer {
        let app = squirrel_api::router(self.state.clone())
            .layer(Extension(self.data.clone()))
            .layer(Extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000)))));
        TestServer::builder()
            .save_cookies()
            .build(app)
            .expect("test server builds")
    }
}

pub fn server(pool: PgPool) -> TestServer {
    Instance::new(pool).browser()
}

pub fn server_with(pool: PgPool, config: ApiConfig) -> TestServer {
    Instance::with_config(pool, config).browser()
}

/// Alice (admin) and Bob (invited by her), each in their own browser.
pub async fn two_users(instance: &Instance) -> ((TestServer, Value), (TestServer, Value)) {
    let alice = instance.browser();
    let alice_me = setup_admin(&alice, "alice").await;
    let created: Value = alice
        .post("/api/invitations")
        .add_header(CSRF, csrf(&alice_me))
        .json(&json!({}))
        .await
        .json();
    let token = created["token"].as_str().expect("token");
    let bob = instance.browser();
    let joined = bob.post(&format!("/api/join/{token}")).json(&account("bob")).await;
    joined.assert_status(StatusCode::CREATED);
    let bob_me = joined.json();
    ((alice, alice_me), (bob, bob_me))
}
```

**Step 4: Run tests** — `cargo test -p squirrel-api` → PASS (3 new unit tests; every Phase 1 test unchanged). `cargo build -p squirrel` → OK. Regenerate `openapi.json` (tags changed) and `pnpm gen:api`; `cargo test --workspace` → PASS.

**Step 5: Commit**
```bash
git add Cargo.lock crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): wire the blob store, data dir and transfer route group" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: `POST /api/uploads`, `GET`/`DELETE /api/uploads/{id}`

**Files:**
- Create: `crates/squirrel-api/src/files_dto.rs`, `src/routes/uploads/mod.rs`, `src/routes/uploads/create.rs`
- Create: `crates/squirrel-api/tests/common/files.rs`, `crates/squirrel-api/tests/uploads.rs`
- Modify: `src/lib.rs` (`mod files_dto;`, routes), `src/routes/mod.rs` (`pub mod uploads;`), `tests/common/mod.rs` (`pub mod files;`)

**Step 1: Test helpers** — `crates/squirrel-api/tests/common/files.rs`:
```rust
use axum::body::Bytes;
use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use squirrel_db::nodes;
use uuid::Uuid;

use super::{CSRF, Instance, csrf};

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn user_id(me: &Value) -> Uuid {
    me["id"].as_str().expect("id").parse().expect("uuid")
}

/// One of the user's top-level folders, read from the database.
pub async fn root_id(instance: &Instance, me: &Value, name: &str) -> Uuid {
    nodes::list_children(&instance.state.pool, user_id(me), None, None, 10)
        .await
        .expect("roots")
        .items
        .into_iter()
        .find(|node| node.name == name)
        .expect("root exists")
        .id
}

pub async fn start_upload(browser: &TestServer, me: &Value, parent: Uuid, name: &str, content: &[u8]) -> TestResponse {
    browser
        .post("/api/uploads")
        .add_header(CSRF, csrf(me))
        .json(&json!({
            "parent_id": parent,
            "name": name,
            "size": content.len(),
            "sha256": sha256_hex(content),
        }))
        .await
}

pub async fn put_chunk(browser: &TestServer, me: &Value, upload_id: &str, index: usize, bytes: &[u8]) -> TestResponse {
    browser
        .put(&format!("/api/uploads/{upload_id}/chunks/{index}"))
        .add_header(CSRF, csrf(me))
        .bytes(Bytes::copy_from_slice(bytes))
        .await
}

pub async fn complete(browser: &TestServer, me: &Value, upload_id: &str) -> TestResponse {
    browser
        .post(&format!("/api/uploads/{upload_id}/complete"))
        .add_header(CSRF, csrf(me))
        .await
}

/// Runs the whole protocol (or takes the dedup shortcut) and returns the node.
pub async fn upload(browser: &TestServer, me: &Value, parent: Uuid, name: &str, content: &[u8]) -> Value {
    let started = start_upload(browser, me, parent, name, content).await;
    if started.status_code() == StatusCode::OK {
        return started.json::<Value>()["node"].clone();
    }
    started.assert_status(StatusCode::CREATED);
    let session: Value = started.json();
    let id = session["upload_id"].as_str().expect("upload_id").to_owned();
    let chunk_size = usize::try_from(session["chunk_size"].as_u64().expect("chunk_size")).unwrap();
    for (index, chunk) in content.chunks(chunk_size).enumerate() {
        put_chunk(browser, me, &id, index, chunk).await.assert_status(StatusCode::NO_CONTENT);
    }
    let done = complete(browser, me, &id).await;
    done.assert_status(StatusCode::CREATED);
    done.json()
}

/// Error code of an envelope response.
pub fn code(response: &TestResponse) -> String {
    response.json::<Value>()["error"]["code"].as_str().unwrap_or_default().to_owned()
}
```
(`put_chunk`, `complete` and `upload` are exercised from Tasks 13–14; `#![allow(dead_code)]` in `common/mod.rs` covers them meanwhile.)

**Step 2: Write the failing tests** — `crates/squirrel-api/tests/uploads.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_core::Username;
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creating_an_upload_opens_a_session_and_a_sparse_file(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;

    let response = files::start_upload(&browser, &me, parent, "big.bin", &[7; 2500]).await;
    response.assert_status(StatusCode::CREATED);
    let session: Value = response.json();
    assert_eq!(session["chunk_size"], common::TEST_CHUNK_SIZE);
    assert_eq!(session["chunk_count"], 3);
    assert_eq!(session["received_chunks"], json!([]));
    let file = instance
        .data_dir()
        .join("tmp/uploads")
        .join(session["upload_id"].as_str().unwrap());
    assert_eq!(std::fs::metadata(file).unwrap().len(), 2500);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_same_file_resumes_the_same_session(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let first: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await.json();
    let second: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await.json();
    assert_eq!(first["upload_id"], second["upload_id"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_requests_are_rejected(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;

    let bad_name = files::start_upload(&browser, &me, parent, "a/b", b"x").await;
    bad_name.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let bad_hash = browser
        .post("/api/uploads")
        .add_header(common::CSRF, common::csrf(&me))
        .json(&json!({ "parent_id": parent, "name": "a", "size": 1, "sha256": "ABC" }))
        .await;
    bad_hash.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let unknown_parent = files::start_upload(&browser, &me, Uuid::now_v7(), "a", b"x").await;
    unknown_parent.assert_status(StatusCode::NOT_FOUND);
    let no_csrf = browser
        .post("/api/uploads")
        .json(&json!({ "parent_id": parent, "name": "a", "size": 1, "sha256": files::sha256_hex(b"x") }))
        .await;
    no_csrf.assert_status(StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn quota_is_checked_before_any_byte_is_sent(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100))
        .await
        .unwrap();
    let response = files::start_upload(&browser, &me, parent, "big.bin", &[0; 101]).await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&response), "quota_exceeded");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn uploads_that_would_fill_the_disk_are_refused(pool: PgPool) {
    let config = squirrel_api::ApiConfig { min_free_bytes: u64::MAX, ..common::test_config() };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let response = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&response), "storage_full");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn status_and_abort(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await.json();
    let id = session["upload_id"].as_str().unwrap();

    let status: Value = browser.get(&format!("/api/uploads/{id}")).await.json();
    assert_eq!(status["upload_id"], id);
    browser
        .delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    browser.get(&format!("/api/uploads/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    assert!(!instance.data_dir().join("tmp/uploads").join(id).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn uploads_and_folders_are_private(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alices_files = files::root_id(&instance, &alice_me, "Files").await;

    files::start_upload(&bob, &bob_me, alices_files, "a.bin", b"abc")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let session: Value = files::start_upload(&alice, &alice_me, alices_files, "a.bin", b"abc").await.json();
    let id = session["upload_id"].as_str().unwrap();
    bob.get(&format!("/api/uploads/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    bob.delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
```

**Step 3: Run to verify failure** — `cargo test -p squirrel-api --test uploads` → FAIL (404 on `/api/uploads`).

**Step 4: Implement**

`src/files_dto.rs`:
```rust
//! Request and response bodies for files, uploads and the trash.

use serde::{Deserialize, Serialize};
use squirrel_core::NodeKind;
use squirrel_db::nodes::NodeRecord;
use squirrel_db::uploads::UploadRecord;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeDto {
    pub id: Uuid,
    /// `null` for the top-level folders.
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub kind: NodeKind,
    /// Bytes; 0 for folders.
    pub size: i64,
    /// Detected from the content at upload; `null` for folders.
    pub mime: Option<String>,
    /// Top-level folders cannot be renamed, moved or deleted.
    pub is_root: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub modified_at: OffsetDateTime,
}

impl From<&NodeRecord> for NodeDto {
    fn from(node: &NodeRecord) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent_id,
            name: node.name.clone(),
            kind: node.kind(),
            size: node.size,
            mime: node.mime.clone(),
            is_root: node.is_root(),
            created_at: node.created_at,
            modified_at: node.modified_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUploadRequest {
    /// Destination folder.
    pub parent_id: Uuid,
    pub name: String,
    /// Total size in bytes.
    pub size: u64,
    /// SHA-256 of the whole file, 64 lowercase hex characters.
    pub sha256: String,
}

/// An upload session: send the missing chunks, then complete it.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadSessionDto {
    pub upload_id: Uuid,
    pub chunk_size: u64,
    pub chunk_count: u32,
    /// Chunk indexes already stored, ascending.
    pub received_chunks: Vec<u32>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

impl UploadSessionDto {
    pub fn new(record: &UploadRecord, received: &[i32]) -> Self {
        Self {
            upload_id: record.id,
            chunk_size: u64::try_from(record.chunk_size).unwrap_or_default(),
            chunk_count: u32::try_from(record.chunk_count).unwrap_or_default(),
            received_chunks: received.iter().filter_map(|&i| u32::try_from(i).ok()).collect(),
            expires_at: record.expires_at,
        }
    }
}

/// The content was already in the caller's library: a node was created without
/// uploading anything.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeduplicatedUpload {
    /// Always `true`.
    pub deduplicated: bool,
    pub node: NodeDto,
}
```

`src/routes/uploads/create.rs`:
```rust
use squirrel_core::{ContentHash, NodeName, UploadPlan};
use squirrel_db::nodes::{self, NewFile, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::uploads::{self, NewUpload, UploadRecord};
use squirrel_db::{DbError, blobs};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::files_dto::CreateUploadRequest;
use crate::state::AppState;

/// A validated `CreateUploadRequest`.
pub struct UploadInput {
    pub parent_id: Uuid,
    pub name: NodeName,
    pub hash: ContentHash,
    /// Fits in `i64` (checked by `parse`).
    pub size: u64,
}

impl UploadInput {
    pub fn parse(request: CreateUploadRequest) -> Result<Self, ApiError> {
        if i64::try_from(request.size).is_err() {
            return Err(ApiError::Validation("the file is too large".into()));
        }
        Ok(Self {
            parent_id: request.parent_id,
            name: NodeName::parse(&request.name)?,
            hash: ContentHash::parse(&request.sha256)?,
            size: request.size,
        })
    }

    fn db_size(&self) -> i64 {
        i64::try_from(self.size).expect("checked in parse")
    }
}

/// Dedup within the caller's own library: if one of their nodes already
/// references this content, link a new node to it. `None` otherwise — even when
/// another user has the content (never revealed).
pub async fn link_existing(state: &AppState, owner_id: Uuid, input: &UploadInput) -> Result<Option<NodeRecord>, ApiError> {
    let mut tx = state.pool.begin().await.map_err(DbError::from)?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await?;
    let Some(blob) = blobs::find_owned(&mut tx, owner_id, &input.hash).await? else {
        return Ok(None);
    };
    if blob.size != input.db_size() {
        return Err(ApiError::Validation("size does not match the content with this sha256".into()));
    }
    nodes::find_live_folder(&mut *tx, owner_id, input.parent_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    quota::ensure_room(&mut tx, owner_id, quota_bytes, blob.size, Pending::Ignore).await?;
    blobs::lock(&mut tx, &input.hash).await?;
    let node = nodes::insert_file(
        &mut tx,
        &NewFile {
            owner_id,
            parent_id: input.parent_id,
            name: &input.name,
            blob: &input.hash,
            size: blob.size,
            mime: &blob.mime,
        },
    )
    .await?;
    tx.commit().await.map_err(DbError::from)?;
    tracing::info!(user_id = %owner_id, node_id = %node.id, "upload deduplicated");
    Ok(Some(node))
}

/// Resumes the caller's open session for this exact file, or opens a new one
/// (after checking disk space and quota) with its sparse upload file.
pub async fn open_session(state: &AppState, owner_id: Uuid, input: &UploadInput) -> Result<UploadRecord, ApiError> {
    let pool = &state.pool;
    if let Some(existing) =
        uploads::find_resumable(pool, owner_id, input.parent_id, &input.name, &input.hash, input.db_size()).await?
    {
        let (store, id, size) = (state.storage.clone(), existing.id, input.size);
        run_storage(move || store.create_upload(id, size)).await?;
        return Ok(existing);
    }
    ensure_disk_space(state, input.size).await?;
    let plan = UploadPlan::new(input.size, state.config.upload_chunk_size)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("upload_chunk_size must be positive")))?;
    let chunk_count = i32::try_from(plan.chunk_count())
        .map_err(|_| ApiError::Validation("the file is too large".into()))?;
    let record = uploads::create(
        pool,
        &NewUpload {
            owner_id,
            parent_id: input.parent_id,
            name: &input.name,
            sha256: &input.hash,
            size: input.db_size(),
            chunk_size: i64::try_from(plan.chunk_size()).expect("chunk size fits i64"),
            chunk_count,
            expires_at: OffsetDateTime::now_utc() + state.config.upload_ttl,
        },
    )
    .await?;
    let (store, id) = (state.storage.clone(), record.id);
    if let Err(err) = run_storage(move || store.create_upload(id, plan.size())).await {
        uploads::delete(pool, record.id).await?;
        return Err(err);
    }
    tracing::info!(user_id = %owner_id, upload_id = %record.id, size = input.size, "upload started");
    Ok(record)
}

async fn ensure_disk_space(state: &AppState, size: u64) -> Result<(), ApiError> {
    let store = state.storage.clone();
    let available = run_storage(move || store.available_space()).await?;
    if available.saturating_sub(state.config.min_free_bytes) < size {
        tracing::warn!(available, size, "refusing upload: not enough free disk space");
        return Err(ApiError::StorageFull);
    }
    Ok(())
}
```

`src/routes/uploads/mod.rs`:
```rust
mod create;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use squirrel_db::uploads;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::{CreateUploadRequest, DeduplicatedUpload, UploadSessionDto};
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;

#[utoipa::path(post, path = "/api/uploads", tag = "uploads",
    request_body = CreateUploadRequest,
    responses(
        (status = 200, body = DeduplicatedUpload, description = "Content already in your library: linked, nothing to upload"),
        (status = 201, body = UploadSessionDto, description = "Session opened or resumed: send the missing chunks"),
        (status = 404, body = ErrorBody, description = "Destination folder not found"),
        (status = 422, body = ErrorBody),
        (status = 507, body = ErrorBody, description = "quota_exceeded or storage_full"),
    ))]
pub async fn create_upload(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(request): ApiJson<CreateUploadRequest>,
) -> Result<Response, ApiError> {
    let input = create::UploadInput::parse(request)?;
    let owner_id = current.user.id;
    if let Some(node) = create::link_existing(&state, owner_id, &input).await? {
        let body = DeduplicatedUpload { deduplicated: true, node: (&node).into() };
        return Ok((StatusCode::OK, Json(body)).into_response());
    }
    let session = create::open_session(&state, owner_id, &input).await?;
    let received = uploads::received_chunks(&state.pool, session.id).await?;
    Ok((StatusCode::CREATED, Json(UploadSessionDto::new(&session, &received))).into_response())
}

#[utoipa::path(get, path = "/api/uploads/{id}", tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses((status = 200, body = UploadSessionDto), (status = 404, body = ErrorBody)))]
pub async fn upload_status(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<UploadSessionDto>, ApiError> {
    let session = uploads::find(&state.pool, current.user.id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let received = uploads::received_chunks(&state.pool, id).await?;
    Ok(Json(UploadSessionDto::new(&session, &received)))
}

#[utoipa::path(delete, path = "/api/uploads/{id}", tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn abort_upload(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    uploads::abort(&state.pool, current.user.id, id).await?;
    let store = state.storage.clone();
    run_storage(move || store.discard_upload(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

`src/lib.rs` — in `json_routes()`:
```rust
        .routes(routes!(routes::uploads::create_upload))
        .routes(routes!(routes::uploads::upload_status, routes::uploads::abort_upload))
```

**Step 5: Run tests** — `cargo test -p squirrel-api --test uploads` → PASS (7). Regenerate `openapi.json` + `schema.d.ts`; `cargo test --workspace` → PASS.

**Step 6: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): start, resume, inspect and abort uploads with dedup" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: `PUT /api/uploads/{id}/chunks/{index}`

**Files:**
- Create: `crates/squirrel-api/src/routes/uploads/chunks.rs`, `crates/squirrel-api/tests/upload_chunks.rs`
- Modify: `src/routes/uploads/mod.rs` (`pub mod chunks;` — register the handler by its full path, `routes!(routes::uploads::chunks::put_chunk)`, so utoipa-axum also finds the generated `__path_put_chunk`), `src/lib.rs`

**Step 1: Write the failing tests** — `crates/squirrel-api/tests/upload_chunks.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn session(instance: &common::Instance, browser: &axum_test::TestServer, me: &Value, size: usize) -> String {
    let parent = files::root_id(instance, me, "Files").await;
    let content = vec![1u8; size];
    let started: Value = files::start_upload(browser, me, parent, "big.bin", &content).await.json();
    started["upload_id"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_arrive_in_any_order_and_repeat_safely(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 2500).await;

    for (index, len) in [(2, 452), (0, 1024), (0, 1024), (1, 1024)] {
        files::put_chunk(&browser, &me, &id, index, &vec![1u8; len])
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let status: Value = browser.get(&format!("/api/uploads/{id}")).await.json();
    assert_eq!(status["received_chunks"], json!([0, 1, 2]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunk_sizes_and_indexes_are_validated(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 2500).await;

    let short = files::put_chunk(&browser, &me, &id, 0, &[1; 1000]).await;
    short.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let past_end = files::put_chunk(&browser, &me, &id, 3, &[1; 10]).await;
    past_end.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let huge = files::put_chunk(&browser, &me, &id, 0, &vec![1; 200 * 1024]).await;
    huge.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(files::code(&huge), "payload_too_large");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_need_csrf_and_the_owners_session(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let id = session(&instance, &alice, &alice_me, 10).await;

    alice
        .put(&format!("/api/uploads/{id}/chunks/0"))
        .bytes(vec![1u8; 10].into())
        .await
        .assert_status(StatusCode::FORBIDDEN);
    files::put_chunk(&bob, &bob_me, &id, 0, &[1; 10])
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn chunks_for_an_aborted_upload_are_not_found(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let id = session(&instance, &browser, &me, 10).await;
    browser
        .delete(&format!("/api/uploads/{id}"))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    files::put_chunk(&browser, &me, &id, 0, &[1; 10])
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test upload_chunks` → FAIL (405/404).

**Step 3: Implement** — `src/routes/uploads/chunks.rs`:
```rust
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::UploadPlan;
use squirrel_db::{DbError, uploads};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiBytes, ApiPath};
use crate::state::AppState;

/// Stores one chunk at its offset. Chunks may arrive in any order and may be
/// re-sent; each must have exactly the planned length.
#[utoipa::path(put, path = "/api/uploads/{id}/chunks/{index}", tag = "uploads",
    params(
        ("id" = Uuid, Path, description = "Upload id"),
        ("index" = u32, Path, description = "Zero-based chunk index"),
    ),
    request_body = Vec<u8>,
    responses(
        (status = 204, description = "Chunk stored"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The upload is being completed"),
        (status = 413, body = ErrorBody),
        (status = 422, body = ErrorBody, description = "Wrong index or length"),
        (status = 507, body = ErrorBody),
    ))]
pub async fn put_chunk(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath((id, index)): ApiPath<(Uuid, u32)>,
    ApiBytes(body): ApiBytes,
) -> Result<StatusCode, ApiError> {
    let owner_id = current.user.id;
    let session = uploads::find(&state.pool, owner_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !session.is_open() {
        return Err(DbError::UploadBusy.into());
    }
    let plan = UploadPlan::new(
        u64::try_from(session.size).unwrap_or_default(),
        u64::try_from(session.chunk_size).unwrap_or_default(),
    )
    .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("upload {id} has an invalid chunk size")))?;
    let chunk = u64::from(index);
    let (Some(offset), Some(expected)) = (plan.offset(chunk), plan.chunk_len(chunk)) else {
        return Err(ApiError::Validation(format!(
            "chunk index must be below {}",
            plan.chunk_count()
        )));
    };
    if body.len() as u64 != expected {
        return Err(ApiError::Validation(format!(
            "chunk {index} must be exactly {expected} bytes"
        )));
    }
    let store = state.storage.clone();
    run_storage(move || store.write_chunk(id, offset, &body)).await?;
    let index = i32::try_from(index).map_err(|_| ApiError::Validation("chunk index too large".into()))?;
    let expires_at = OffsetDateTime::now_utc() + state.config.upload_ttl;
    if !uploads::record_chunk(&state.pool, owner_id, id, index, expires_at).await? {
        // Completed, aborted or expired while the bytes were being written.
        return Err(DbError::UploadBusy.into());
    }
    Ok(StatusCode::NO_CONTENT)
}
```
`src/lib.rs` — register it in the chunk group of `transfer_routes`:
```rust
    let chunks = OpenApiRouter::new()
        .routes(routes!(routes::uploads::chunks::put_chunk))
        .layer(DefaultBodyLimit::max(chunk_limit))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, CHUNK_TIMEOUT));
```
Note: the body is fully buffered (`Bytes`, ≤ chunk size + 64 KiB), then written with `pwrite` + `fdatasync` on the blocking pool *before* the chunk is recorded, so a recorded chunk is always durable.

**Step 4: Run tests** — `cargo test -p squirrel-api --test upload_chunks` → PASS (4). Regenerate the contract; workspace tests green.

**Step 5: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): accept upload chunks in any order" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: `POST /api/uploads/{id}/complete`

**Files:**
- Create: `crates/squirrel-api/src/routes/uploads/complete.rs`, `crates/squirrel-api/tests/upload_complete.rs`, `crates/squirrel-api/tests/dedup.rs`
- Modify: `src/routes/uploads/mod.rs` (`pub mod complete;`), `src/lib.rs`

**Step 1: Write the failing tests**

`crates/squirrel-api/tests/upload_complete.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::{ContentHash, Username};
use time::{Duration, OffsetDateTime};

async fn alice(instance: &common::Instance) -> (axum_test::TestServer, Value, uuid::Uuid) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(instance, &me, "Files").await;
    (browser, me, parent)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_stores_the_blob_and_creates_the_node(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let content: Vec<u8> = (0..2500u32).map(|i| (i % 251) as u8).collect();

    let node = files::upload(&browser, &me, parent, "data.bin", &content).await;
    assert_eq!((node["name"].as_str(), node["size"].as_u64()), (Some("data.bin"), Some(2500)));
    assert_eq!(node["kind"], "file");

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    assert_eq!(std::fs::read(instance.state.storage.blob_path(&hash)).unwrap(), content);
    let leftovers = std::fs::read_dir(instance.data_dir().join("tmp/uploads")).unwrap().count();
    assert_eq!(leftovers, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn empty_files_complete_without_chunks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let node = files::upload(&browser, &me, parent, "empty.txt", b"").await;
    assert_eq!(node["size"], 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn mime_is_sniffed_from_content_not_the_name(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let pdf = files::upload(&browser, &me, parent, "notes.txt", b"%PDF-1.7\n%...").await;
    assert_eq!(pdf["mime"], "application/pdf");
    let text = files::upload(&browser, &me, parent, "readme.txt", b"hello").await;
    assert_eq!(text["mime"], "text/plain");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn missing_chunks_are_reported_and_the_session_survives(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let content = vec![3u8; 1500];
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", &content).await.json();
    let id = session["upload_id"].as_str().unwrap();
    files::put_chunk(&browser, &me, id, 0, &content[..1024]).await.assert_status(StatusCode::NO_CONTENT);

    let early = files::complete(&browser, &me, id).await;
    early.assert_status(StatusCode::CONFLICT);
    assert_eq!(files::code(&early), "upload_incomplete");
    files::put_chunk(&browser, &me, id, 1, &content[1024..]).await.assert_status(StatusCode::NO_CONTENT);
    files::complete(&browser, &me, id).await.assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_hash_mismatch_discards_the_upload(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await.json();
    let id = session["upload_id"].as_str().unwrap();
    files::put_chunk(&browser, &me, id, 0, b"abd").await.assert_status(StatusCode::NO_CONTENT);

    let response = files::complete(&browser, &me, id).await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(files::code(&response), "hash_mismatch");
    browser.get(&format!("/api/uploads/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    assert!(!instance.data_dir().join("tmp/uploads").join(id).exists());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn name_clashes_are_renamed(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, me, parent) = alice(&instance).await;
    files::upload(&browser, &me, parent, "report.pdf", b"one").await;
    let second = files::upload(&browser, &me, parent, "report.pdf", b"two").await;
    assert_eq!(second["name"], "report (1).pdf");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_quota_is_enforced_again_at_completion(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let alice_name = Username::parse("alice").unwrap();
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", &[1; 100]).await.json();
    let id = session["upload_id"].as_str().unwrap();
    files::put_chunk(&browser, &me, id, 0, &[1; 100]).await.assert_status(StatusCode::NO_CONTENT);

    squirrel_db::users::set_quota(&pool, &alice_name, Some(50)).await.unwrap();
    let refused = files::complete(&browser, &me, id).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&refused), "quota_exceeded");

    squirrel_db::users::set_quota(&pool, &alice_name, None).await.unwrap();
    files::complete(&browser, &me, id).await.assert_status(StatusCode::CREATED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_completion_is_refused(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let session: Value = files::start_upload(&browser, &me, parent, "a.bin", b"abc").await.json();
    let id = session["upload_id"].as_str().unwrap();
    files::put_chunk(&browser, &me, id, 0, b"abc").await.assert_status(StatusCode::NO_CONTENT);

    // Another request holds the session.
    let later = OffsetDateTime::now_utc() + Duration::hours(1);
    squirrel_db::uploads::claim(&pool, files::user_id(&me), id.parse().unwrap(), later).await.unwrap();
    files::complete(&browser, &me, id).await.assert_status(StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn completing_into_a_trashed_folder_fails(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let (browser, me, parent) = alice(&instance).await;
    let folder = squirrel_db::nodes::create_folder(
        &pool,
        files::user_id(&me),
        parent,
        &squirrel_core::NodeName::parse("dir").unwrap(),
    )
    .await
    .unwrap();
    let session: Value = files::start_upload(&browser, &me, folder.id, "a.bin", b"abc").await.json();
    let id = session["upload_id"].as_str().unwrap();
    files::put_chunk(&browser, &me, id, 0, b"abc").await.assert_status(StatusCode::NO_CONTENT);
    squirrel_db::trash::trash(&pool, files::user_id(&me), folder.id).await.unwrap();
    files::complete(&browser, &me, id).await.assert_status(StatusCode::CONFLICT);
}
```

`crates/squirrel-api/tests/dedup.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::ContentHash;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn known_content_is_linked_without_uploading(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let files_root = files::root_id(&instance, &me, "Files").await;
    let docs = files::root_id(&instance, &me, "Documents").await;
    let content = vec![9u8; 3000];
    files::upload(&browser, &me, files_root, "a.bin", &content).await;

    let again = files::start_upload(&browser, &me, docs, "copy.bin", &content).await;
    again.assert_status(StatusCode::OK);
    let body: Value = again.json();
    assert_eq!(body["deduplicated"], true);
    assert_eq!((body["node"]["name"].as_str(), body["node"]["size"].as_u64()), (Some("copy.bin"), Some(3000)));

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    let blob = squirrel_db::blobs::find(&pool, &hash).await.unwrap().unwrap();
    assert_eq!(blob.ref_count, 2);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_users_content_is_never_revealed_but_still_shared_on_disk(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let content = b"the same holiday photo".to_vec();
    let alice_files = files::root_id(&instance, &alice_me, "Files").await;
    let bob_files = files::root_id(&instance, &bob_me, "Files").await;
    files::upload(&alice, &alice_me, alice_files, "photo.jpg", &content).await;

    // Bob must upload the bytes: the probe answers exactly as for unknown content.
    let probe = files::start_upload(&bob, &bob_me, bob_files, "photo.jpg", &content).await;
    probe.assert_status(StatusCode::CREATED);
    files::upload(&bob, &bob_me, bob_files, "photo.jpg", &content).await;

    let hash = ContentHash::parse(&files::sha256_hex(&content)).unwrap();
    assert_eq!(squirrel_db::blobs::find(&pool, &hash).await.unwrap().unwrap().ref_count, 2);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn dedup_counts_against_the_quota(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "a.bin", &[1; 60]).await;
    squirrel_db::users::set_quota(&pool, &squirrel_core::Username::parse("alice").unwrap(), Some(100))
        .await
        .unwrap();
    let response = files::start_upload(&browser, &me, parent, "b.bin", &[1; 60]).await;
    response.assert_status(StatusCode::INSUFFICIENT_STORAGE);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test upload_complete --test dedup` → FAIL (complete is 404; dedup tests fail on `files::upload`).

**Step 3: Implement** — `src/routes/uploads/complete.rs`:
```rust
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::{ContentHash, NodeName};
use squirrel_db::nodes::{self, NewFile, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::uploads::{self, UploadRecord};
use squirrel_db::{DbError, blobs};
use squirrel_storage::mime;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::NodeDto;
use crate::input::ApiPath;
use crate::state::AppState;

/// How a failed completion leaves the session.
enum Failure {
    /// The client can fix the cause and complete again (missing chunks, quota).
    Retry(ApiError),
    /// The session and its file are discarded (bad hash, destination gone,
    /// or a failure after the file moved into the blob store).
    Discard(ApiError),
}

fn retry(err: impl Into<ApiError>) -> Failure {
    Failure::Retry(err.into())
}

fn discard(err: impl Into<ApiError>) -> Failure {
    Failure::Discard(err.into())
}

#[utoipa::path(post, path = "/api/uploads/{id}/complete", tag = "uploads",
    params(("id" = Uuid, Path, description = "Upload id")),
    responses(
        (status = 201, body = NodeDto, description = "File created; its name may carry a \" (n)\" suffix if the name was taken"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "upload_incomplete, already completing, or destination folder gone"),
        (status = 422, body = ErrorBody, description = "hash_mismatch: the upload was discarded"),
        (status = 507, body = ErrorBody),
    ))]
pub async fn complete_upload(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<(StatusCode, Json<NodeDto>), ApiError> {
    let expires_at = OffsetDateTime::now_utc() + state.config.upload_ttl;
    let session = uploads::claim(&state.pool, current.user.id, id, expires_at).await?;
    match finish(&state, &session).await {
        Ok(node) => {
            tracing::info!(user_id = %current.user.id, upload_id = %id, node_id = %node.id,
                size = node.size, "upload completed");
            Ok((StatusCode::CREATED, Json((&node).into())))
        }
        Err(Failure::Retry(err)) => {
            uploads::release(&state.pool, current.user.id, id, expires_at).await?;
            Err(err)
        }
        Err(Failure::Discard(err)) => {
            discard_session(&state, id).await;
            Err(err)
        }
    }
}

async fn finish(state: &AppState, session: &UploadRecord) -> Result<NodeRecord, Failure> {
    let received = uploads::received_chunks(&state.pool, session.id).await.map_err(retry)?;
    let expected = usize::try_from(session.chunk_count).unwrap_or_default();
    if received.len() < expected {
        let missing = expected - received.len();
        return Err(retry(ApiError::UploadIncomplete(format!(
            "{missing} chunk(s) have not been received yet"
        ))));
    }
    let claimed = ContentHash::parse(&session.sha256).map_err(|err| discard(ApiError::Internal(err.into())))?;
    let name = NodeName::parse(&session.name).map_err(|err| discard(ApiError::Internal(err.into())))?;
    let (store, id) = (state.storage.clone(), session.id);
    let hashed = run_storage(move || store.hash_upload(id)).await.map_err(Failure::Retry)?;
    if hashed.hash != claimed || i64::try_from(hashed.size).ok() != Some(session.size) {
        tracing::warn!(upload_id = %session.id, "uploaded content does not match its sha256");
        return Err(discard(ApiError::HashMismatch));
    }
    let mime = mime::detect(&hashed.head, name.as_str());
    store_and_link(state, session, &claimed, &name, &mime).await
}

/// One transaction: owner lock and quota, blob lock, move the file into the
/// blob store (idempotent), record blob and node, drop the session.
async fn store_and_link(
    state: &AppState,
    session: &UploadRecord,
    hash: &ContentHash,
    name: &NodeName,
    mime: &str,
) -> Result<NodeRecord, Failure> {
    let owner_id = session.owner_id;
    let mut tx = state.pool.begin().await.map_err(|err| retry(DbError::from(err)))?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await.map_err(retry)?;
    nodes::find_live_folder(&mut *tx, owner_id, session.parent_id)
        .await
        .map_err(retry)?
        .ok_or_else(|| discard(ApiError::Conflict("the destination folder no longer exists".into())))?;
    quota::ensure_room(&mut tx, owner_id, quota_bytes, session.size, Pending::Ignore)
        .await
        .map_err(retry)?;
    blobs::lock(&mut tx, hash).await.map_err(retry)?;

    let (store, id, blob) = (state.storage.clone(), session.id, hash.clone());
    run_storage(move || store.adopt_upload(id, &blob)).await.map_err(Failure::Retry)?;
    // The upload file has moved: from here on a failure cannot be retried.
    blobs::insert_if_absent(&mut tx, hash, session.size, mime).await.map_err(discard)?;
    let node = nodes::insert_file(
        &mut tx,
        &NewFile { owner_id, parent_id: session.parent_id, name, blob: hash, size: session.size, mime },
    )
    .await
    .map_err(discard)?;
    uploads::delete(&mut *tx, session.id).await.map_err(discard)?;
    tx.commit().await.map_err(|err| discard(DbError::from(err)))?;
    Ok(node)
}

/// Best effort: the expiry sweep (Task 15) catches anything left behind.
async fn discard_session(state: &AppState, id: Uuid) {
    if let Err(err) = uploads::delete(&state.pool, id).await {
        tracing::warn!(upload_id = %id, error = %err, "could not delete upload session");
    }
    let store = state.storage.clone();
    if let Err(err) = run_storage(move || store.discard_upload(id)).await {
        tracing::warn!(upload_id = %id, error = %err, "could not delete upload file");
    }
}
```
Notes:
- `hash_upload` returns `StorageError::NotFound` when the upload file is gone (e.g. removed by the expiry sweep or by hand). That cannot be fixed by retrying, so `finish` must map it to `Failure::Discard` (session dropped, 404/409 to the client) rather than `Failure::Retry`, e.g. `.map_err(|err| match err { ApiError::NotFound => discard(err), other => Failure::Retry(other) })` (`StorageError::NotFound` maps to `ApiError::NotFound`).
- `adopt_upload` (like `adopt` and `remove_blob`) must be called with the per-blob lock held; `store_and_link` takes it via `blobs::lock` first.
- If `adopt_upload` itself fails (e.g. `ENOSPC` while creating the shard directory) the file is still in the upload area, so the session is released and the client may retry (507 `storage_full`).
- If the commit fails after the rename, the blob file exists without a row. That is harmless: the next upload of the same content renames over it or finds it (`adopt` is idempotent) and inserts the row. Such orphans are reclaimed by the Phase 7 scrub.
- Register it in the long-running group of `transfer_routes`:
```rust
    let long_running = OpenApiRouter::new()
        .routes(routes!(routes::uploads::complete::complete_upload))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
```

**Step 4: Run tests** — `cargo test -p squirrel-api --test upload_complete --test dedup` → PASS (12). Regenerate the contract; workspace green.

**Step 5: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): verify and store completed uploads" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Upload cleanup and periodic tasks

**Files:**
- Create: `crates/squirrel-api/src/maintenance.rs`, `crates/squirrel-api/tests/maintenance.rs`
- Create: `crates/squirrel/src/tasks.rs`
- Modify: `crates/squirrel-api/src/lib.rs` (`pub mod maintenance;`), `crates/squirrel/src/main.rs`

**Step 1: Write the failing tests** — `crates/squirrel-api/tests/maintenance.rs`:
```rust
mod common;

use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_api::maintenance;
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_sessions_and_orphan_files_are_removed(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let expired: Value = files::start_upload(&browser, &me, parent, "old.bin", b"old").await.json();
    let live: Value = files::start_upload(&browser, &me, parent, "new.bin", b"new").await.json();
    sqlx::query("UPDATE upload_sessions SET expires_at = now() - interval '1 minute' WHERE id = $1")
        .bind(expired["upload_id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let orphan = Uuid::now_v7();
    instance.state.storage.create_upload(orphan, 1).unwrap();

    let cleanup = maintenance::purge_expired_uploads(&instance.state).await.unwrap();
    assert_eq!((cleanup.expired_sessions, cleanup.files_removed), (1, 2));

    let uploads_dir = instance.data_dir().join("tmp/uploads");
    let left: Vec<String> = std::fs::read_dir(uploads_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, [live["upload_id"].as_str().unwrap()]);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test maintenance` → FAIL (no `maintenance`).

**Step 3: Implement** — `crates/squirrel-api/src/maintenance.rs`:
```rust
//! Periodic housekeeping, run by the `squirrel` binary (see `tasks.rs`).

use std::collections::HashSet;

use squirrel_db::uploads;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UploadCleanup {
    pub expired_sessions: usize,
    /// Upload files without a session (expired, aborted mid-write, crashed).
    pub files_removed: usize,
}

impl UploadCleanup {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Deletes expired upload sessions, then every upload file that no session owns.
/// Sessions are created before their file, so a file listed here whose id has
/// no row really is an orphan.
pub async fn purge_expired_uploads(state: &AppState) -> Result<UploadCleanup, ApiError> {
    let expired = uploads::delete_expired(&state.pool).await?;
    let store = state.storage.clone();
    let on_disk = run_storage(move || store.list_uploads()).await?;
    let live: HashSet<Uuid> = uploads::existing(&state.pool, &on_disk).await?.into_iter().collect();
    let orphans: Vec<Uuid> = on_disk.into_iter().filter(|id| !live.contains(id)).collect();
    let files_removed = orphans.len();
    let store = state.storage.clone();
    run_storage(move || orphans.iter().try_for_each(|id| store.discard_upload(*id))).await?;
    Ok(UploadCleanup { expired_sessions: expired.len(), files_removed })
}
```

`crates/squirrel/src/tasks.rs` — move the session cleanup here and generalize it:
```rust
use std::future::Future;
use std::time::Duration;

use squirrel_api::{AppState, maintenance};
use tokio::time::MissedTickBehavior;

const HOURLY: Duration = Duration::from_secs(60 * 60);

/// Runs `job` now and then every `every`. Jobs log their own failures and are
/// retried on the next tick.
fn spawn_periodic<F, Fut>(every: Duration, mut job: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(every);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            job().await;
        }
    });
}

pub fn spawn_all(state: &AppState) {
    let pool = state.pool.clone();
    spawn_periodic(HOURLY, move || {
        let pool = pool.clone();
        async move {
            match squirrel_db::sessions::delete_expired(&pool).await {
                Ok(0) => {}
                Ok(deleted) => tracing::info!(deleted, "removed expired sessions"),
                Err(err) => tracing::error!(error = %err, "failed to remove expired sessions"),
            }
        }
    });
    let uploads_state = state.clone();
    spawn_periodic(HOURLY, move || {
        let state = uploads_state.clone();
        async move {
            match maintenance::purge_expired_uploads(&state).await {
                Ok(cleanup) if cleanup.is_empty() => {}
                Ok(cleanup) => tracing::info!(?cleanup, "removed expired uploads"),
                Err(err) => tracing::error!(error = %err, "failed to remove expired uploads"),
            }
        }
    });
}
```
`main.rs`: `mod tasks;`, delete `spawn_session_cleanup` and `SESSION_CLEANUP_INTERVAL`, and call `tasks::spawn_all(&state);` where `spawn_session_cleanup(pool)` was.

**Step 4: Run tests** — `cargo test -p squirrel-api --test maintenance` → PASS. Full Rust gate.

**Step 5: Manual smoke test** (optional but recommended):
```bash
docker compose -f deploy/docker-compose.dev.yml up -d --wait
SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
SQUIRREL_DATA_DIR="$PWD/target/dev-data" SQUIRREL_COOKIE_SECURE=false \
cargo run -p squirrel -- serve
```
Expected: starts, creates `target/dev-data/{blobs,tmp/uploads}`.

**Step 6: Commit**
```bash
git add crates
git commit -m "feat: clean up expired and orphaned uploads periodically" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Milestone D — Tree, trash, downloads & storage API

### Task 16: Folder listing, node details, create folder, rename/move

**Files:**
- Create: `crates/squirrel-api/src/cursor.rs`, `crates/squirrel-api/src/routes/nodes.rs`, `crates/squirrel-api/tests/nodes.rs`
- Modify: `src/files_dto.rs`, `src/lib.rs` (`mod cursor;`, routes), `src/routes/mod.rs` (`pub mod nodes;`)

**Step 1: Write the failing tests**

Unit tests at the bottom of `src/cursor.rs` (create the file with just these first):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn folder_cursors_round_trip_any_name() {
        let cursor = ListCursor { kind: NodeKind::File, name: "Été 2026 (1).jpg".into() };
        assert_eq!(decode_folder(&encode_folder(&cursor)).unwrap(), cursor);
    }

    #[test]
    fn trash_cursors_keep_microseconds() {
        let cursor = TrashCursor { deleted_at: datetime!(2026-11-13 10:00:00.123456 UTC), id: Uuid::now_v7() };
        assert_eq!(decode_trash(&encode_trash(&cursor)).unwrap(), cursor);
    }

    #[test]
    fn garbage_is_a_bad_request() {
        for raw in ["", "!!!", &URL_SAFE_NO_PAD.encode("no-separator"), &URL_SAFE_NO_PAD.encode("symlink\nx")] {
            assert!(matches!(decode_folder(raw), Err(ApiError::BadRequest(_))), "{raw}");
        }
    }

    #[test]
    fn page_sizes_are_clamped() {
        assert_eq!(page_size(None), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some(0)), 1);
        assert_eq!(page_size(Some(10_000)), MAX_PAGE_SIZE);
    }
}
```
(`time::macros` needs the `macros` feature of `time`: add it to the workspace `time` features list.)

Integration tests — `crates/squirrel-api/tests/nodes.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn mkdir(browser: &axum_test::TestServer, me: &Value, parent: Uuid, name: &str) -> axum_test::TestResponse {
    browser
        .post("/api/folders")
        .add_header(common::CSRF, common::csrf(me))
        .json(&json!({ "parent_id": parent, "name": name }))
        .await
}

async fn patch(browser: &axum_test::TestServer, me: &Value, id: &str, body: Value) -> axum_test::TestResponse {
    browser
        .patch(&format!("/api/nodes/{id}"))
        .add_header(common::CSRF, common::csrf(me))
        .json(&body)
        .await
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn lists_the_top_level_folders(pool: PgPool) {
    let browser = common::server(pool);
    common::setup_admin(&browser, "alice").await;
    let listing: Value = browser.get("/api/nodes").await.json();
    let names: Vec<_> = listing["items"].as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Documents", "Files", "Photos"]);
    assert!(listing["items"].as_array().unwrap().iter().all(|n| n["is_root"] == true && n["kind"] == "folder"));
    assert_eq!(listing["next_cursor"], Value::Null);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_lists_and_paginates_folders(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    for name in ["c", "a", "b"] {
        mkdir(&browser, &me, parent, name).await.assert_status(StatusCode::CREATED);
    }
    let dup = mkdir(&browser, &me, parent, "a").await;
    dup.assert_status(StatusCode::CONFLICT);
    assert_eq!(files::code(&dup), "name_conflict");

    let first: Value = browser.get(&format!("/api/nodes?parent_id={parent}&limit=2")).await.json();
    let cursor = first["next_cursor"].as_str().unwrap();
    let second: Value = browser
        .get(&format!("/api/nodes?parent_id={parent}&limit=2&cursor={cursor}"))
        .await
        .json();
    let names: Vec<_> = first["items"].as_array().unwrap().iter()
        .chain(second["items"].as_array().unwrap())
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(second["next_cursor"], Value::Null);
    browser
        .get(&format!("/api/nodes?parent_id={parent}&cursor=garbage!"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn node_details_include_the_breadcrumb(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let docs = files::root_id(&instance, &me, "Documents").await;
    let taxes: Value = mkdir(&browser, &me, docs, "Taxes").await.json();
    let details: Value = browser.get(&format!("/api/nodes/{}", taxes["id"].as_str().unwrap())).await.json();
    assert_eq!(details["node"]["name"], "Taxes");
    assert_eq!(details["path"], json!([{ "id": docs, "name": "Documents" }]));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn renames_and_moves_with_conflict_and_cycle_checks(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let a: Value = mkdir(&browser, &me, parent, "a").await.json();
    let b: Value = mkdir(&browser, &me, parent, "b").await.json();
    let (a_id, b_id) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());

    let renamed: Value = patch(&browser, &me, a_id, json!({ "name": "alpha" })).await.json();
    assert_eq!(renamed["name"], "alpha");
    patch(&browser, &me, a_id, json!({ "name": "b" })).await.assert_status(StatusCode::CONFLICT);
    let moved: Value = patch(&browser, &me, b_id, json!({ "parent_id": a_id })).await.json();
    assert_eq!(moved["parent_id"], a_id);
    patch(&browser, &me, a_id, json!({ "parent_id": b_id })).await.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(&browser, &me, a_id, json!({})).await.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(&browser, &me, a_id, json!({ "name": "x/y" })).await.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    patch(&browser, &me, &parent.to_string(), json!({ "name": "Stuff" }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_users_nodes_are_invisible(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let alice_files = files::root_id(&instance, &alice_me, "Files").await;
    let folder: Value = mkdir(&alice, &alice_me, alice_files, "private").await.json();
    let id = folder["id"].as_str().unwrap();

    bob.get(&format!("/api/nodes/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    bob.get(&format!("/api/nodes?parent_id={alice_files}")).await.assert_status(StatusCode::NOT_FOUND);
    patch(&bob, &bob_me, id, json!({ "name": "mine" })).await.assert_status(StatusCode::NOT_FOUND);
    mkdir(&bob, &bob_me, alice_files, "intruder").await.assert_status(StatusCode::NOT_FOUND);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test nodes` → FAIL.

**Step 3: Implement**

`src/cursor.rs`:
```rust
//! Opaque pagination cursors: base64url of `"<a>\n<b>"`. Node names cannot
//! contain control characters, so the newline is a safe separator.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use squirrel_core::NodeKind;
use squirrel_db::nodes::ListCursor;
use squirrel_db::trash::TrashCursor;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::ApiError;

pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 500;

pub fn page_size(requested: Option<u32>) -> u32 {
    requested.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

fn invalid() -> ApiError {
    ApiError::BadRequest("invalid cursor".into())
}

fn encode(first: &str, second: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{first}\n{second}"))
}

fn decode(raw: &str) -> Result<(String, String), ApiError> {
    let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let (first, second) = text.split_once('\n').ok_or_else(invalid)?;
    Ok((first.to_owned(), second.to_owned()))
}

pub fn encode_folder(cursor: &ListCursor) -> String {
    encode(cursor.kind.as_str(), &cursor.name)
}

pub fn decode_folder(raw: &str) -> Result<ListCursor, ApiError> {
    let (kind, name) = decode(raw)?;
    let kind = kind.parse::<NodeKind>().map_err(|_| invalid())?;
    Ok(ListCursor { kind, name })
}

pub fn encode_trash(cursor: &TrashCursor) -> String {
    encode(&cursor.deleted_at.unix_timestamp_nanos().to_string(), &cursor.id.to_string())
}

pub fn decode_trash(raw: &str) -> Result<TrashCursor, ApiError> {
    let (nanos, id) = decode(raw)?;
    let nanos: i128 = nanos.parse().map_err(|_| invalid())?;
    Ok(TrashCursor {
        deleted_at: OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| invalid())?,
        id: id.parse::<Uuid>().map_err(|_| invalid())?,
    })
}
```

`src/files_dto.rs` — append:
```rust
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListNodesQuery {
    /// Folder to list; omit it for the top-level folders.
    pub parent_id: Option<Uuid>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Page size, default 100, at most 500.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeListing {
    /// Folders first, then files; names in byte order.
    pub items: Vec<NodeDto>,
    /// Pass back as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PathSegmentDto {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NodeDetails {
    pub node: NodeDto,
    /// Ancestors from the top-level folder down, excluding the node itself.
    pub path: Vec<PathSegmentDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    pub parent_id: Uuid,
    pub name: String,
}

/// Rename and/or move. At least one field is required.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNodeRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// New parent folder.
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}
```

`src/routes/nodes.rs`:
```rust
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_core::NodeName;
use squirrel_db::nodes::{self, ListCursor, NodeChange};
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::{
    CreateFolderRequest, ListNodesQuery, NodeDetails, NodeDto, NodeListing, PathSegmentDto, UpdateNodeRequest,
};
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

#[utoipa::path(get, path = "/api/nodes", tag = "files", params(ListNodesQuery),
    responses((status = 200, body = NodeListing), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn list_nodes(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(query): ApiQuery<ListNodesQuery>,
) -> Result<Json<NodeListing>, ApiError> {
    let owner_id = current.user.id;
    if let Some(parent) = query.parent_id {
        nodes::find_live_folder(&state.pool, owner_id, parent)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
    let after = query.cursor.as_deref().map(cursor::decode_folder).transpose()?;
    let page = nodes::list_children(&state.pool, owner_id, query.parent_id, after.as_ref(), cursor::page_size(query.limit))
        .await?;
    let next_cursor = page
        .items
        .last()
        .filter(|_| page.has_more)
        .map(|last| cursor::encode_folder(&ListCursor { kind: last.kind(), name: last.name.clone() }));
    Ok(Json(NodeListing { items: page.items.iter().map(NodeDto::from).collect(), next_cursor }))
}

#[utoipa::path(get, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 200, body = NodeDetails), (status = 404, body = ErrorBody)))]
pub async fn get_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<NodeDetails>, ApiError> {
    let owner_id = current.user.id;
    let node = nodes::find_live(&state.pool, owner_id, id).await?.ok_or(ApiError::NotFound)?;
    let path = nodes::path_to(&state.pool, owner_id, id).await?;
    Ok(Json(NodeDetails {
        node: (&node).into(),
        path: path.into_iter().map(|s| PathSegmentDto { id: s.id, name: s.name }).collect(),
    }))
}

#[utoipa::path(post, path = "/api/folders", tag = "files", request_body = CreateFolderRequest,
    responses((status = 201, body = NodeDto), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_folder(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiJson(request): ApiJson<CreateFolderRequest>,
) -> Result<(StatusCode, Json<NodeDto>), ApiError> {
    let name = NodeName::parse(&request.name)?;
    let node = nodes::create_folder(&state.pool, current.user.id, request.parent_id, &name).await?;
    Ok((StatusCode::CREATED, Json((&node).into())))
}

#[utoipa::path(patch, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    request_body = UpdateNodeRequest,
    responses((status = 200, body = NodeDto), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn update_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<UpdateNodeRequest>,
) -> Result<Json<NodeDto>, ApiError> {
    if request.name.is_none() && request.parent_id.is_none() {
        return Err(ApiError::Validation("give a new name, a new parent_id, or both".into()));
    }
    let change = NodeChange {
        name: request.name.as_deref().map(NodeName::parse).transpose()?,
        parent_id: request.parent_id,
    };
    let node = nodes::update(&state.pool, current.user.id, id, &change).await?;
    Ok(Json((&node).into()))
}
```
`src/lib.rs`, `json_routes()`:
```rust
        .routes(routes!(routes::nodes::list_nodes))
        .routes(routes!(routes::nodes::get_node, routes::nodes::update_node))
        .routes(routes!(routes::nodes::create_folder))
```

**Step 4: Run tests** — `cargo test -p squirrel-api --test nodes --lib` → PASS. Regenerate the contract; workspace green.

**Step 5: Commit**
```bash
git add Cargo.toml Cargo.lock crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): list folders, show breadcrumbs, create, rename and move" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: Delete to trash and trash endpoints

**Files:**
- Create: `crates/squirrel-api/src/routes/trash.rs`, `crates/squirrel-api/tests/trash.rs`
- Modify: `src/routes/nodes.rs` (`trash_node`), `src/files_dto.rs`, `src/lib.rs`, `src/routes/mod.rs`

**Step 1: Write the failing tests** — `crates/squirrel-api/tests/trash.rs`:
```rust
mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::files;
use serde_json::Value;
use sqlx::PgPool;
use squirrel_core::Username;

async fn delete(browser: &TestServer, me: &Value, path: &str) -> axum_test::TestResponse {
    browser.delete(path).add_header(common::CSRF, common::csrf(me)).await
}

async fn post(browser: &TestServer, me: &Value, path: &str) -> axum_test::TestResponse {
    browser.post(path).add_header(common::CSRF, common::csrf(me)).await
}

fn ids(listing: &Value) -> Vec<String> {
    listing["items"].as_array().unwrap().iter()
        .map(|e| e["node"]["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_moves_to_trash_and_restore_brings_back(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"hello").await;
    let id = node["id"].as_str().unwrap();

    delete(&browser, &me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NO_CONTENT);
    browser.get(&format!("/api/nodes/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    let trash: Value = browser.get("/api/trash").await.json();
    assert_eq!(ids(&trash), [id]);
    assert!(trash["items"][0]["deleted_at"].is_string());

    let restored: Value = post(&browser, &me, &format!("/api/trash/{id}/restore")).await.json();
    assert_eq!(restored["name"], "a.txt");
    browser.get(&format!("/api/nodes/{id}")).await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_renames_when_the_name_was_reused(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let old = files::upload(&browser, &me, parent, "a.txt", b"old").await;
    let id = old["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NO_CONTENT);
    files::upload(&browser, &me, parent, "a.txt", b"new").await;

    let restored: Value = post(&browser, &me, &format!("/api/trash/{id}/restore")).await.json();
    assert_eq!(restored["name"], "a (1).txt");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_forever_and_empty(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let a = files::upload(&browser, &me, parent, "a.txt", b"a").await;
    let b = files::upload(&browser, &me, parent, "b.txt", b"b").await;
    for node in [&a, &b] {
        let id = node["id"].as_str().unwrap();
        delete(&browser, &me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NO_CONTENT);
    }
    let a_id = a["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/trash/{a_id}")).await.assert_status(StatusCode::NO_CONTENT);
    delete(&browser, &me, &format!("/api/trash/{a_id}")).await.assert_status(StatusCode::NOT_FOUND);
    delete(&browser, &me, "/api/trash").await.assert_status(StatusCode::NO_CONTENT);
    let trash: Value = browser.get("/api/trash").await.json();
    assert!(ids(&trash).is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn top_level_folders_cannot_be_deleted(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    delete(&browser, &me, &format!("/api/nodes/{parent}"))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn restore_over_quota_is_refused(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let old = files::upload(&browser, &me, parent, "old.bin", &[1; 60]).await;
    let id = old["id"].as_str().unwrap();
    delete(&browser, &me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NO_CONTENT);
    files::upload(&browser, &me, parent, "new.bin", &[2; 60]).await;
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(100)).await.unwrap();
    let refused = post(&browser, &me, &format!("/api/trash/{id}/restore")).await;
    refused.assert_status(StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(files::code(&refused), "quota_exceeded");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_is_private(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, bob_me)) = common::two_users(&instance).await;
    let parent = files::root_id(&instance, &alice_me, "Files").await;
    let node = files::upload(&alice, &alice_me, parent, "a.txt", b"a").await;
    let id = node["id"].as_str().unwrap();
    delete(&bob, &bob_me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NOT_FOUND);
    delete(&alice, &alice_me, &format!("/api/nodes/{id}")).await.assert_status(StatusCode::NO_CONTENT);
    let bobs: Value = bob.get("/api/trash").await.json();
    assert!(ids(&bobs).is_empty());
    post(&bob, &bob_me, &format!("/api/trash/{id}/restore")).await.assert_status(StatusCode::NOT_FOUND);
    delete(&bob, &bob_me, &format!("/api/trash/{id}")).await.assert_status(StatusCode::NOT_FOUND);
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test trash` → FAIL.

**Step 3: Implement**

`src/files_dto.rs` — append:
```rust
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrashQuery {
    pub cursor: Option<String>,
    /// Page size, default 100, at most 500.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrashEntryDto {
    /// `parent_id` is where it was deleted from.
    pub node: NodeDto,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub deleted_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrashListing {
    /// Newest first. Folders appear once; their contents come back with them.
    pub items: Vec<TrashEntryDto>,
    pub next_cursor: Option<String>,
}
```

`src/routes/nodes.rs` — add:
```rust
#[utoipa::path(delete, path = "/api/nodes/{id}", tag = "files",
    params(("id" = Uuid, Path, description = "Node id")),
    responses((status = 204, description = "Moved to the trash with everything inside it"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn trash_node(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    squirrel_db::trash::trash(&state.pool, current.user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

`src/routes/trash.rs`:
```rust
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use squirrel_db::trash::{self, TrashCursor};
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::files_dto::{NodeDto, TrashEntryDto, TrashListing, TrashQuery};
use crate::input::{ApiPath, ApiQuery};
use crate::state::AppState;

#[utoipa::path(get, path = "/api/trash", tag = "trash", params(TrashQuery),
    responses((status = 200, body = TrashListing), (status = 400, body = ErrorBody)))]
pub async fn list_trash(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiQuery(query): ApiQuery<TrashQuery>,
) -> Result<Json<TrashListing>, ApiError> {
    let after = query.cursor.as_deref().map(cursor::decode_trash).transpose()?;
    let page = trash::list(&state.pool, current.user.id, after.as_ref(), cursor::page_size(query.limit)).await?;
    // Trash entries always have `deleted_at` (CHECK constraint).
    let items: Vec<TrashEntryDto> = page
        .items
        .iter()
        .filter_map(|node| node.deleted_at.map(|deleted_at| TrashEntryDto { node: node.into(), deleted_at }))
        .collect();
    let next_cursor = page.items.last().filter(|_| page.has_more).and_then(|last| {
        last.deleted_at
            .map(|deleted_at| cursor::encode_trash(&TrashCursor { deleted_at, id: last.id }))
    });
    Ok(Json(TrashListing { items, next_cursor }))
}

#[utoipa::path(post, path = "/api/trash/{id}/restore", tag = "trash",
    params(("id" = Uuid, Path, description = "Trash entry (node) id")),
    responses(
        (status = 200, body = NodeDto, description = "Restored into its folder, or into Files/ if that folder is gone; renamed \"name (n)\" on a clash"),
        (status = 404, body = ErrorBody), (status = 507, body = ErrorBody)))]
pub async fn restore(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<NodeDto>, ApiError> {
    let node = trash::restore(&state.pool, current.user.id, id).await?;
    Ok(Json((&node).into()))
}

#[utoipa::path(delete, path = "/api/trash/{id}", tag = "trash",
    params(("id" = Uuid, Path, description = "Trash entry (node) id")),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn delete_forever(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    trash::delete_forever(&state.pool, current.user.id, id).await?;
    tracing::info!(user_id = %current.user.id, node_id = %id, "trash entry deleted forever");
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/api/trash", tag = "trash", responses((status = 204)))]
pub async fn empty_trash(State(state): State<AppState>, current: CurrentUser) -> Result<StatusCode, ApiError> {
    let entries = trash::empty(&state.pool, current.user.id).await?;
    tracing::info!(user_id = %current.user.id, entries, "trash emptied");
    Ok(StatusCode::NO_CONTENT)
}
```
`src/lib.rs`, `json_routes()` — extend the `/api/nodes/{id}` group and add the trash routes:
```rust
        .routes(routes!(routes::nodes::get_node, routes::nodes::update_node, routes::nodes::trash_node))
        .routes(routes!(routes::trash::list_trash, routes::trash::empty_trash))
        .routes(routes!(routes::trash::restore))
        .routes(routes!(routes::trash::delete_forever))
```

**Step 4: Run tests** — `cargo test -p squirrel-api --test trash` → PASS (6). Regenerate the contract; workspace green.

**Step 5: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): trash, restore, delete forever and empty trash" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: Download helpers — content policy, ranges, `Content-Disposition`

Pure functions, unit-tested, used by Task 19. All three were compiled and tested while writing this plan.

**Files:**
- Create: `crates/squirrel-api/src/content/mod.rs`, `src/content/policy.rs`, `src/content/range.rs`, `src/content/disposition.rs`
- Modify: `crates/squirrel-api/src/lib.rs` (`mod content;`)

**Step 1: Write the failing tests** — create each file with only its test module:

`policy.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfs_are_inline_without_sandbox_unless_downloaded() {
        let inline = ContentPolicy::for_mime(Some("application/pdf"), false);
        assert_eq!((inline.content_type.as_str(), inline.inline, inline.csp), (PDF, true, PDF_CSP));
        let download = ContentPolicy::for_mime(Some("application/pdf"), true);
        assert_eq!((download.inline, download.csp), (false, SANDBOX_CSP));
    }

    #[test]
    fn images_and_text_are_inline_and_sandboxed() {
        let png = ContentPolicy::for_mime(Some("image/png"), false);
        assert_eq!((png.inline, png.csp), (true, SANDBOX_CSP));
        let text = ContentPolicy::for_mime(Some("Text/Plain"), false);
        assert_eq!(text.content_type, "text/plain; charset=utf-8");
        assert!(text.inline);
    }

    #[test]
    fn active_types_are_downgraded_to_attachments() {
        for mime in ["text/html", "image/svg+xml", "application/xml", "application/rss+xml", "text/javascript"] {
            let policy = ContentPolicy::for_mime(Some(mime), false);
            assert_eq!(policy.content_type, OCTET_STREAM, "{mime}");
            assert!(!policy.inline, "{mime}");
        }
    }

    #[test]
    fn unknown_types_are_attachments() {
        let none = ContentPolicy::for_mime(None, false);
        assert_eq!((none.content_type.as_str(), none.inline), (OCTET_STREAM, false));
        let zip = ContentPolicy::for_mime(Some("application/zip"), false);
        assert_eq!((zip.content_type.as_str(), zip.inline), ("application/zip", false));
    }

    #[test]
    fn if_none_match_uses_weak_comparison() {
        let etag = "\"abc\"";
        assert!(etag_matches(Some("\"abc\""), etag));
        assert!(etag_matches(Some("W/\"abc\", \"def\""), etag));
        assert!(etag_matches(Some("*"), etag));
        assert!(!etag_matches(Some("\"def\""), etag));
        assert!(!etag_matches(None, etag));
    }
}
```

`range.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_means_full_content() {
        assert_eq!(resolve(None, 10), RangeRequest::Full);
    }

    #[test]
    fn single_ranges_are_clamped_to_the_content() {
        assert_eq!(resolve(Some("bytes=0-3"), 10), RangeRequest::Partial(0..=3));
        assert_eq!(resolve(Some("bytes=5-"), 10), RangeRequest::Partial(5..=9));
        assert_eq!(resolve(Some("bytes=-4"), 10), RangeRequest::Partial(6..=9));
        assert_eq!(resolve(Some("bytes=8-100"), 10), RangeRequest::Partial(8..=9));
    }

    #[test]
    fn ranges_past_the_end_are_unsatisfiable() {
        assert_eq!(resolve(Some("bytes=10-"), 10), RangeRequest::Unsatisfiable);
        assert_eq!(resolve(Some("bytes=0-0"), 0), RangeRequest::Unsatisfiable);
    }

    #[test]
    fn malformed_or_multiple_ranges_fall_back_to_full_content() {
        assert_eq!(resolve(Some("bytes=abc"), 10), RangeRequest::Full);
        assert_eq!(resolve(Some("items=0-1"), 10), RangeRequest::Full);
        assert_eq!(resolve(Some("bytes=0-1,4-5"), 10), RangeRequest::Full);
    }
}
```

`disposition.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_names_are_kept() {
        assert_eq!(
            content_disposition(true, "report.pdf"),
            "inline; filename=\"report.pdf\"; filename*=UTF-8''report.pdf"
        );
    }

    #[test]
    fn unicode_and_quotes_are_encoded() {
        assert_eq!(
            content_disposition(false, "café \"menu\".pdf"),
            "attachment; filename=\"caf_ _menu_.pdf\"; filename*=UTF-8''caf%C3%A9%20%22menu%22.pdf"
        );
    }

    #[test]
    fn header_injection_characters_never_appear_raw() {
        let value = content_disposition(false, "a\r\nSet-Cookie: x.txt");
        assert!(!value.contains('\r') && !value.contains('\n'));
    }
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --lib content` → FAIL.

**Step 3: Implement**

`policy.rs`:
```rust
/// How a stored file may be served to a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPolicy {
    pub content_type: String,
    pub inline: bool,
    pub csp: &'static str,
}

/// Inline content runs in a sandbox: no scripts, an opaque origin, and only its
/// own bytes may load as an image or media.
pub const SANDBOX_CSP: &str = "sandbox; default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; frame-ancestors 'self'";
/// Built-in PDF viewers refuse to start in a sandboxed document, so inline PDFs
/// get a strict policy without `sandbox` (see Task 25 for the browser checks).
pub const PDF_CSP: &str = "default-src 'none'; object-src 'self'; frame-ancestors 'self'";
const OCTET_STREAM: &str = "application/octet-stream";
const PDF: &str = "application/pdf";
const TEXT: &str = "text/plain";

/// Types a browser displays without running code from the file.
const INLINE_TYPES: &[&str] = &[
    PDF, TEXT, "image/png", "image/jpeg", "image/gif", "image/webp", "image/avif", "image/bmp",
    "video/mp4", "video/webm", "video/ogg", "audio/mpeg", "audio/ogg", "audio/wav",
    "audio/webm", "audio/flac", "audio/mp4",
];
/// Types that render active content when opened: never served under their own type.
const ACTIVE_TYPES: &[&str] = &[
    "text/html", "application/xhtml+xml", "image/svg+xml", "text/xml", "application/xml",
    "text/javascript", "application/javascript", "application/ecmascript", "text/ecmascript",
    "text/xsl", "application/x-shockwave-flash", "multipart/x-mixed-replace",
];

impl ContentPolicy {
    /// `mime` is the type sniffed at upload time; `force_download` is `?download=true`.
    pub fn for_mime(mime: Option<&str>, force_download: bool) -> Self {
        let essence = mime
            .and_then(|m| m.split(';').next())
            .map(|m| m.trim().to_ascii_lowercase())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| OCTET_STREAM.to_owned());
        if ACTIVE_TYPES.contains(&essence.as_str()) || essence.ends_with("+xml") {
            return Self { content_type: OCTET_STREAM.to_owned(), inline: false, csp: SANDBOX_CSP };
        }
        let inline = !force_download && INLINE_TYPES.contains(&essence.as_str());
        let csp = if inline && essence == PDF { PDF_CSP } else { SANDBOX_CSP };
        let content_type = if essence == TEXT { "text/plain; charset=utf-8".to_owned() } else { essence };
        Self { content_type, inline, csp }
    }
}

/// `If-None-Match` matching (weak comparison, RFC 9110 §13.1.2): `*` or any listed tag.
pub fn etag_matches(if_none_match: Option<&str>, etag: &str) -> bool {
    if_none_match.is_some_and(|value| {
        value
            .split(',')
            .map(str::trim)
            .any(|tag| tag == "*" || tag.trim_start_matches("W/") == etag)
    })
}

```

`range.rs`:
```rust
use std::ops::RangeInclusive;

use http_range_header::RangeUnsatisfiableError;

/// What to send for a `Range` request header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeRequest {
    /// No usable `Range`: the whole content with 200.
    Full,
    /// One satisfiable byte range (inclusive) with 206.
    Partial(RangeInclusive<u64>),
    /// Well-formed but outside the content: 416 with `Content-Range: bytes */size`.
    Unsatisfiable,
}

/// Interprets a `Range` header for content of `size` bytes. Only single ranges
/// are honoured; multiple ranges, malformed values and other units fall back to
/// the full content, which RFC 9110 §14.2 allows.
pub fn resolve(header: Option<&str>, size: u64) -> RangeRequest {
    let Some(value) = header else {
        return RangeRequest::Full;
    };
    let Ok(parsed) = http_range_header::parse_range_header(value) else {
        return RangeRequest::Full;
    };
    match parsed.validate(size) {
        Ok(mut ranges) if ranges.len() == 1 => RangeRequest::Partial(ranges.remove(0)),
        Ok(_) => RangeRequest::Full,
        Err(RangeUnsatisfiableError::Unsatisfiable) => RangeRequest::Unsatisfiable,
        Err(_) => RangeRequest::Full,
    }
}

```

`disposition.rs`:
```rust
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// RFC 5987 `attr-char`: everything else in `filename*` is percent-encoded.
const ATTR_CHAR_ESCAPES: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// `Content-Disposition` value with an ASCII `filename` fallback and the exact
/// UTF-8 name in `filename*` (RFC 6266 / RFC 5987).
pub fn content_disposition(inline: bool, name: &str) -> String {
    let kind = if inline { "inline" } else { "attachment" };
    let fallback: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' && c != '%' => c,
            _ => '_',
        })
        .collect();
    let encoded = utf8_percent_encode(name, ATTR_CHAR_ESCAPES);
    format!("{kind}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

```

`content/mod.rs`:
```rust
//! Pure helpers for serving stored files safely.

mod disposition;
mod policy;
mod range;

pub use disposition::content_disposition;
pub use policy::{ContentPolicy, etag_matches};
pub use range::{RangeRequest, resolve as resolve_range};
```

Why these choices:
- Inline allowlist, not a denylist: anything unknown is an attachment.
- `text/html`, SVG, XML (`+xml` too), JavaScript are served as `application/octet-stream` + attachment even with `?download=false`, so neither a navigation nor an `<iframe>` can render them as our origin.
- `filename` keeps only printable ASCII minus `"`, `\` and `%`; `filename*` carries the exact UTF-8 name percent-encoded. CR/LF can never reach the header.

**Step 4: Run tests** — `cargo test -p squirrel-api --lib content` → PASS (12).

**Step 5: Commit**
```bash
git add crates/squirrel-api
git commit -m "feat(api): add download policy, range and disposition helpers" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: `GET /api/files/{id}/content`

**Files:**
- Create: `crates/squirrel-api/src/routes/content.rs`, `crates/squirrel-api/tests/content.rs`
- Modify: `src/routes/mod.rs`, `src/lib.rs`

**Step 1: Write the failing tests** — `crates/squirrel-api/tests/content.rs`:
```rust
mod common;

use axum::http::StatusCode;
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_RANGE, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG,
    IF_NONE_MATCH, RANGE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum_test::TestServer;
use common::files;
use serde_json::Value;
use sqlx::PgPool;

async fn alice_with(instance: &common::Instance, name: &str, content: &[u8]) -> (TestServer, Value, String) {
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, name, content).await;
    let url = format!("/api/files/{}/content", node["id"].as_str().unwrap());
    (browser, me, url)
}

fn header(response: &axum_test::TestResponse, name: axum::http::HeaderName) -> String {
    response.header(name).to_str().unwrap().to_owned()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn serves_the_bytes_with_safe_headers(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "notes.txt", b"hello squirrel").await;
    let response = browser.get(&url).await;
    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), b"hello squirrel");
    assert_eq!(header(&response, CONTENT_TYPE), "text/plain; charset=utf-8");
    assert_eq!(header(&response, CONTENT_DISPOSITION), "inline; filename=\"notes.txt\"; filename*=UTF-8''notes.txt");
    assert_eq!(header(&response, ETAG), format!("\"{}\"", files::sha256_hex(b"hello squirrel")));
    assert_eq!(header(&response, CACHE_CONTROL), "private, no-cache");
    assert_eq!(header(&response, ACCEPT_RANGES), "bytes");
    assert_eq!(header(&response, X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(header(&response, X_FRAME_OPTIONS), "SAMEORIGIN");
    assert!(header(&response, CONTENT_SECURITY_POLICY).starts_with("sandbox;"));
    assert_eq!(header(&response, "cross-origin-resource-policy".parse().unwrap()), "same-origin");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn honours_single_ranges(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "abc.txt", b"abcdefghij").await;
    let partial = browser.get(&url).add_header(RANGE, "bytes=2-4").await;
    partial.assert_status(StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.as_bytes().as_ref(), b"cde");
    assert_eq!(header(&partial, CONTENT_RANGE), "bytes 2-4/10");

    let tail = browser.get(&url).add_header(RANGE, "bytes=-3").await;
    assert_eq!(tail.as_bytes().as_ref(), b"hij");

    let outside = browser.get(&url).add_header(RANGE, "bytes=50-").await;
    outside.assert_status(StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(header(&outside, CONTENT_RANGE), "bytes */10");
    assert_eq!(files::code(&outside), "range_not_satisfiable");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revalidates_with_etags(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "a.txt", b"etag me").await;
    let etag = header(&browser.get(&url).await, ETAG);
    let cached = browser.get(&url).add_header(IF_NONE_MATCH, etag.as_str()).await;
    cached.assert_status(StatusCode::NOT_MODIFIED);
    assert!(cached.as_bytes().is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn download_forces_attachment_and_keeps_unicode_names(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "café.txt", b"bonjour").await;
    let response = browser.get(&format!("{url}?download=true")).await;
    assert_eq!(
        header(&response, CONTENT_DISPOSITION),
        "attachment; filename=\"caf_.txt\"; filename*=UTF-8''caf%C3%A9.txt"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn active_content_is_never_served_as_itself(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) =
        alice_with(&instance, "page.html", b"<!DOCTYPE html><script>alert(document.cookie)</script>").await;
    let response = browser.get(&url).await;
    assert_eq!(header(&response, CONTENT_TYPE), "application/octet-stream");
    assert!(header(&response, CONTENT_DISPOSITION).starts_with("attachment;"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pdfs_are_inline_and_frameable_by_the_app(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let (browser, _, url) = alice_with(&instance, "doc.pdf", b"%PDF-1.7\n%%EOF\n").await;
    let response = browser.get(&url).await;
    assert_eq!(header(&response, CONTENT_TYPE), "application/pdf");
    assert!(header(&response, CONTENT_DISPOSITION).starts_with("inline;"));
    assert!(!header(&response, CONTENT_SECURITY_POLICY).contains("sandbox"));
    assert!(header(&response, CONTENT_SECURITY_POLICY).contains("frame-ancestors 'self'"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn folders_trash_and_other_users_are_404(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let ((alice, alice_me), (bob, _)) = common::two_users(&instance).await;
    let parent = files::root_id(&instance, &alice_me, "Files").await;
    let node = files::upload(&alice, &alice_me, parent, "a.txt", b"secret").await;
    let id = node["id"].as_str().unwrap();

    bob.get(&format!("/api/files/{id}/content")).await.assert_status(StatusCode::NOT_FOUND);
    alice.get(&format!("/api/files/{parent}/content")).await.assert_status(StatusCode::NOT_FOUND);
    alice
        .delete(&format!("/api/nodes/{id}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    alice.get(&format!("/api/files/{id}/content")).await.assert_status(StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn other_api_responses_stay_no_store(pool: PgPool) {
    let browser = common::server(pool);
    common::setup_admin(&browser, "alice").await;
    assert_eq!(header(&browser.get("/api/nodes").await, CACHE_CONTROL), "no-store");
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test content` → FAIL (404).

**Step 3: Implement** — `src/routes/content.rs`:
```rust
use std::io::SeekFrom;

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_SECURITY_POLICY,
    CONTENT_TYPE, ETAG, IF_NONE_MATCH, RANGE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use squirrel_core::{ContentHash, NodeKind};
use squirrel_db::nodes::{self, NodeRecord};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::content::{ContentPolicy, RangeRequest, content_disposition, etag_matches, resolve_range};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::input::{ApiPath, ApiQuery};
use crate::state::AppState;

/// Browsers may keep a copy but must revalidate it (auth + ETag) every time.
const CACHE_POLICY: &str = "private, no-cache";
const STREAM_BUFFER: usize = 64 * 1024;
const CROSS_ORIGIN_RESOURCE_POLICY: HeaderName = HeaderName::from_static("cross-origin-resource-policy");

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ContentQuery {
    /// `true` forces `Content-Disposition: attachment`.
    #[serde(default)]
    pub download: bool,
}

#[utoipa::path(get, path = "/api/files/{id}/content", tag = "files",
    params(("id" = Uuid, Path, description = "File id"), ContentQuery),
    responses(
        (status = 200, body = Vec<u8>, description = "The file; active content types are served as application/octet-stream"),
        (status = 206, body = Vec<u8>, description = "One byte range"),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 404, body = ErrorBody),
        (status = 416, body = ErrorBody),
    ))]
pub async fn file_content(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let node = nodes::find_live(&state.pool, current.user.id, id)
        .await?
        .filter(|node| node.kind() == NodeKind::File)
        .ok_or(ApiError::NotFound)?;
    let hash = blob_hash(&node)?;
    let etag = format!("\"{hash}\"");
    if etag_matches(header_str(&headers, IF_NONE_MATCH), &etag) {
        let headers = [(ETAG, etag), (CACHE_CONTROL, CACHE_POLICY.to_owned())];
        return Ok((StatusCode::NOT_MODIFIED, headers).into_response());
    }

    let (store, blob) = (state.storage.clone(), hash.clone());
    let (file, size) = run_storage(move || store.open_blob(&blob)).await.inspect_err(|err| {
        if matches!(err, ApiError::NotFound) {
            tracing::error!(node_id = %id, %hash, "blob missing from the data directory");
        }
    })?;
    let (status, start, len, content_range) = match resolve_range(header_str(&headers, RANGE), size) {
        RangeRequest::Full => (StatusCode::OK, 0, size, None),
        RangeRequest::Partial(range) => {
            let (start, end) = (*range.start(), *range.end());
            (StatusCode::PARTIAL_CONTENT, start, end - start + 1, Some(format!("bytes {start}-{end}/{size}")))
        }
        RangeRequest::Unsatisfiable => {
            let headers = [(CONTENT_RANGE, format!("bytes */{size}"))];
            return Ok((headers, ApiError::RangeNotSatisfiable).into_response());
        }
    };
    let policy = ContentPolicy::for_mime(node.mime.as_deref(), query.download);
    let response_headers = content_headers(&policy, &node.name, &etag, len, content_range)?;
    let body = stream(file, start, len).await?;
    Ok((status, response_headers, body).into_response())
}

fn blob_hash(node: &NodeRecord) -> Result<ContentHash, ApiError> {
    node.blob_sha256
        .as_deref()
        .and_then(|sha| ContentHash::parse(sha).ok())
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("file node {} has no valid blob hash", node.id)))
}

fn header_str(headers: &HeaderMap, name: HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn content_headers(
    policy: &ContentPolicy,
    name: &str,
    etag: &str,
    len: u64,
    content_range: Option<String>,
) -> Result<HeaderMap, ApiError> {
    let mut pairs = vec![
        (CONTENT_TYPE, policy.content_type.clone()),
        (CONTENT_LENGTH, len.to_string()),
        (CONTENT_DISPOSITION, content_disposition(policy.inline, name)),
        (CONTENT_SECURITY_POLICY, policy.csp.to_owned()),
        (ETAG, etag.to_owned()),
        (CACHE_CONTROL, CACHE_POLICY.to_owned()),
        (ACCEPT_RANGES, "bytes".to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        (X_FRAME_OPTIONS, "SAMEORIGIN".to_owned()),
        (CROSS_ORIGIN_RESOURCE_POLICY, "same-origin".to_owned()),
    ];
    if let Some(range) = content_range {
        pairs.push((CONTENT_RANGE, range));
    }
    pairs
        .into_iter()
        .map(|(name, value)| HeaderValue::try_from(value).map(|value| (name, value)))
        .collect::<Result<HeaderMap, _>>()
        .map_err(|err| ApiError::Internal(err.into()))
}

async fn stream(file: std::fs::File, start: u64, len: u64) -> Result<Body, ApiError> {
    let mut file = tokio::fs::File::from_std(file);
    file.seek(SeekFrom::Start(start))
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;
    Ok(Body::from_stream(ReaderStream::with_capacity(file.take(len), STREAM_BUFFER)))
}
```
The global security-header layers use `if_not_present`, so this handler's CSP and `X-Frame-Options: SAMEORIGIN` win over the app defaults (`DENY`, `frame-ancestors 'none'`), which is what lets the SPA show PDFs in an `<iframe>`.

`src/lib.rs` — long-running transfer group:
```rust
    let long_running = OpenApiRouter::new()
        .routes(routes!(routes::uploads::complete::complete_upload))
        .routes(routes!(routes::content::file_content))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT));
```

**Step 4: Run tests** — `cargo test -p squirrel-api --test content` → PASS (8). Regenerate the contract; workspace green.

**Step 5: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat(api): download files with ranges, ETags and a safe content policy" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 20: Storage usage, trash purge, blob GC, `admin set-quota`, health

**Files:**
- Create: `crates/squirrel-api/src/routes/storage.rs`, `crates/squirrel-api/tests/storage.rs`, `crates/squirrel/src/admin.rs`
- Modify: `crates/squirrel-api/src/maintenance.rs`, `src/routes/health.rs`, `src/lib.rs`, `src/routes/mod.rs`, `crates/squirrel-api/tests/maintenance.rs`
- Modify: `crates/squirrel/src/main.rs`, `crates/squirrel/src/tasks.rs`, `crates/squirrel/Cargo.toml` (`squirrel-core.workspace = true`, `squirrel-db` is already there)

**Step 1: Write the failing tests**

`crates/squirrel-api/tests/storage.rs`:
```rust
mod common;

use axum::http::StatusCode;
use common::files;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_core::Username;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn usage_counts_live_files_and_trash(pool: PgPool) {
    let instance = common::Instance::new(pool.clone());
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    files::upload(&browser, &me, parent, "a.bin", &[1; 100]).await;
    let b = files::upload(&browser, &me, parent, "b.bin", &[2; 30]).await;
    browser
        .delete(&format!("/api/nodes/{}", b["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    squirrel_db::users::set_quota(&pool, &Username::parse("alice").unwrap(), Some(1000)).await.unwrap();

    let usage: Value = browser.get("/api/storage").await.json();
    assert_eq!(usage, json!({ "used_bytes": 100, "trash_bytes": 30, "quota_bytes": 1000 }));
}
```

Add to `crates/squirrel-api/tests/maintenance.rs` (imports at the top of the file):
```rust
use squirrel_core::ContentHash;
use time::Duration;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn old_trash_is_purged_and_unreferenced_blobs_collected(pool: PgPool) {
    let config = squirrel_api::ApiConfig {
        trash_retention: Duration::ZERO,
        blob_gc_grace: Duration::ZERO,
        ..common::test_config()
    };
    let instance = common::Instance::with_config(pool, config);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let gone = files::upload(&browser, &me, parent, "gone.txt", b"gone").await;
    files::upload(&browser, &me, parent, "kept.txt", b"kept").await;
    browser
        .delete(&format!("/api/nodes/{}", gone["id"].as_str().unwrap()))
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);

    assert_eq!(maintenance::purge_old_trash(&instance.state).await.unwrap(), 1);
    assert_eq!(maintenance::collect_garbage(&instance.state).await.unwrap(), 1);

    let store = &instance.state.storage;
    assert!(!store.contains(&ContentHash::parse(&files::sha256_hex(b"gone")).unwrap()));
    assert!(store.contains(&ContentHash::parse(&files::sha256_hex(b"kept")).unwrap()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn gc_waits_for_the_grace_period(pool: PgPool) {
    let instance = common::Instance::new(pool);
    let browser = instance.browser();
    let me = common::setup_admin(&browser, "alice").await;
    let parent = files::root_id(&instance, &me, "Files").await;
    let node = files::upload(&browser, &me, parent, "a.txt", b"recent").await;
    let id = node["id"].as_str().unwrap();
    for path in [format!("/api/nodes/{id}"), format!("/api/trash/{id}")] {
        browser.delete(&path).add_header(common::CSRF, common::csrf(&me)).await;
    }
    assert_eq!(maintenance::collect_garbage(&instance.state).await.unwrap(), 0);
    assert!(instance.state.storage.contains(&ContentHash::parse(&files::sha256_hex(b"recent")).unwrap()));
}
```

`crates/squirrel/src/admin.rs` test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotas_are_bytes_or_unlimited() {
        assert_eq!(parse_quota("1048576"), Ok(Quota(Some(1_048_576))));
        assert_eq!(parse_quota("Unlimited"), Ok(Quota(None)));
        assert!(parse_quota("-1").is_err());
        assert!(parse_quota("10G").is_err());
    }
}
```

**Step 2: Run to verify failure** — `cargo test -p squirrel-api --test storage --test maintenance && cargo test -p squirrel` → FAIL.

**Step 3: Implement**

`src/routes/storage.rs`:
```rust
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use squirrel_db::quota;
use utoipa::ToSchema;

use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct StorageUsageDto {
    /// Logical bytes of live files: what the quota limits.
    pub used_bytes: i64,
    /// Bytes in the trash (purged after 30 days, not counted against the quota).
    pub trash_bytes: i64,
    /// `null`: unlimited.
    pub quota_bytes: Option<i64>,
}

#[utoipa::path(get, path = "/api/storage", tag = "storage",
    responses((status = 200, body = StorageUsageDto), (status = 401, body = ErrorBody)))]
pub async fn storage_usage(State(state): State<AppState>, current: CurrentUser) -> Result<Json<StorageUsageDto>, ApiError> {
    let usage = quota::usage(&state.pool, current.user.id).await?;
    Ok(Json(StorageUsageDto {
        used_bytes: usage.used_bytes,
        trash_bytes: usage.trash_bytes,
        quota_bytes: usage.quota_bytes,
    }))
}
```
Register in `json_routes()`: `.routes(routes!(routes::storage::storage_usage))`.

`src/maintenance.rs` — append:
```rust
use squirrel_core::ContentHash;
use squirrel_db::{DbError, blobs, trash};
use sqlx::PgConnection;
use time::OffsetDateTime;

/// Trash entries removed per statement.
const TRASH_BATCH: i64 = 500;
/// Blobs examined per GC run; the rest wait for the next run.
const GC_BATCH: i64 = 1000;

/// Deletes trash entries older than the retention period (30 days).
pub async fn purge_old_trash(state: &AppState) -> Result<u64, ApiError> {
    let cutoff = OffsetDateTime::now_utc() - state.config.trash_retention;
    Ok(trash::purge_older_than(&state.pool, cutoff, TRASH_BATCH).await?)
}

/// Deletes blobs unreferenced for longer than the grace period: the row first,
/// then the file, under the blob's session-level advisory lock so a concurrent
/// upload of the same content cannot link to a file being removed.
pub async fn collect_garbage(state: &AppState) -> Result<usize, ApiError> {
    let cutoff = OffsetDateTime::now_utc() - state.config.blob_gc_grace;
    let candidates = blobs::unreferenced_since(&state.pool, cutoff, GC_BATCH).await?;
    let mut removed = 0;
    for hash in &candidates {
        if remove_if_unreferenced(state, hash, cutoff).await? {
            removed += 1;
        }
    }
    Ok(removed)
}

async fn remove_if_unreferenced(state: &AppState, hash: &ContentHash, cutoff: OffsetDateTime) -> Result<bool, ApiError> {
    let mut conn = state.pool.acquire().await.map_err(DbError::from)?;
    blobs::lock_session(&mut conn, hash).await?;
    let result = delete_row_then_file(state, &mut conn, hash, cutoff).await;
    if let Err(err) = blobs::unlock_session(&mut conn, hash).await {
        // Never hand a connection that still holds the lock back to the pool.
        tracing::error!(error = %err, %hash, "could not release a blob lock; closing the connection");
        conn.close_on_drop();
    }
    result
}

async fn delete_row_then_file(
    state: &AppState,
    conn: &mut PgConnection,
    hash: &ContentHash,
    cutoff: OffsetDateTime,
) -> Result<bool, ApiError> {
    if !blobs::delete_if_unreferenced(conn, hash, cutoff).await? {
        return Ok(false);
    }
    // Row first: a crash between the two leaves an orphan file, never a row without its file.
    let (store, blob) = (state.storage.clone(), hash.clone());
    run_storage(move || store.remove_blob(&blob)).await?;
    tracing::debug!(%hash, "blob deleted");
    Ok(true)
}
```

`src/routes/health.rs` — also prove the data directory is reachable (design: "/healthz (DB + disk)"):
```rust
    let store = state.storage.clone();
    run_storage(move || store.available_space()).await?;
```
(placed after the DB check; update the doc comment.)

`crates/squirrel/src/admin.rs`:
```rust
use clap::Subcommand;
use sqlx::PgPool;
use squirrel_core::Username;

#[derive(Subcommand)]
pub enum AdminCommand {
    /// Set a user's storage quota: a number of bytes, or `unlimited`.
    SetQuota {
        username: String,
        #[arg(value_parser = parse_quota)]
        quota: Quota,
    },
}

/// `None` is unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota(pub Option<i64>);

pub fn parse_quota(raw: &str) -> Result<Quota, String> {
    if raw.eq_ignore_ascii_case("unlimited") {
        return Ok(Quota(None));
    }
    raw.parse::<i64>()
        .ok()
        .filter(|bytes| *bytes >= 0)
        .map(|bytes| Quota(Some(bytes)))
        .ok_or_else(|| format!("expected a number of bytes or `unlimited`, got {raw:?}"))
}

pub async fn run(pool: &PgPool, command: AdminCommand) -> anyhow::Result<()> {
    match command {
        AdminCommand::SetQuota { username, quota } => {
            let username = Username::parse(&username)?;
            if !squirrel_db::users::set_quota(pool, &username, quota.0).await? {
                anyhow::bail!("no user named {}", username.as_str());
            }
            match quota.0 {
                Some(bytes) => println!("quota for {} set to {bytes} bytes", username.as_str()),
                None => println!("quota for {} removed (unlimited)", username.as_str()),
            }
            Ok(())
        }
    }
}
```
`main.rs`:
```rust
mod admin;
mod tasks;

enum Command {
    // … Serve, Migrate, Openapi …
    /// Administrative commands (run on the server).
    Admin(AdminArgs),
}

#[derive(Args)]
struct AdminArgs {
    #[command(flatten)]
    database: DatabaseArgs,
    #[command(subcommand)]
    command: admin::AdminCommand,
}

// in main():
        Command::Admin(args) => {
            init_tracing(false);
            let pool = connect_and_migrate(&args.database).await?;
            admin::run(&pool, args.command).await
        }
```

`tasks.rs` — one storage job every 6 hours: purge old trash, then collect garbage (so freshly purged blobs wait out their grace period on a later run):
```rust
const STORAGE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

    let storage_state = state.clone();
    spawn_periodic(STORAGE_INTERVAL, move || {
        let state = storage_state.clone();
        async move {
            match maintenance::purge_old_trash(&state).await {
                Ok(0) => {}
                Ok(purged) => tracing::info!(purged, "purged old trash entries"),
                Err(err) => tracing::error!(error = %err, "failed to purge the trash"),
            }
            match maintenance::collect_garbage(&state).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "deleted unreferenced blobs"),
                Err(err) => tracing::error!(error = %err, "blob garbage collection failed"),
            }
        }
    });
```

**Step 4: Run tests** — `cargo test --workspace` → PASS. Regenerate the contract.

**Step 5: Manual check**
```bash
SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
cargo run -q -p squirrel -- admin set-quota alice 1073741824
```
Expected: `quota for alice set to 1073741824 bytes` (or `no user named alice` on an empty dev DB, exit code 1).

**Step 6: Commit**
```bash
git add crates openapi.json web/src/lib/api/schema.d.ts
git commit -m "feat: storage usage, trash purge, blob GC and admin set-quota" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Milestone E — Web UI

Conventions: Svelte 5 runes, `#lib/...ts` imports with explicit extensions, the shared `api` client (CSRF + session middleware) for JSON calls, pure logic in `.ts` files with Vitest tests, components under `src/lib/components/files/`. Every web command runs in `web/` with `source ~/.nvm/nvm.sh && nvm use 22 >/dev/null &&`. Gate after each task: `pnpm check && pnpm test:unit && pnpm build`.

Phase 1 E2E strings stay unchanged ("Hello, Alice", "Invitations", "Log out", login/setup/join labels).

### Task 21: Client plumbing — dependencies, CSP, formatting, files API

**Files:**
- Modify: `web/package.json` (via pnpm), `web/vite.config.ts`, `web/src/lib/api/client.ts`
- Create: `web/src/lib/files/format.ts`, `format.test.ts`, `web/src/lib/files/pages.ts`, `pages.test.ts`, `web/src/lib/files/api.ts`

**Step 1: Dependencies and CSP**
```bash
cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm add hash-wasm@^4.12.0
```
`vite.config.ts` — hash-wasm compiles WebAssembly, which a CSP `script-src` without `'wasm-unsafe-eval'` forbids (it allows *only* Wasm compilation, not `eval`):
```ts
			csp: {
				mode: 'hash',
				directives: {
					'script-src': ['self', 'wasm-unsafe-eval'],
					'object-src': ['none'],
					'base-uri': ['none']
				}
			}
```
(The worker's own policy comes from its response headers, which have no `script-src`; the meta policy matters if hashing ever runs on the page. Keep both working.)

`client.ts` — expose the token for raw `fetch` uploads:
```ts
/** The current session's CSRF token, for requests made without the API client. */
export function getCsrfToken(): string | null {
	return csrfToken;
}
```

**Step 2: Write the failing tests**

`src/lib/files/format.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { contentUrl, formatBytes, previewKind, storageSummary } from '#lib/files/format.ts';

describe('formatBytes', () => {
	it.each([
		[0, '0 B'],
		[999, '999 B'],
		[1500, '1.5 KB'],
		[9_437_184, '9.4 MB'],
		[123_456_789_012, '123 GB'],
		[-1, '—']
	])('%d → %s', (bytes, expected) => expect(formatBytes(bytes)).toBe(expected));
});

describe('previewKind', () => {
	it.each([
		['application/pdf', 'pdf'],
		['image/png', 'image'],
		['image/svg+xml', null],
		['video/mp4', 'video'],
		['video/quicktime', null],
		['audio/mpeg', 'audio'],
		['text/plain', 'text'],
		['text/csv', 'text'],
		['application/json', 'text'],
		['text/html', null],
		['application/zip', null],
		[null, null]
	])('%s → %s', (mime, expected) => expect(previewKind(mime)).toBe(expected));
});

describe('contentUrl', () => {
	it('builds inline and download URLs', () => {
		expect(contentUrl('abc')).toBe('/api/files/abc/content');
		expect(contentUrl('abc', true)).toBe('/api/files/abc/content?download=true');
	});
});

describe('storageSummary', () => {
	it('mentions the quota only when there is one', () => {
		expect(storageSummary({ used_bytes: 1500, quota_bytes: null })).toBe('1.5 KB used');
		expect(storageSummary({ used_bytes: 1500, quota_bytes: 1_000_000 })).toBe('1.5 KB of 1.0 MB used');
	});
});
```

`src/lib/files/pages.test.ts`:
```ts
import { describe, expect, it, vi } from 'vitest';
import { collectPages } from '#lib/files/pages.ts';

describe('collectPages', () => {
	it('follows cursors until the last page', async () => {
		const fetchPage = vi.fn(async (cursor: string | null) =>
			cursor === null
				? { ok: true as const, data: { items: [1, 2], next_cursor: 'c2' } }
				: { ok: true as const, data: { items: [3], next_cursor: null } }
		);
		expect(await collectPages(fetchPage)).toEqual({ ok: true, data: [1, 2, 3] });
		expect(fetchPage).toHaveBeenNthCalledWith(2, 'c2');
	});

	it('stops at the first error', async () => {
		const failure = { ok: false as const, error: 'boom', status: 500 };
		expect(await collectPages(async () => failure)).toEqual(failure);
	});
});
```

**Step 3: Run to verify failure** — `pnpm test:unit` → FAIL (missing modules).

**Step 4: Implement**

`src/lib/files/format.ts`:
```ts
const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** Human-readable size in decimal units, e.g. `1.5 MB`. */
export function formatBytes(bytes: number): string {
	if (!Number.isFinite(bytes) || bytes < 0) return '—';
	let value = bytes;
	let unit = 0;
	while (value >= 1000 && unit < UNITS.length - 1) {
		value /= 1000;
		unit += 1;
	}
	const digits = unit === 0 || value >= 100 ? 0 : 1;
	return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

export type PreviewKind = 'image' | 'pdf' | 'video' | 'audio' | 'text';

// Must stay within what the server serves inline (crates/squirrel-api/src/content/policy.rs),
// except text, which the preview fetches rather than navigates to.
const IMAGES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif', 'image/bmp']);
const VIDEOS = new Set(['video/mp4', 'video/webm', 'video/ogg']);
const AUDIO = new Set(['audio/mpeg', 'audio/ogg', 'audio/wav', 'audio/webm', 'audio/flac', 'audio/mp4']);
const ACTIVE_TEXT = new Set(['text/html', 'text/xml', 'text/javascript', 'text/ecmascript', 'text/xsl']);

/** How a file can be previewed in the browser, or null if it can only be downloaded. */
export function previewKind(mime: string | null | undefined): PreviewKind | null {
	if (!mime) return null;
	if (mime === 'application/pdf') return 'pdf';
	if (IMAGES.has(mime)) return 'image';
	if (VIDEOS.has(mime)) return 'video';
	if (AUDIO.has(mime)) return 'audio';
	if (mime === 'application/json' || (mime.startsWith('text/') && !ACTIVE_TEXT.has(mime))) return 'text';
	return null;
}

export function contentUrl(id: string, download = false): string {
	return `/api/files/${encodeURIComponent(id)}/content${download ? '?download=true' : ''}`;
}

export function storageSummary(usage: { used_bytes: number; quota_bytes?: number | null }): string {
	const used = formatBytes(usage.used_bytes);
	return usage.quota_bytes == null ? `${used} used` : `${used} of ${formatBytes(usage.quota_bytes)} used`;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

export function formatDate(iso: string): string {
	return dateFormat.format(new Date(iso));
}
```

`src/lib/files/pages.ts`:
```ts
export type Outcome<T> = { ok: true; data: T } | { ok: false; error: string; status: number };

interface PageBody<T> {
	items: T[];
	next_cursor?: string | null;
}

/** Fetches every page of a keyset-paginated listing. */
export async function collectPages<T>(
	fetchPage: (cursor: string | null) => Promise<Outcome<PageBody<T>>>
): Promise<Outcome<T[]>> {
	const items: T[] = [];
	let cursor: string | null = null;
	do {
		const page = await fetchPage(cursor);
		if (!page.ok) return page;
		items.push(...page.data.items);
		cursor = page.data.next_cursor ?? null;
	} while (cursor);
	return { ok: true, data: items };
}
```

`src/lib/files/api.ts` (thin typed wrappers; their behaviour is covered by the API tests and the E2E):
```ts
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import { collectPages, type Outcome } from '#lib/files/pages.ts';

export type { Outcome } from '#lib/files/pages.ts';
export type FileNode = components['schemas']['NodeDto'];
export type PathSegment = components['schemas']['PathSegmentDto'];
export type NodeDetails = components['schemas']['NodeDetails'];
export type TrashEntry = components['schemas']['TrashEntryDto'];
export type StorageUsage = components['schemas']['StorageUsageDto'];

/** Folders hold at most a few thousand entries at home: load them all. */
const PAGE_SIZE = 500;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export function listFolder(parentId: string | null): Promise<Outcome<FileNode[]>> {
	return collectPages((cursor) =>
		call(
			api.GET('/api/nodes', {
				params: { query: { parent_id: parentId ?? undefined, cursor: cursor ?? undefined, limit: PAGE_SIZE } }
			})
		)
	);
}

export const getNode = (id: string) => call(api.GET('/api/nodes/{id}', { params: { path: { id } } }));

export const createFolder = (parentId: string, name: string) =>
	call(api.POST('/api/folders', { body: { parent_id: parentId, name } }));

export const renameNode = (id: string, name: string) =>
	call(api.PATCH('/api/nodes/{id}', { params: { path: { id } }, body: { name } }));

export const moveNode = (id: string, parentId: string) =>
	call(api.PATCH('/api/nodes/{id}', { params: { path: { id } }, body: { parent_id: parentId } }));

export const trashNode = (id: string) => call(api.DELETE('/api/nodes/{id}', { params: { path: { id } } }));

export function listTrash(): Promise<Outcome<TrashEntry[]>> {
	return collectPages((cursor) =>
		call(api.GET('/api/trash', { params: { query: { cursor: cursor ?? undefined, limit: PAGE_SIZE } } }))
	);
}

export const restoreEntry = (id: string) =>
	call(api.POST('/api/trash/{id}/restore', { params: { path: { id } } }));

export const deleteForever = (id: string) => call(api.DELETE('/api/trash/{id}', { params: { path: { id } } }));

export const emptyTrash = () => call(api.DELETE('/api/trash'));

export const storageUsage = () => call(api.GET('/api/storage'));
```
If `pnpm check` complains about a body or query shape, the generated `schema.d.ts` is the reference — adjust the call, not the schema.

**Step 5: Run** — `pnpm check && pnpm test:unit` → PASS.

**Step 6: Commit**
```bash
git add web
git commit -m "feat(web): add files API client, formatting and wasm CSP" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 22: Hashing worker and resumable uploader

**Files:**
- Create: `web/src/lib/upload/hash-core.ts`, `hash-core.test.ts`, `hash.worker.ts`, `hasher.ts`
- Create: `web/src/lib/upload/plan.ts`, `plan.test.ts`, `uploader.ts`, `uploader.test.ts`, `transport.ts`

**Step 1: Write the failing tests**

`hash-core.test.ts` (Vitest runs in Node 22, which has `Blob` and runs hash-wasm):
```ts
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { HASH_SLICE, hashBlob } from '#lib/upload/hash-core.ts';

describe('hashBlob', () => {
	it('matches SHA-256 across slice boundaries and reports progress', async () => {
		const bytes = new Uint8Array(HASH_SLICE * 2 + 123).map((_, i) => i % 251);
		const progress: number[] = [];
		const hash = await hashBlob(new Blob([bytes]), (done) => progress.push(done));
		expect(hash).toBe(createHash('sha256').update(bytes).digest('hex'));
		expect(progress).toEqual([HASH_SLICE, HASH_SLICE * 2, bytes.length]);
	});

	it('hashes empty files', async () => {
		expect(await hashBlob(new Blob([]))).toBe(
			'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
		);
	});

	it('stops when asked', async () => {
		await expect(hashBlob(new Blob([new Uint8Array(10)]), () => {}, () => true)).rejects.toThrow(/cancel/i);
	});
});
```

`plan.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { chunkRange, missingChunks } from '#lib/upload/plan.ts';

describe('upload plan', () => {
	it('lists chunks not yet received', () => {
		expect(missingChunks(4, [1, 3])).toEqual([0, 2]);
		expect(missingChunks(0, [])).toEqual([]);
	});

	it('computes byte ranges with a short last chunk', () => {
		expect(chunkRange(0, 4, 10)).toEqual([0, 4]);
		expect(chunkRange(2, 4, 10)).toEqual([8, 10]);
	});
});
```

`uploader.test.ts`:
```ts
import { describe, expect, it, vi } from 'vitest';
import { UploadError, uploadFile, type UploadDeps, type UploadTransport } from '#lib/upload/uploader.ts';

const node = { id: 'n1', name: 'a.bin' } as never;

function session(chunkCount: number, received: number[] = []) {
	return {
		kind: 'session' as const,
		session: { upload_id: 'u1', chunk_size: 4, chunk_count: chunkCount, received_chunks: received, expires_at: '' }
	};
}

function deps(transport: Partial<UploadTransport>): UploadDeps & { transport: UploadTransport } {
	return {
		transport: {
			start: vi.fn(async () => session(3)),
			putChunk: vi.fn(async () => {}),
			complete: vi.fn(async () => node),
			abort: vi.fn(async () => {}),
			...transport
		},
		hash: vi.fn(async () => 'f'.repeat(64)),
		sleep: async () => {},
		concurrency: 2
	};
}

const file = () => new File([new Uint8Array(10)], 'a.bin');
const run = (d: UploadDeps, signal = new AbortController().signal) => uploadFile(file(), 'p1', d, () => {}, signal);

describe('uploadFile', () => {
	it('links known content without sending chunks', async () => {
		const d = deps({ start: vi.fn(async () => ({ kind: 'deduplicated' as const, node })) });
		expect(await run(d)).toEqual({ node, deduplicated: true });
		expect(d.transport.putChunk).not.toHaveBeenCalled();
	});

	it('sends only the missing chunks with the right sizes, then completes', async () => {
		const d = deps({ start: vi.fn(async () => session(3, [1])) });
		const progress: number[] = [];
		await uploadFile(file(), 'p1', d, (p) => progress.push(p.done), new AbortController().signal);
		const calls = vi.mocked(d.transport.putChunk).mock.calls.map(([, index, body]) => [index, body.size]);
		expect(calls.sort()).toEqual([
			[0, 4],
			[2, 2]
		]);
		expect(d.transport.complete).toHaveBeenCalledWith('u1', expect.anything());
		expect(progress.at(-1)).toBe(10);
	});

	it('retries transient chunk failures', async () => {
		let failures = 1;
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				if (failures-- > 0) throw new UploadError('busy', 503, true);
			})
		});
		await run(d);
		expect(d.transport.putChunk).toHaveBeenCalledTimes(2);
	});

	it('does not retry client errors and keeps the session for a later retry', async () => {
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				throw new UploadError('quota', 507, false);
			})
		});
		await expect(run(d)).rejects.toThrow('quota');
		expect(d.transport.putChunk).toHaveBeenCalledTimes(1);
		expect(d.transport.abort).not.toHaveBeenCalled();
	});

	it('aborts the server session when cancelled', async () => {
		const controller = new AbortController();
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				controller.abort();
				throw new DOMException('aborted', 'AbortError');
			})
		});
		await expect(run(d, controller.signal)).rejects.toThrow();
		expect(d.transport.abort).toHaveBeenCalledWith('u1');
	});
});
```

**Step 2: Run to verify failure** — `pnpm test:unit` → FAIL.

**Step 3: Implement**

`hash-core.ts`:
```ts
import { createSHA256 } from 'hash-wasm';

/** Bytes read per step: memory stays flat whatever the file size. */
export const HASH_SLICE = 4 * 1024 * 1024;

/** SHA-256 of a Blob as lowercase hex (the server's ContentHash format). */
export async function hashBlob(
	blob: Blob,
	onProgress: (hashedBytes: number) => void = () => {},
	shouldStop: () => boolean = () => false
): Promise<string> {
	const hasher = await createSHA256();
	hasher.init();
	for (let offset = 0; offset < blob.size; offset += HASH_SLICE) {
		if (shouldStop()) throw new DOMException('Hashing was cancelled', 'AbortError');
		const end = Math.min(offset + HASH_SLICE, blob.size);
		hasher.update(new Uint8Array(await blob.slice(offset, end).arrayBuffer()));
		onProgress(end);
	}
	return hasher.digest('hex');
}
```

`hash.worker.ts` (typed without the `webworker` lib, which clashes with the DOM lib in `svelte-check`):
```ts
import { hashBlob } from './hash-core.ts';

export type HashMessage =
	| { type: 'progress'; hashed: number }
	| { type: 'done'; sha256: string }
	| { type: 'error'; message: string };

const scope = self as unknown as {
	onmessage: ((event: MessageEvent<{ file: Blob }>) => void) | null;
	postMessage(message: HashMessage): void;
};

scope.onmessage = async (event) => {
	try {
		const sha256 = await hashBlob(event.data.file, (hashed) => scope.postMessage({ type: 'progress', hashed }));
		scope.postMessage({ type: 'done', sha256 });
	} catch (error) {
		scope.postMessage({ type: 'error', message: error instanceof Error ? error.message : String(error) });
	}
};
```

`hasher.ts` (browser only; exercised by the E2E test):
```ts
import type { HashMessage } from './hash.worker.ts';

/** Hashes `file` in a module Web Worker so the page stays responsive. */
export function hashInWorker(file: Blob, onProgress: (hashed: number) => void, signal: AbortSignal): Promise<string> {
	return new Promise((resolve, reject) => {
		const worker = new Worker(new URL('./hash.worker.ts', import.meta.url), { type: 'module' });
		const finish = () => {
			worker.terminate();
			signal.removeEventListener('abort', onAbort);
		};
		const onAbort = () => {
			finish();
			reject(new DOMException('Upload cancelled', 'AbortError'));
		};
		if (signal.aborted) return onAbort();
		signal.addEventListener('abort', onAbort, { once: true });
		worker.onmessage = (event: MessageEvent<HashMessage>) => {
			const message = event.data;
			if (message.type === 'progress') return onProgress(message.hashed);
			finish();
			if (message.type === 'done') resolve(message.sha256);
			else reject(new Error(message.message));
		};
		worker.onerror = (event) => {
			finish();
			reject(new Error(event.message || 'Could not compute the file checksum'));
		};
		worker.postMessage({ file });
	});
}
```

`plan.ts`:
```ts
/** Indexes in `0..chunkCount` that the server has not received yet. */
export function missingChunks(chunkCount: number, received: readonly number[]): number[] {
	const have = new Set(received);
	return Array.from({ length: chunkCount }, (_, i) => i).filter((i) => !have.has(i));
}

/** `[start, end)` byte range of chunk `index`. */
export function chunkRange(index: number, chunkSize: number, size: number): [number, number] {
	const start = index * chunkSize;
	return [start, Math.min(start + chunkSize, size)];
}
```

`uploader.ts`:
```ts
import type { components } from '#lib/api/schema.d.ts';
import { chunkRange, missingChunks } from '#lib/upload/plan.ts';

type FileNode = components['schemas']['NodeDto'];
type UploadSession = components['schemas']['UploadSessionDto'];

/** An HTTP failure; `retryable` for transient server-side conditions. */
export class UploadError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly retryable: boolean
	) {
		super(message);
	}
}

export type StartResult = { kind: 'deduplicated'; node: FileNode } | { kind: 'session'; session: UploadSession };

export interface UploadTransport {
	start(
		request: { parent_id: string; name: string; size: number; sha256: string },
		signal: AbortSignal
	): Promise<StartResult>;
	putChunk(uploadId: string, index: number, body: Blob, signal: AbortSignal): Promise<void>;
	complete(uploadId: string, signal: AbortSignal): Promise<FileNode>;
	abort(uploadId: string): Promise<void>;
}

export type UploadPhase = 'hashing' | 'uploading' | 'finishing';

export interface UploadProgress {
	phase: UploadPhase;
	done: number;
	total: number;
}

export interface UploadDeps {
	transport: UploadTransport;
	hash: (file: Blob, onProgress: (hashed: number) => void, signal: AbortSignal) => Promise<string>;
	sleep?: (ms: number) => Promise<void>;
	/** Chunks in flight at once. */
	concurrency?: number;
	/** Attempts per chunk for transient failures. */
	maxAttempts?: number;
}

export interface UploadResult {
	node: FileNode;
	deduplicated: boolean;
}

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Hash → start (dedup or session) → send missing chunks with retries → complete.
 * A failure keeps the server session, so calling again resumes where it stopped;
 * cancelling (aborting `signal`) deletes it.
 */
export async function uploadFile(
	file: File,
	parentId: string,
	deps: UploadDeps,
	onProgress: (progress: UploadProgress) => void,
	signal: AbortSignal
): Promise<UploadResult> {
	const total = file.size;
	const sha256 = await deps.hash(file, (done) => onProgress({ phase: 'hashing', done, total }), signal);
	const started = await deps.transport.start({ parent_id: parentId, name: file.name, size: total, sha256 }, signal);
	if (started.kind === 'deduplicated') return { node: started.node, deduplicated: true };

	const { session } = started;
	try {
		const missing = missingChunks(session.chunk_count, session.received_chunks);
		let sent = total - missing.reduce((sum, i) => sum + rangeLength(chunkRange(i, session.chunk_size, total)), 0);
		onProgress({ phase: 'uploading', done: sent, total });
		await runPool(missing, deps.concurrency ?? 3, async (index) => {
			const [start, end] = chunkRange(index, session.chunk_size, total);
			await withRetry(
				() => deps.transport.putChunk(session.upload_id, index, file.slice(start, end), signal),
				deps,
				signal
			);
			sent += end - start;
			onProgress({ phase: 'uploading', done: sent, total });
		});
		onProgress({ phase: 'finishing', done: total, total });
		// Not retried automatically: if the response was lost, the file may already exist.
		const node = await deps.transport.complete(session.upload_id, signal);
		return { node, deduplicated: false };
	} catch (error) {
		if (signal.aborted) await deps.transport.abort(session.upload_id).catch(() => undefined);
		throw error;
	}
}

function rangeLength([start, end]: [number, number]): number {
	return end - start;
}

async function withRetry<T>(operation: () => Promise<T>, deps: UploadDeps, signal: AbortSignal): Promise<T> {
	const attempts = deps.maxAttempts ?? 4;
	const sleep = deps.sleep ?? delay;
	for (let attempt = 1; ; attempt += 1) {
		try {
			return await operation();
		} catch (error) {
			if (signal.aborted || !isTransient(error) || attempt >= attempts) throw error;
			await sleep(500 * 2 ** (attempt - 1));
		}
	}
}

/** Server said "try again", or the network failed (fetch rejects with a TypeError). */
function isTransient(error: unknown): boolean {
	return error instanceof UploadError ? error.retryable : error instanceof TypeError;
}

/** Runs `worker` over `items` with at most `concurrency` in flight; stops on the first failure. */
async function runPool<T>(items: T[], concurrency: number, worker: (item: T) => Promise<void>): Promise<void> {
	let next = 0;
	let failed = false;
	const lane = async () => {
		while (!failed && next < items.length) {
			const item = items[next++];
			try {
				await worker(item);
			} catch (error) {
				failed = true;
				throw error;
			}
		}
	};
	await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, lane));
}
```

`transport.ts`:
```ts
import { api, errorMessage, getCsrfToken } from '#lib/api/client.ts';
import { UploadError, type UploadTransport } from '#lib/upload/uploader.ts';

function uploadError(error: unknown, response: Response): UploadError {
	const status = response.status;
	const retryable = status === 408 || status === 429 || (status >= 500 && status !== 507);
	return new UploadError(errorMessage(error, response), status, retryable);
}

/** JSON calls go through the API client; chunks are raw `PUT`s with the CSRF header. */
export function createTransport(
	fetchFn: typeof fetch = (input, init) => fetch(input, init)
): UploadTransport {
	return {
		async start(body, signal) {
			const { data, error, response } = await api.POST('/api/uploads', { body, signal });
			if (!response.ok || !data) throw uploadError(error, response);
			return 'upload_id' in data ? { kind: 'session', session: data } : { kind: 'deduplicated', node: data.node };
		},
		async putChunk(uploadId, index, body, signal) {
			const headers: Record<string, string> = { 'Content-Type': 'application/octet-stream' };
			const csrf = getCsrfToken();
			if (csrf) headers['X-CSRF-Token'] = csrf;
			const response = await fetchFn(`/api/uploads/${encodeURIComponent(uploadId)}/chunks/${index}`, {
				method: 'PUT',
				body,
				headers,
				signal,
				credentials: 'same-origin'
			});
			if (!response.ok) throw uploadError(await response.json().catch(() => null), response);
		},
		async complete(uploadId, signal) {
			const { data, error, response } = await api.POST('/api/uploads/{id}/complete', {
				params: { path: { id: uploadId } },
				signal
			});
			if (!data) throw uploadError(error, response);
			return data;
		},
		async abort(uploadId) {
			await api.DELETE('/api/uploads/{id}', { params: { path: { id: uploadId } } });
		}
	};
}
```

**Step 4: Run** — `pnpm check && pnpm test:unit` → PASS (10 new tests).

**Step 5: Commit**
```bash
git add web
git commit -m "feat(web): hash files in a worker and upload missing chunks with retries" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 23: Upload queue and upload panel

**Files:**
- Create: `web/src/lib/upload/queue-logic.ts`, `queue-logic.test.ts`, `web/src/lib/upload/uploads.svelte.ts`
- Create: `web/src/lib/components/files/UploadPanel.svelte`, `web/src/lib/components/files/DropZone.svelte`

**Step 1: Write the failing tests** — `queue-logic.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { progressPercent, statusLabel, type QueueEntry } from '#lib/upload/queue-logic.ts';

const entry = (over: Partial<QueueEntry>): QueueEntry => ({
	status: 'queued',
	done: 0,
	total: 100,
	error: null,
	deduplicated: false,
	finalName: null,
	name: 'a.bin',
	...over
});

describe('statusLabel', () => {
	it.each([
		[entry({}), 'Waiting'],
		[entry({ status: 'hashing', done: 40 }), 'Checking 40%'],
		[entry({ status: 'uploading', done: 1500, total: 3000 }), 'Uploading 1.5 KB of 3.0 KB'],
		[entry({ status: 'finishing' }), 'Finishing'],
		[entry({ status: 'done', deduplicated: true }), 'Already on your drive'],
		[entry({ status: 'done', finalName: 'a (1).bin' }), 'Saved as a (1).bin'],
		[entry({ status: 'done', finalName: 'a.bin' }), 'Done'],
		[entry({ status: 'failed', error: 'this would exceed your storage quota' }), 'this would exceed your storage quota'],
		[entry({ status: 'cancelled' }), 'Cancelled']
	])('%o → %s', (item, expected) => expect(statusLabel(item)).toBe(expected));
});

describe('progressPercent', () => {
	it('handles empty files and clamps', () => {
		expect(progressPercent(entry({ total: 0 }))).toBe(0);
		expect(progressPercent(entry({ status: 'done', total: 0 }))).toBe(100);
		expect(progressPercent(entry({ done: 50 }))).toBe(50);
	});
});
```

**Step 2: Run to verify failure** — `pnpm test:unit` → FAIL.

**Step 3: Implement**

`queue-logic.ts`:
```ts
import { formatBytes } from '#lib/files/format.ts';
import type { UploadPhase } from '#lib/upload/uploader.ts';

export type UploadStatus = 'queued' | UploadPhase | 'done' | 'failed' | 'cancelled';

export interface QueueEntry {
	name: string;
	status: UploadStatus;
	done: number;
	total: number;
	error: string | null;
	deduplicated: boolean;
	finalName: string | null;
}

export function progressPercent(entry: QueueEntry): number {
	if (entry.status === 'done') return 100;
	if (entry.total <= 0) return 0;
	return Math.min(100, Math.round((entry.done / entry.total) * 100));
}

export function statusLabel(entry: QueueEntry): string {
	switch (entry.status) {
		case 'queued':
			return 'Waiting';
		case 'hashing':
			return `Checking ${progressPercent(entry)}%`;
		case 'uploading':
			return `Uploading ${formatBytes(entry.done)} of ${formatBytes(entry.total)}`;
		case 'finishing':
			return 'Finishing';
		case 'done':
			if (entry.deduplicated) return 'Already on your drive';
			return entry.finalName && entry.finalName !== entry.name ? `Saved as ${entry.finalName}` : 'Done';
		case 'failed':
			return entry.error ?? 'Upload failed';
		case 'cancelled':
			return 'Cancelled';
	}
}
```

`uploads.svelte.ts` — one queue for the whole app, one file at a time (chunks of that file in parallel):
```ts
import type { QueueEntry } from '#lib/upload/queue-logic.ts';
import { hashInWorker } from '#lib/upload/hasher.ts';
import { createTransport } from '#lib/upload/transport.ts';
import { uploadFile } from '#lib/upload/uploader.ts';

export interface UploadItem extends QueueEntry {
	id: number;
	file: File;
	parentId: string;
}

export const uploadQueue: { items: UploadItem[] } = $state({ items: [] });

const transport = createTransport();
const controllers = new Map<number, AbortController>();
let nextId = 1;
let running = false;
let finishedHandler: ((parentId: string) => void) | null = null;

/** Called after each successful upload, e.g. to refresh the folder being shown. */
export function onUploadFinished(handler: ((parentId: string) => void) | null): void {
	finishedHandler = handler;
}

export function enqueue(files: Iterable<File>, parentId: string): void {
	for (const file of files) {
		uploadQueue.items.push({
			id: nextId++,
			file,
			parentId,
			name: file.name,
			status: 'queued',
			done: 0,
			total: file.size,
			error: null,
			deduplicated: false,
			finalName: null
		});
	}
	void pump();
}

async function pump(): Promise<void> {
	if (running) return;
	running = true;
	try {
		for (let item = nextQueued(); item; item = nextQueued()) await run(item);
	} finally {
		running = false;
	}
}

function nextQueued(): UploadItem | undefined {
	return uploadQueue.items.find((item) => item.status === 'queued');
}

async function run(item: UploadItem): Promise<void> {
	const controller = new AbortController();
	controllers.set(item.id, controller);
	item.status = 'hashing';
	item.error = null;
	try {
		const result = await uploadFile(
			item.file,
			item.parentId,
			{ transport, hash: hashInWorker },
			(progress) => {
				item.status = progress.phase;
				item.done = progress.done;
				item.total = progress.total;
			},
			controller.signal
		);
		item.status = 'done';
		item.deduplicated = result.deduplicated;
		item.finalName = result.node.name;
		finishedHandler?.(item.parentId);
	} catch (error) {
		if (controller.signal.aborted) {
			item.status = 'cancelled';
		} else {
			item.status = 'failed';
			item.error = error instanceof Error ? error.message : String(error);
		}
	} finally {
		controllers.delete(item.id);
	}
}

export function cancel(id: number): void {
	const item = uploadQueue.items.find((candidate) => candidate.id === id);
	if (item?.status === 'queued') item.status = 'cancelled';
	controllers.get(id)?.abort();
}

/** Resumes from the chunks the server already has. */
export function retry(id: number): void {
	const item = uploadQueue.items.find((candidate) => candidate.id === id);
	if (!item || (item.status !== 'failed' && item.status !== 'cancelled')) return;
	item.status = 'queued';
	item.error = null;
	void pump();
}

export function clearFinished(): void {
	uploadQueue.items = uploadQueue.items.filter((item) => item.status !== 'done' && item.status !== 'cancelled');
}
```

`UploadPanel.svelte`:
```svelte
<script lang="ts">
	import { progressPercent, statusLabel } from '#lib/upload/queue-logic.ts';
	import { cancel, clearFinished, retry, uploadQueue } from '#lib/upload/uploads.svelte.ts';

	const active = new Set(['queued', 'hashing', 'uploading', 'finishing']);
</script>

{#if uploadQueue.items.length > 0}
	<section class="uploads" aria-label="Uploads">
		<header>
			<h2>Uploads</h2>
			<button class="link" onclick={clearFinished}>Clear finished</button>
		</header>
		<ul>
			{#each uploadQueue.items as item (item.id)}
				<li data-testid="upload-item">
					<span class="name">{item.name}</span>
					<progress max="100" value={progressPercent(item)}></progress>
					<span class:error={item.status === 'failed'} role={item.status === 'failed' ? 'alert' : undefined}
						>{statusLabel(item)}</span
					>
					{#if active.has(item.status)}
						<button class="link" onclick={() => cancel(item.id)}>Cancel</button>
					{:else if item.status === 'failed' || item.status === 'cancelled'}
						<button class="link" onclick={() => retry(item.id)}>Retry</button>
					{/if}
				</li>
			{/each}
		</ul>
	</section>
{/if}

<style>
	.uploads { margin-top: 1.5rem; padding: 1rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
	header { display: flex; justify-content: space-between; align-items: center; }
	h2 { font-size: 1rem; margin: 0; }
	ul { list-style: none; padding: 0; margin: 0.5rem 0 0; display: grid; gap: 0.5rem; }
	li { display: grid; grid-template-columns: minmax(0, 1fr) 8rem auto auto; gap: 0.75rem; align-items: center; }
	.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	progress { width: 100%; }
</style>
```

`DropZone.svelte`:
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	let { enabled, onFiles, children }: { enabled: boolean; onFiles: (files: File[]) => void; children: Snippet } =
		$props();
	let over = $state(false);

	function hasFiles(event: DragEvent): boolean {
		return event.dataTransfer?.types.includes('Files') ?? false;
	}

	function dragOver(event: DragEvent) {
		if (!enabled || !hasFiles(event)) return;
		event.preventDefault();
		over = true;
	}

	function drop(event: DragEvent) {
		over = false;
		if (!enabled || !hasFiles(event)) return;
		event.preventDefault();
		// Folders dropped here arrive as zero-byte entries without a type in some browsers;
		// only regular files are uploaded (folder upload is not part of Phase 2).
		const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type !== '' || file.size > 0);
		if (files.length > 0) onFiles(files);
	}
</script>

<div
	class="dropzone"
	class:over
	role="region"
	aria-label="Drop files here to upload"
	ondragover={dragOver}
	ondragleave={() => (over = false)}
	ondrop={drop}
>
	{@render children()}
</div>

<style>
	.dropzone { border: 2px dashed transparent; border-radius: var(--radius); padding: 0.25rem; }
	.dropzone.over { border-color: var(--accent); }
</style>
```
(The empty-type filter drops genuinely empty, extension-less files too; acceptable — the upload button still handles them.)

**Step 4: Run** — `pnpm check && pnpm test:unit` → PASS.

**Step 5: Commit**
```bash
git add web
git commit -m "feat(web): add the upload queue, panel and drop zone" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 24: File browser pages

**Files:**
- Create: `web/src/lib/components/files/Breadcrumbs.svelte`, `FileTable.svelte`, `NameDialog.svelte`, `MoveDialog.svelte`, `StorageMeter.svelte`, `FolderView.svelte`
- Create: `web/src/routes/files/+page.svelte`, `web/src/routes/files/[id]/+page.svelte`
- Create: `web/src/lib/files/names.ts`, `names.test.ts`
- Modify: `web/src/routes/+layout.svelte` (topbar link), `web/src/routes/+page.svelte` (home link), `web/src/app.css` (a few shared styles)

**Step 1: Write the failing tests** — client-side name check mirroring `NodeName` (server stays authoritative), `names.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { nameProblem } from '#lib/files/names.ts';

describe('nameProblem', () => {
	it.each([
		['report.pdf', null],
		['  ', 'Enter a name.'],
		['.', 'This name is reserved.'],
		['..', 'This name is reserved.'],
		['a/b', 'Names cannot contain "/".'],
		['a'.repeat(256), 'This name is too long.'],
		['é'.repeat(128), 'This name is too long.']
	])('%s → %s', (name, expected) => expect(nameProblem(name)).toBe(expected));
});
```

**Step 2: Run to verify failure** — `pnpm test:unit` → FAIL.

**Step 3: Implement**

`names.ts`:
```ts
const MAX_BYTES = 255;
const encoder = new TextEncoder();

/** Mirrors the server's NodeName rules closely enough for instant feedback. */
export function nameProblem(raw: string): string | null {
	const name = raw.trim().normalize('NFC');
	if (name === '') return 'Enter a name.';
	if (name === '.' || name === '..') return 'This name is reserved.';
	if (name.includes('/')) return 'Names cannot contain "/".';
	if (encoder.encode(name).length > MAX_BYTES) return 'This name is too long.';
	return null;
}
```

`Breadcrumbs.svelte`:
```svelte
<script lang="ts">
	import type { PathSegment } from '#lib/files/api.ts';

	let { path, current }: { path: PathSegment[]; current: string | null } = $props();
</script>

<nav aria-label="Breadcrumb" class="breadcrumbs">
	<ol>
		<li>
			{#if current === null}<span aria-current="page">My drive</span>{:else}<a href="/files">My drive</a>{/if}
		</li>
		{#each path as segment (segment.id)}
			<li><a href={`/files/${segment.id}`}>{segment.name}</a></li>
		{/each}
		{#if current !== null}<li><span aria-current="page">{current}</span></li>{/if}
	</ol>
</nav>

<style>
	ol { display: flex; flex-wrap: wrap; gap: 0.25rem; list-style: none; padding: 0; margin: 0; }
	li + li::before { content: '›'; margin-right: 0.25rem; color: var(--muted); }
</style>
```

`FileTable.svelte`:
```svelte
<script lang="ts">
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, formatDate } from '#lib/files/format.ts';

	let {
		items,
		onOpen,
		onRename,
		onMove,
		onDelete
	}: {
		items: FileNode[];
		onOpen: (node: FileNode) => void;
		onRename: (node: FileNode) => void;
		onMove: (node: FileNode) => void;
		onDelete: (node: FileNode) => void;
	} = $props();
</script>

<table class="files-table">
	<thead>
		<tr><th>Name</th><th>Size</th><th>Modified</th><th><span class="visually-hidden">Actions</span></th></tr>
	</thead>
	<tbody>
		{#each items as node (node.id)}
			<tr data-node-id={node.id}>
				<td>
					<button class="link name" onclick={() => onOpen(node)}>
						<span aria-hidden="true">{node.kind === 'folder' ? '📁' : '📄'}</span>
						{node.name}
					</button>
				</td>
				<td>{node.kind === 'folder' ? '—' : formatBytes(node.size)}</td>
				<td>{formatDate(node.modified_at)}</td>
				<td class="row-actions">
					{#if node.kind === 'file'}<a href={contentUrl(node.id, true)} download>Download</a>{/if}
					{#if !node.is_root}
						<button class="link" onclick={() => onRename(node)}>Rename</button>
						<button class="link" onclick={() => onMove(node)}>Move</button>
						<button class="link danger" onclick={() => onDelete(node)}>Delete</button>
					{/if}
				</td>
			</tr>
		{:else}
			<tr><td colspan="4" class="muted">This folder is empty. Drop files here or use “Upload files”.</td></tr>
		{/each}
	</tbody>
</table>
```

`NameDialog.svelte` — used for "New folder" and "Rename"; native `<dialog>` for focus handling:
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { nameProblem } from '#lib/files/names.ts';

	let {
		title,
		initial = '',
		submitLabel,
		onSubmit,
		onClose
	}: {
		title: string;
		initial?: string;
		submitLabel: string;
		/** Resolves to an error message, or null on success. */
		onSubmit: (name: string) => Promise<string | null>;
		onClose: () => void;
	} = $props();

	let dialog: HTMLDialogElement;
	let name = $state(initial);
	let error: string | null = $state(null);
	let busy = $state(false);

	onMount(() => dialog.showModal());

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = nameProblem(name);
		if (error) return;
		busy = true;
		try {
			error = await onSubmit(name.trim());
			if (!error) onClose();
		} finally {
			busy = false;
		}
	}
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="name-dialog-title">
	<form onsubmit={submit}>
		<h2 id="name-dialog-title">{title}</h2>
		<label>Name <input bind:value={name} required autocomplete="off" /></label>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<div class="dialog-actions">
			<button type="button" class="link" onclick={() => dialog.close()}>Cancel</button>
			<button type="submit" disabled={busy}>{submitLabel}</button>
		</div>
	</form>
</dialog>
```

`MoveDialog.svelte` — a simple folder picker starting at the top level:
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { listFolder, type FileNode } from '#lib/files/api.ts';

	let {
		node,
		onMove,
		onClose
	}: {
		node: FileNode;
		/** Resolves to an error message, or null on success. */
		onMove: (parentId: string) => Promise<string | null>;
		onClose: () => void;
	} = $props();

	let dialog: HTMLDialogElement;
	let trail: FileNode[] = $state([]);
	let folders: FileNode[] = $state([]);
	let error: string | null = $state(null);
	const here = $derived(trail.at(-1) ?? null);

	async function browse(next: FileNode[]) {
		const listing = await listFolder(next.at(-1)?.id ?? null);
		if (!listing.ok) return (error = listing.error);
		trail = next;
		folders = listing.data.filter((item) => item.kind === 'folder' && item.id !== node.id);
		error = null;
	}

	async function moveHere() {
		if (!here) return;
		error = await onMove(here.id);
		if (!error) dialog.close();
	}

	onMount(() => {
		dialog.showModal();
		void browse([]);
	});
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="move-dialog-title">
	<h2 id="move-dialog-title">Move “{node.name}”</h2>
	<p class="muted">{here ? `To: ${trail.map((f) => f.name).join(' › ')}` : 'Choose a folder'}</p>
	{#if trail.length > 0}<button class="link" onclick={() => browse(trail.slice(0, -1))}>Up</button>{/if}
	<ul class="folder-picker">
		{#each folders as folder (folder.id)}
			<li><button class="link" onclick={() => browse([...trail, folder])}>📁 {folder.name}</button></li>
		{:else}
			<li class="muted">No subfolders</li>
		{/each}
	</ul>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<div class="dialog-actions">
		<button type="button" class="link" onclick={() => dialog.close()}>Cancel</button>
		<button onclick={moveHere} disabled={!here || here.id === node.parent_id}>Move here</button>
	</div>
</dialog>
```

`StorageMeter.svelte`:
```svelte
<script lang="ts">
	import { storageUsage, type StorageUsage } from '#lib/files/api.ts';
	import { formatBytes, storageSummary } from '#lib/files/format.ts';

	let { refresh = 0 }: { refresh?: number } = $props();
	let usage: StorageUsage | null = $state(null);

	$effect(() => {
		void refresh; // reload when the parent bumps it
		storageUsage().then((result) => {
			if (result.ok) usage = result.data;
		});
	});
</script>

{#if usage}
	<p class="muted storage" data-testid="storage-usage">
		{storageSummary(usage)}{#if usage.trash_bytes > 0} · {formatBytes(usage.trash_bytes)} in trash{/if}
		{#if usage.quota_bytes != null}
			<progress max={usage.quota_bytes} value={Math.min(usage.used_bytes, usage.quota_bytes)}></progress>
		{/if}
	</p>
{/if}
```

`FolderView.svelte`:
```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import DropZone from '#lib/components/files/DropZone.svelte';
	import FileTable from '#lib/components/files/FileTable.svelte';
	import MoveDialog from '#lib/components/files/MoveDialog.svelte';
	import NameDialog from '#lib/components/files/NameDialog.svelte';
	import Preview from '#lib/components/files/Preview.svelte';
	import StorageMeter from '#lib/components/files/StorageMeter.svelte';
	import UploadPanel from '#lib/components/files/UploadPanel.svelte';
	import {
		createFolder,
		getNode,
		listFolder,
		moveNode,
		renameNode,
		trashNode,
		type FileNode,
		type PathSegment
	} from '#lib/files/api.ts';
	import { enqueue, onUploadFinished } from '#lib/upload/uploads.svelte.ts';

	let { folderId }: { folderId: string | null } = $props();

	type Dialog =
		| { kind: 'new-folder' }
		| { kind: 'rename'; node: FileNode }
		| { kind: 'move'; node: FileNode }
		| { kind: 'preview'; node: FileNode }
		| null;

	let folder: FileNode | null = $state(null);
	let path: PathSegment[] = $state([]);
	let items: FileNode[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);
	let dialog: Dialog = $state(null);
	let storageVersion = $state(0);

	/** Files can only live inside a folder; the top level is fixed. */
	const canWrite = $derived(folderId !== null && folder !== null);

	async function load() {
		loading = true;
		error = null;
		if (folderId !== null) {
			const details = await getNode(folderId);
			if (!details.ok || details.data.node.kind !== 'folder') {
				error = details.ok || details.status === 404 ? 'This folder does not exist or was deleted.' : details.error;
				loading = false;
				return;
			}
			folder = details.data.node;
			path = details.data.path;
		}
		const listing = await listFolder(folderId);
		if (listing.ok) items = listing.data;
		else error = listing.error;
		loading = false;
	}

	function changed() {
		storageVersion += 1;
		return load();
	}

	onMount(() => {
		void load();
		onUploadFinished((parentId) => {
			if (parentId === folderId) void changed();
		});
		return () => onUploadFinished(null);
	});

	function open(node: FileNode) {
		if (node.kind === 'folder') void goto(`/files/${node.id}`);
		else dialog = { kind: 'preview', node };
	}

	async function remove(node: FileNode) {
		const result = await trashNode(node.id);
		if (!result.ok) error = result.error;
		else await changed();
	}

	function upload(files: File[]) {
		if (folderId !== null) enqueue(files, folderId);
	}

	function chosen(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		upload(Array.from(input.files ?? []));
		input.value = '';
	}

	async function outcome(result: { ok: boolean; error?: string }): Promise<string | null> {
		if (!result.ok) return result.error ?? 'Something went wrong.';
		await load();
		return null;
	}
</script>

<svelte:head><title>{folder?.name ?? 'Files'} · Squirrel</title></svelte:head>

<main class="page files">
	<header class="files-header">
		<Breadcrumbs {path} current={folder?.name ?? null} />
		<div class="files-actions">
			{#if canWrite}
				<button onclick={() => (dialog = { kind: 'new-folder' })}>New folder</button>
				<label class="button">
					Upload files
					<input type="file" multiple class="visually-hidden" onchange={chosen} />
				</label>
			{/if}
			<a href="/trash">Trash</a>
		</div>
	</header>

	{#if error}<p class="error" role="alert">{error}</p>{/if}
	{#if loading && items.length === 0}
		<p class="muted">Loading…</p>
	{:else}
		<DropZone enabled={canWrite} onFiles={upload}>
			<FileTable
				{items}
				onOpen={open}
				onRename={(node) => (dialog = { kind: 'rename', node })}
				onMove={(node) => (dialog = { kind: 'move', node })}
				onDelete={remove}
			/>
		</DropZone>
	{/if}
	{#if !canWrite && folderId === null}
		<p class="muted">Open a folder to add files.</p>
	{/if}

	<UploadPanel />
	<StorageMeter refresh={storageVersion} />
</main>

{#if dialog?.kind === 'new-folder' && folderId !== null}
	{@const parent = folderId}
	<NameDialog
		title="New folder"
		submitLabel="Create folder"
		onSubmit={async (name) => outcome(await createFolder(parent, name))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'rename'}
	{@const node = dialog.node}
	<NameDialog
		title={`Rename “${node.name}”`}
		initial={node.name}
		submitLabel="Save"
		onSubmit={async (name) => outcome(await renameNode(node.id, name))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'move'}
	{@const node = dialog.node}
	<MoveDialog
		{node}
		onMove={async (parentId) => outcome(await moveNode(node.id, parentId))}
		onClose={() => (dialog = null)}
	/>
{:else if dialog?.kind === 'preview'}
	<Preview node={dialog.node} onClose={() => (dialog = null)} />
{/if}
```
(`Preview.svelte` arrives in Task 25; until then create it as a stub that renders a "Download" link and a close button, so this task builds.)

Routes:
`src/routes/files/+page.svelte`:
```svelte
<script lang="ts">
	import FolderView from '#lib/components/files/FolderView.svelte';
</script>

<FolderView folderId={null} />
```
`src/routes/files/[id]/+page.svelte` — re-mount on navigation between folders:
```svelte
<script lang="ts">
	import { page } from '$app/state';
	import FolderView from '#lib/components/files/FolderView.svelte';
</script>

{#key page.params.id}
	<FolderView folderId={page.params.id ?? null} />
{/key}
```

`+layout.svelte` — topbar link before "Invitations":
```svelte
			<nav>
				<a href="/files">Files</a>
				{#if session.me.can_invite}<a href="/settings/invitations">Invitations</a>{/if}
```
`+page.svelte` (home) — replace the "arrive in the next phases" paragraph:
```svelte
		<p><a href="/files">Open your files</a></p>
```
`app.css` — add:
```css
.files-header { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 1rem; margin-bottom: 1rem; }
.files-actions { display: flex; gap: 0.75rem; align-items: center; }
label.button { display: inline-block; padding: 0.6rem 1rem; border-radius: 8px; background: var(--accent); color: var(--accent-text); font-weight: 600; cursor: pointer; }
label.button:focus-within { outline: 2px solid var(--accent); outline-offset: 2px; }
.row-actions { display: flex; gap: 0.75rem; justify-content: flex-end; }
button.link.name { color: var(--text); text-align: left; }
.danger { color: var(--danger) !important; }
dialog { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); max-width: min(90vw, 960px); }
dialog::backdrop { background: rgb(0 0 0 / 0.5); }
.dialog-actions { display: flex; justify-content: flex-end; gap: 1rem; margin-top: 1rem; }
.folder-picker { list-style: none; padding: 0; max-height: 50vh; overflow: auto; }
```

**Step 4: Run** — `pnpm check && pnpm test:unit && pnpm build` → PASS. Manual: run the server (Task 15 smoke command) and `pnpm dev`; log in, open Files → Documents, create a folder, upload a few files (watch the network tab: `PUT …/chunks/n`), rename, move, delete.

**Step 5: Commit**
```bash
git add web
git commit -m "feat(web): add the file browser with folders, rename, move and delete" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 25: Previews (PDF, images, video, audio, text) and the PDF CSP check

**Files:**
- Modify: `web/src/lib/components/files/Preview.svelte` (replace the stub)
- Possibly modify: `crates/squirrel-api/src/content/policy.rs` (`PDF_CSP`), depending on Step 2

**Step 1: Implement** — `Preview.svelte`:
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import type { FileNode } from '#lib/files/api.ts';
	import { contentUrl, formatBytes, previewKind } from '#lib/files/format.ts';

	/** Text previews show the start of the file only. */
	const TEXT_PREVIEW_BYTES = 256 * 1024;

	let { node, onClose }: { node: FileNode; onClose: () => void } = $props();

	let dialog: HTMLDialogElement;
	const kind = $derived(previewKind(node.mime));
	const url = $derived(contentUrl(node.id));
	let text: string | null = $state(null);
	let textError: string | null = $state(null);

	onMount(() => {
		dialog.showModal();
		if (kind === 'text') void loadText();
	});

	async function loadText() {
		try {
			const response = await fetch(url, {
				headers: { Range: `bytes=0-${TEXT_PREVIEW_BYTES - 1}` },
				credentials: 'same-origin'
			});
			if (!response.ok) throw new Error(String(response.status));
			text = await response.text();
		} catch {
			textError = 'Could not load the preview.';
		}
	}
</script>

<dialog bind:this={dialog} onclose={onClose} class="preview" aria-labelledby="preview-title">
	<header>
		<h2 id="preview-title">{node.name}</h2>
		<span class="muted">{formatBytes(node.size)}</span>
		<a href={url} target="_blank" rel="noopener">Open in new tab</a>
		<a href={contentUrl(node.id, true)} download>Download</a>
		<button class="link" onclick={() => dialog.close()}>Close</button>
	</header>
	{#if kind === 'image'}
		<img src={url} alt={node.name} />
	{:else if kind === 'pdf'}
		<iframe src={url} title={`PDF preview of ${node.name}`}></iframe>
	{:else if kind === 'video'}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video src={url} controls preload="metadata"></video>
	{:else if kind === 'audio'}
		<audio src={url} controls preload="metadata"></audio>
	{:else if kind === 'text'}
		{#if textError}<p class="error" role="alert">{textError}</p>
		{:else if text === null}<p class="muted">Loading…</p>
		{:else}
			<pre data-testid="text-preview">{text}</pre>
			{#if node.size > TEXT_PREVIEW_BYTES}<p class="muted">Showing the first {formatBytes(TEXT_PREVIEW_BYTES)}.</p>{/if}
		{/if}
	{:else}
		<p class="muted">No preview for this kind of file. Download it to open it.</p>
	{/if}
</dialog>

<style>
	.preview { width: min(90vw, 960px); }
	header { display: flex; flex-wrap: wrap; gap: 1rem; align-items: baseline; margin-bottom: 1rem; }
	h2 { margin: 0; font-size: 1.1rem; overflow-wrap: anywhere; }
	img, video { max-width: 100%; max-height: 75vh; display: block; margin: 0 auto; }
	iframe { width: 100%; height: 75vh; border: 0; background: white; }
	pre { max-height: 70vh; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
```
The text preview uses `fetch` + `textContent` rendering (Svelte escapes it), never an iframe, so even an HTML file mislabelled as text can't execute.

**Step 2: Verify the PDF viewer under the content CSP (manual, required)**

Headless Chromium (Playwright) has no PDF viewer, so this is a manual check. Run the server and the web UI, upload a real multi-page PDF, then in **Chrome/Chromium, Firefox and Safari** (Safari: if available):

| Check | Expected |
|---|---|
| Click the PDF → preview dialog | PDF renders inside the iframe, pages scroll |
| "Open in new tab" | PDF renders in the browser viewer |
| DevTools console on the app page | no CSP violation reports |
| Response headers of `/api/files/{id}/content` | `Content-Security-Policy: default-src 'none'; object-src 'self'; frame-ancestors 'self'`, `X-Frame-Options: SAMEORIGIN`, `Content-Type: application/pdf` |
| An uploaded `.html` file: "Open in new tab" | downloads (attachment), never renders |
| An image: "Open in new tab" | image shows (sandboxed document) |

If the PDF does **not** render in a browser:
1. Chrome blocks plugins in sandboxed documents — confirm `PDF_CSP` contains no `sandbox`.
2. If it still fails, relax `PDF_CSP` to `"frame-ancestors 'self'"` (still `nosniff`, exact `application/pdf`, `X-Frame-Options: SAMEORIGIN`). Update the unit test in `policy.rs` and `pdfs_are_inline_and_frameable_by_the_app`, and note the browser/version in the commit body.
3. If the iframe is refused but a new tab works, keep the iframe and rely on "Open in new tab" (document it in the README).
Record the outcome (browsers and versions tested) in the commit message body.

**Step 3: Run** — `pnpm check && pnpm build`; `cargo test --workspace` if `policy.rs` changed.

**Step 4: Commit**
```bash
git add web crates
git commit -m "feat(web): preview PDFs, images, media and text" -m "Checked the PDF viewer in: <browsers and versions>." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 26: Trash page

**Files:**
- Create: `web/src/lib/components/ConfirmButton.svelte`, `web/src/routes/trash/+page.svelte`

**Step 1: Implement**

`ConfirmButton.svelte` — two-step confirmation without `window.confirm`:
```svelte
<script lang="ts">
	let {
		label,
		confirmLabel,
		onConfirm,
		disabled = false
	}: { label: string; confirmLabel: string; onConfirm: () => Promise<void> | void; disabled?: boolean } = $props();
	let asking = $state(false);
	let busy = $state(false);

	async function confirm() {
		busy = true;
		try {
			await onConfirm();
		} finally {
			busy = false;
			asking = false;
		}
	}
</script>

{#if asking}
	<span class="confirm">
		<button class="danger-button" onclick={confirm} disabled={busy}>{confirmLabel}</button>
		<button class="link" onclick={() => (asking = false)}>Cancel</button>
	</span>
{:else}
	<button class="link danger" onclick={() => (asking = true)} {disabled}>{label}</button>
{/if}
```

`src/routes/trash/+page.svelte`:
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import ConfirmButton from '#lib/components/ConfirmButton.svelte';
	import { deleteForever, emptyTrash, listTrash, restoreEntry, type TrashEntry } from '#lib/files/api.ts';
	import { formatBytes, formatDate } from '#lib/files/format.ts';

	let entries: TrashEntry[] = $state([]);
	let loaded = $state(false);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);

	async function load() {
		const result = await listTrash();
		if (result.ok) entries = result.data;
		else error = result.error;
		loaded = true;
	}

	async function act(run: () => Promise<{ ok: boolean; error?: string }>, success: string) {
		error = null;
		notice = null;
		const result = await run();
		if (!result.ok) error = result.error ?? 'Something went wrong.';
		else notice = success;
		await load();
	}

	onMount(load);
</script>

<svelte:head><title>Trash · Squirrel</title></svelte:head>

<main class="page">
	<header class="files-header">
		<h1>Trash</h1>
		<div class="files-actions">
			<a href="/files">Back to files</a>
			<ConfirmButton
				label="Empty trash"
				confirmLabel="Delete everything forever"
				disabled={entries.length === 0}
				onConfirm={() => act(emptyTrash, 'Trash emptied.')}
			/>
		</div>
	</header>
	<p class="muted">Items are deleted for good 30 days after they were moved here.</p>
	<div aria-live="polite">{#if notice}<p>{notice}</p>{/if}</div>
	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<table>
		<thead><tr><th>Name</th><th>Size</th><th>Deleted</th><th><span class="visually-hidden">Actions</span></th></tr></thead>
		<tbody>
			{#each entries as entry (entry.node.id)}
				<tr>
					<td>{entry.node.kind === 'folder' ? '📁' : '📄'} {entry.node.name}</td>
					<td>{entry.node.kind === 'folder' ? '—' : formatBytes(entry.node.size)}</td>
					<td>{formatDate(entry.deleted_at)}</td>
					<td class="row-actions">
						<button class="link" onclick={() => act(() => restoreEntry(entry.node.id), `Restored “${entry.node.name}”.`)}
							>Restore</button
						>
						<ConfirmButton
							label="Delete forever"
							confirmLabel="Confirm"
							onConfirm={() => act(() => deleteForever(entry.node.id), 'Deleted forever.')}
						/>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="muted">{loaded ? 'The trash is empty.' : 'Loading…'}</td></tr>
			{/each}
		</tbody>
	</table>
</main>
```
`app.css`: `.danger-button { background: var(--danger); color: var(--accent-text); }`.

**Step 2: Run** — `pnpm check && pnpm build` → PASS. Manual: delete a folder with files, open Trash, restore it, delete forever, empty.

**Step 3: Commit**
```bash
git add web
git commit -m "feat(web): add the trash page" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Milestone F — E2E, CI & docs

### Task 27: E2E infrastructure — data dir, project ordering, admin helper

**Files:**
- Modify: `scripts/e2e-server.sh`, `web/playwright.config.ts`
- Create: `web/e2e/helpers/admin.ts`

**Step 1: Data directory for the E2E server** — `scripts/e2e-server.sh`, before `(cd web && pnpm build)`:
```bash
# A fresh data directory per run. Only a directory named e2e-data is ever wiped.
DATA_DIR="${SQUIRREL_E2E_DATA_DIR:-${PWD}/target/e2e-data}"
case "${DATA_DIR}" in
	*/e2e-data) ;;
	*)
		echo "e2e-server: refusing to wipe '${DATA_DIR}'; the directory must be named e2e-data" >&2
		exit 1
		;;
esac
rm -rf "${DATA_DIR}"
mkdir -p "${DATA_DIR}"
```
and with the other exports:
```bash
export SQUIRREL_DATA_DIR="${DATA_DIR}"
```
Update the header comment (data dir, `SQUIRREL_E2E_DATA_DIR`).

**Step 2: Run the file tests after onboarding** — Playwright orders spec files alphabetically, and `files.spec.ts` needs the accounts `onboarding.spec.ts` creates. `web/playwright.config.ts`:
```ts
	projects: [
		{ name: 'onboarding', testMatch: 'onboarding.spec.ts' },
		{ name: 'files', testMatch: 'files.spec.ts', dependencies: ['onboarding'] }
	],
```

**Step 3: Admin helper** — `web/e2e/helpers/admin.ts` (runs the binary that `scripts/e2e-server.sh` built):
```ts
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Keep in sync with scripts/e2e-server.sh.
const DATABASE_URL =
	process.env.SQUIRREL_E2E_DATABASE_URL ?? 'postgres://squirrel:squirrel@localhost:55432/squirrel_e2e';
const TARGET_DIR = process.env.CARGO_TARGET_DIR ?? fileURLToPath(new URL('../../../target', import.meta.url));

/** Sets a user's quota with `squirrel admin set-quota` (bytes, or 'unlimited'). */
export function setQuota(username: string, quota: number | 'unlimited'): void {
	execFileSync(`${TARGET_DIR}/debug/squirrel`, ['admin', 'set-quota', username, String(quota)], {
		env: { ...process.env, SQUIRREL_DATABASE_URL: DATABASE_URL },
		stdio: 'pipe'
	});
}
```

**Step 4: Verify nothing regressed** — `cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && SQUIRREL_E2E_PORT=8199 pnpm test:e2e` → the onboarding tests pass (3 passed); `ls target/e2e-data` shows `blobs tmp`.

**Step 5: Commit**
```bash
git add scripts web/playwright.config.ts web/e2e/helpers
git commit -m "test(web): give the e2e server a data dir and order spec projects" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 28: File E2E scenarios

**Files:**
- Create: `web/e2e/files.spec.ts`

**Step 1: Write the test** — `web/e2e/files.spec.ts`:
```ts
import { expect, test, type Browser, type Page } from '@playwright/test';
import { setQuota } from './helpers/admin.ts';

const PASSWORD = 'correct horse battery'; // accounts created by onboarding.spec.ts
const MIB = 1024 * 1024;

// One instance shared by the serial tests below.
test.describe.configure({ mode: 'serial' });

let aliceFileId = '';

function patterned(size: number): Buffer {
	const buffer = Buffer.alloc(size);
	for (let i = 0; i < size; i += 1) buffer[i] = (i * 31) % 251;
	return buffer;
}

async function login(browser: Browser, username: string): Promise<Page> {
	const page = await (await browser.newContext()).newPage();
	await page.goto('/login');
	await page.getByLabel('Username').fill(username);
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Log in' }).click();
	await expect(page.getByRole('heading', { name: /^Hello, / })).toBeVisible();
	return page;
}

async function openTopFolder(page: Page, name: string) {
	await page.getByRole('link', { name: 'Files', exact: true }).click();
	await page.getByRole('button', { name, exact: true }).click();
	await expect(page.getByRole('navigation', { name: 'Breadcrumb' })).toContainText(name);
}

function row(page: Page, name: string) {
	return page.locator('tr', { has: page.getByRole('button', { name, exact: true }) });
}

async function upload(page: Page, name: string, buffer: Buffer) {
	await page.getByLabel('Upload files').setInputFiles({ name, mimeType: 'application/octet-stream', buffer });
}

function uploadItem(page: Page, name: string) {
	return page.getByTestId('upload-item').filter({ hasText: name }).last();
}

test('uploads small and multi-chunk files, dedups, downloads and previews', async ({ browser }) => {
	test.setTimeout(120_000);
	const page = await login(browser, 'alice');
	const chunkPuts: string[] = [];
	page.on('request', (request) => {
		if (request.method() === 'PUT' && request.url().includes('/chunks/')) chunkPuts.push(request.url());
	});

	await openTopFolder(page, 'Documents');
	await upload(page, 'hello.txt', Buffer.from('hello squirrel'));
	await expect(uploadItem(page, 'hello.txt')).toContainText('Done');
	await expect(row(page, 'hello.txt')).toBeVisible();

	const big = patterned(9 * MIB + 123); // two 8 MiB chunks
	await upload(page, 'big.bin', big);
	await expect(uploadItem(page, 'big.bin')).toContainText('Done', { timeout: 60_000 });
	expect(chunkPuts.length).toBeGreaterThanOrEqual(3); // 1 for hello.txt + 2 for big.bin

	const href = await row(page, 'big.bin').getByRole('link', { name: 'Download' }).getAttribute('href');
	const downloaded = await page.request.get(href!);
	expect(downloaded.status()).toBe(200);
	expect(Buffer.compare(await downloaded.body(), big)).toBe(0);

	// Same content again, elsewhere: linked without sending a single chunk.
	const putsBefore = chunkPuts.length;
	await openTopFolder(page, 'Files');
	await upload(page, 'big copy.bin', big);
	await expect(uploadItem(page, 'big copy.bin')).toContainText('Already on your drive', { timeout: 30_000 });
	expect(chunkPuts.length).toBe(putsBefore);
	await expect(row(page, 'big copy.bin')).toBeVisible();

	// Text preview.
	await openTopFolder(page, 'Documents');
	await row(page, 'hello.txt').getByRole('button', { name: 'hello.txt', exact: true }).click();
	await expect(page.getByTestId('text-preview')).toHaveText('hello squirrel');
	await page.getByRole('button', { name: 'Close' }).click();

	// PDF preview: the iframe points at the inline content URL (headless Chromium has no PDF viewer).
	await upload(page, 'doc.pdf', Buffer.from('%PDF-1.4\n1 0 obj <<>> endobj\ntrailer <<>>\n%%EOF\n'));
	await expect(uploadItem(page, 'doc.pdf')).toContainText('Done');
	await row(page, 'doc.pdf').getByRole('button', { name: 'doc.pdf', exact: true }).click();
	const frame = page.getByTitle('PDF preview of doc.pdf');
	await expect(frame).toBeVisible();
	const pdf = await page.request.get((await frame.getAttribute('src'))!);
	expect(pdf.headers()['content-type']).toBe('application/pdf');
	expect(pdf.headers()['content-disposition']).toMatch(/^inline;/);
	await page.getByRole('button', { name: 'Close' }).click();

	aliceFileId = (await row(page, 'hello.txt').getAttribute('data-node-id'))!;
	expect(aliceFileId).not.toBe('');
});

test('renames, moves, deletes and restores', async ({ browser }) => {
	const page = await login(browser, 'alice');
	await openTopFolder(page, 'Documents');

	await row(page, 'hello.txt').getByRole('button', { name: 'Rename' }).click();
	await page.getByLabel('Name').fill('notes.txt');
	await page.getByRole('button', { name: 'Save' }).click();
	await expect(row(page, 'notes.txt')).toBeVisible();

	await page.getByRole('button', { name: 'New folder' }).click();
	await page.getByLabel('Name').fill('Archive');
	await page.getByRole('button', { name: 'Create folder' }).click();
	await expect(row(page, 'Archive')).toBeVisible();

	await row(page, 'notes.txt').getByRole('button', { name: 'Move' }).click();
	const picker = page.getByRole('dialog');
	await picker.getByRole('button', { name: /Documents/ }).click();
	await picker.getByRole('button', { name: /Archive/ }).click();
	await picker.getByRole('button', { name: 'Move here' }).click();
	await expect(row(page, 'notes.txt')).toHaveCount(0);

	await page.getByRole('button', { name: 'Archive', exact: true }).click();
	await expect(row(page, 'notes.txt')).toBeVisible();
	await row(page, 'notes.txt').getByRole('button', { name: 'Delete' }).click();
	await expect(row(page, 'notes.txt')).toHaveCount(0);

	await page.getByRole('link', { name: 'Trash' }).click();
	await expect(page.getByRole('heading', { name: 'Trash' })).toBeVisible();
	await page.getByRole('row').filter({ hasText: 'notes.txt' }).getByRole('button', { name: 'Restore' }).click();
	await expect(page.getByText('Restored “notes.txt”.')).toBeVisible();

	await openTopFolder(page, 'Documents');
	await page.getByRole('button', { name: 'Archive', exact: true }).click();
	await expect(row(page, 'notes.txt')).toBeVisible();
});

test('another user can neither see nor fetch alice’s files', async ({ browser }) => {
	const bob = await login(browser, 'bob');
	expect((await bob.request.get(`/api/files/${aliceFileId}/content`)).status()).toBe(404);
	expect((await bob.request.get(`/api/nodes/${aliceFileId}`)).status()).toBe(404);
	await openTopFolder(bob, 'Documents');
	await expect(row(bob, 'notes.txt')).toHaveCount(0);
});

test('an upload over the quota is refused with a clear message', async ({ browser }) => {
	setQuota('bob', MIB);
	const bob = await login(browser, 'bob');
	await openTopFolder(bob, 'Files');
	await upload(bob, 'too-big.bin', patterned(2 * MIB));
	await expect(uploadItem(bob, 'too-big.bin')).toContainText('exceed your storage quota');
	await expect(bob.getByTestId('storage-usage')).toContainText('of 1.0 MB used');
});
```

**Step 2: Run** — `cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && SQUIRREL_E2E_PORT=8199 pnpm test:e2e` → 7 passed (3 onboarding + 4 files).
If a selector fails, prefer fixing the component's accessible name over loosening the selector.

**Step 3: Commit**
```bash
git add web/e2e
git commit -m "test(web): cover uploads, dedup, previews, trash, quota and isolation end to end" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 29: CI and README

**Files:**
- Modify: `.github/workflows/ci.yml`, `README.md`

**Step 1: CI** — in the `e2e` job's `env`:
```yaml
      # Fresh data directory for scripts/e2e-server.sh (it must be named e2e-data).
      SQUIRREL_E2E_DATA_DIR: ${{ runner.temp }}/e2e-data
```
Nothing else changes: Rust tests create their own temporary data directories, and the migration guard already protects `20261113000002_files.sql` once merged.

**Step 2: README** — update these parts:

Development command:
```
    SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
    SQUIRREL_DATA_DIR="$PWD/target/dev-data" \
    SQUIRREL_COOKIE_SECURE=false cargo run -p squirrel -- serve     # http://localhost:8080
```

Configuration table, new row (after `SQUIRREL_DATABASE_URL`):
```
| `SQUIRREL_DATA_DIR` | (required) | Where files are stored (`blobs/`, `tmp/`). Created if missing; `tmp/` and `blobs/` must be on the same filesystem |
```

New section after "Configuration":
```markdown
### Data directory

    <SQUIRREL_DATA_DIR>/
      blobs/ab/cd/<sha256>   file contents, stored once per distinct content (read-only)
      tmp/uploads/<id>       uploads in progress (expire after 24 h without activity)

The database holds names, folders and who owns what; the blobs hold the bytes. Back up
both together (Phase 7 documents `pg_dump` + blob rsync). Files deleted from the trash
(or after 30 days in it) are removed from `blobs/` a day later, once nothing references them.
Uploads are refused while less than 1 GiB would remain free on that disk.

### Administration

    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice 10737418240   # 10 GB
    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice unlimited

Quotas count live files (not the trash). Users see their usage on the Files page.
```

Tests section: mention that `pnpm test:e2e` also recreates `target/e2e-data` (override with `SQUIRREL_E2E_DATA_DIR`, which must end in `e2e-data`), and that PDF rendering in browsers is checked manually (Task 25).

**Step 3: Verify** — full gate:
```bash
cargo fmt --all --check && rtk proxy cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd web && source ~/.nvm/nvm.sh && nvm use 22 >/dev/null && pnpm gen:api && git diff --exit-code src/lib/api/schema.d.ts \
  && pnpm check && pnpm test:unit && pnpm build && SQUIRREL_E2E_PORT=8199 pnpm test:e2e
```
→ all green.

**Step 4: Commit**
```bash
git add .github README.md
git commit -m "docs: document the data directory, quotas and e2e data dir" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Security considerations

| Threat | Mitigation in this plan |
|---|---|
| **Path traversal via hashes** | `ContentHash::parse` accepts exactly 64 `[0-9a-f]`; the DB has the same `CHECK`. Blob paths are built only from a parsed hash (`ab/cd/<hash>`). Upload files are named by server-generated UUIDs. User-supplied names never touch the filesystem. |
| **Path tricks in names** | `NodeName` rejects `/`, NUL/control characters, `.`/`..`, bidi overrides; NFC, ≤ 255 bytes. Names are only ever DB values. |
| **Cross-user access** | Every query filters on `owner_id`; foreign ids behave exactly like missing ones (404), including uploads, trash entries and content. Move/restore targets must be the caller's live folders. API tests and the E2E check this. |
| **Dedup side channel** | The probe consults only blobs referenced by the caller's own nodes. Content owned only by others gets a normal `201` and must be uploaded in full; the shared blob is reused silently at completion. Timing differs only by work the caller did themselves. |
| **XSS through downloads** | MIME is sniffed server-side (`infer` + extension fallback); client types are ignored. Inline only for an allowlist; HTML/SVG/XML/JS are served as `application/octet-stream` + attachment. All content responses: `nosniff`, `CORP: same-origin`, `X-Frame-Options: SAMEORIGIN`, CSP `sandbox; default-src 'none'…`. Text previews render through `fetch` + escaped text, never an iframe. |
| **PDF same-origin rendering** | PDFs are inline without `sandbox` (viewers refuse sandboxed documents) but with `default-src 'none'`; browser PDF viewers (PDFium, pdf.js) run PDF scripts in their own sandbox without access to our origin. A separate content origin would be stronger — out of scope for a home LAN/Tailscale deployment; noted for Phase 7. |
| **Header injection** | `Content-Disposition` is built from a printable-ASCII fallback plus RFC 5987 percent-encoding; header values go through `HeaderValue::try_from`. |
| **Quota race** | Quota checks run in the same transaction as the write, after `SELECT … FOR UPDATE` on the user row; open sessions are reserved at creation; completion re-checks. Two parallel completions cannot both pass. |
| **Disk exhaustion** | Creation refuses uploads leaving < `min_free_bytes`; `ENOSPC`/`EDQUOT` map to 507 `storage_full`; chunk bodies are capped at chunk size + 64 KiB; expired sessions and orphan files are swept hourly. |
| **Slow-loris uploads** | 5-minute timeout per chunk request; chunk bodies are bounded; JSON routes keep their 30 s timeout. |
| **Concurrent completes / chunk after complete** | `status = 'completing'` is claimed atomically; other completes get 409, chunk writes and aborts are refused. |
| **GC deleting live data** | Advisory locks per blob (completion: transaction lock from before the rename to commit; GC: session lock across delete-row → unlink), plus a 24 h grace period. Ref counts come from a trigger, so cascades are exact. |
| **Corrupt uploads** | Full SHA-256 verification at completion before the blob is adopted; mismatch → 422 and discard. Chunks are `fdatasync`ed before being recorded; blobs are fsynced before the atomic rename and the directory after. |
| **CSRF** | Every state-changing endpoint, including raw chunk `PUT`s, goes through `CurrentUser` (synchronizer token). |
| **Zip bombs / decompression** | Not applicable: nothing is decompressed or parsed beyond magic-number sniffing of the first 8 KiB. |
| **Wasm in CSP** | `'wasm-unsafe-eval'` allows WebAssembly compilation only, not `eval`; scripts remain `'self'` + hashes. |

## Not in Phase 2

- EXIF/XMP extraction, thumbnails, blurhash, geocoding, the job queue (`squirrel-jobs`, `process_media`) — **Phase 3**. `POST …/complete` does not enqueue anything yet; `assets` rows do not exist.
- PDF first-page thumbnails and text extraction / full-text document search — **Phase 3** (with the media pipeline). Phase 2 only previews PDFs with the browser's viewer.
- Video probing, HLS transcoding and the player — **Phase 4** (Phase 2 plays MP4/WebM directly via Range requests when the browser can).
- CLIP tags, faces, search — **Phase 5**.
- Device sync: `POST /api/sync/check`, `device_assets`, `taken_at`/`device_asset_id` upload fields — **Phase 6**. (Unknown JSON fields are ignored, so the KMP app can send them early.)
- Docker Compose, deployment docs, backups, the weekly blob scrub and orphan-blob reclamation — **Phase 7**.
- Sharing and public links (out of scope for v1), WebDAV, OCR.
- Quota administration UI and per-user default quotas (CLI only for now); folder upload by drag-and-drop; multi-select and bulk actions; virtualized listings for very large folders; overwrite-in-place ("new version") uploads.

## Done when

- `cargo test --workspace` green: core types, storage (chunk writes, atomic adopt, idempotency, ENOSPC mapping), DB (schema constraints, ref-count trigger, tree, trash, uploads, quota) and API (upload protocol incl. dedup isolation, quota at creation and completion, hash mismatch, concurrent completion, tree/trash endpoints, Range/ETag/disposition/CSP, cross-user 404s, maintenance).
- `pnpm test:unit` and `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` green (7 E2E tests).
- `openapi.json` and `schema.d.ts` regenerated and committed; CI green.
- Manual: PDF preview verified in Chrome and Firefox (Task 25), a > 1 GB upload resumes after killing the browser tab and re-uploading the same file.
- Then add an **"As built"** section at the top of this plan, as Phase 1 did.
