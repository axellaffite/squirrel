# Shared text editor

Date: 2026-11-23. Status: built. Builds on sharing and comments.

## Decisions (validated with the user)

- **Live co-editing** of text files in the drive, in the web app: everyone who can see the file
  opens it, edits appear at once for all, with each person's cursor and name.
- Who edits: the owner and people with a **write** share on the file or a folder above it.
  Read shares see the document live but cannot type.
- Which files: text mime types (`text/*`, JSON, XML, YAML, …), valid UTF-8, at most **1 MiB**.
- The **file stays the truth**: edits are saved into the file (a new blob, same node) 2 s after
  the last change, at most every 10 s while typing, and when the last person leaves. Downloads,
  search and phones see the saved text. Saved bytes count against the **owner's** quota.

## Model

- CRDT: Yjs in the browser (CodeMirror 6 + `y-codemirror.next`, `y-websocket` client), `yrs`
  on the server speaking the same y-sync/awareness protocol over an axum WebSocket.
- Migration `collab_docs(node_id PK → nodes ON DELETE CASCADE, epoch uuid, blob_sha256, state
  bytea, updated_at)`: the Yjs state matching the file's current blob. If the file's blob changed
  outside the editor, the state is rebuilt from the file and the **epoch** changes.
- `POST /api/files/{id}/collab` → `{ epoch, can_write, max_bytes }` (422 `not_editable`).
  `GET /api/files/{id}/collab/ws?epoch=…` upgrades. Close codes: 4401 signed out, 4404 no
  access/trashed, 4409 stale epoch (client reopens with a fresh doc), 4413 too large, 4429 busy.
- One in-memory room per open file (single API process), dropped once empty and saved.

## Risks

- Cross-site WebSocket hijacking: cookies ride on WS upgrades; the `Origin` must match `Host`.
- Revocation/trash while connected: each connection re-checks access every 10 s and on save.
  Readers' document updates are dropped by the server.
- Doc merges after a server restart: the stored state and the epoch prevent duplicated text.
- Size: an update pushing the text above 1 MiB is refused (the sender is closed with 4413).

## Tasks

1. Migration + `squirrel-db::collab`, storage `put_bytes`, DB tests.
2. API: open route, WebSocket room/registry/persistence, access checks; API tests.
3. Web: editor page, "Edit" action on text files, presence; E2E (two browsers co-edit).

## As built

- Migration `20261123000002_collab.sql`; `squirrel_db::collab`; rooms in `squirrel-api/src/collab/` (room, registry, saver, socket); routes in `routes/collab.rs`. Close code 1012 on shutdown (rooms saved first; clients reconnect).
- `yrs` pinned to 0.26: 0.27+ needs Rust 1.95 (`if let` guards), the toolchain is 1.94.
- The greeting is sent one message per frame: the y-websocket client decodes one message per frame (found by E2E).
- Web: `/edit/[id]` (CodeMirror 6 + y-codemirror), "Edit"/"Open in editor" on text rows and in the preview; remote cursor colours are restricted to tokens on the client. Awareness names are client-asserted.
- Review fixes: one save at a time per room; a room is retired (not joinable) before it leaves the registry, and never while unsaved; failed saves retry; a full quota closes with 4507; the epoch is checked before loading; a room on an older epoch is closed; `collab::start` writes only if the file still has that blob; access changes send 4409 (reopen with new rights); a peer announces only its own awareness clients (4 max); updates with missing dependencies are refused.
- "New text file" (folder header): `POST /api/files {parent_id, name}` creates an empty file whose extension must be a text type (422 `not_editable` otherwise), then the editor opens it.
- Known limits: names in awareness are client-asserted; revocation takes effect within 10 s.
- Tests: DB `tests/collab.rs`, API `tests/collab.rs` (11 scenarios over a real socket), E2E `e2e/text-editor.spec.ts`.
