# Duplicates: group near-identical photos, keep one, hide the rest

**As built (2026-11-22).** Shipped as planned (`ea3f681` pHash and schema · `33ba54d` grouping job · `0ef1d51` hidden filter · `cb11a2b` API · `4446d82` web · `b945f9e` E2E). pHash is taken from the in-memory thumbnail (the backfill hashes the stored one: same input, ≤ 2 bits apart). Grouping waits for the capture time (metadata step) and runs per owner after `SQUIRREL_DUPLICATES_DELAY_SECS` (60 s). Deviations: a group whose last remaining photo is hidden is kept (otherwise that photo could never be restored); trashing a photo takes it out of its group and unhides it; the sync check still counts hidden photos as backed up (no re-upload). The web page shows one large preview with a strip of the group's photos (user request), previous/next group and Undo. Review fixes (`3ef579a`, `9174e2a`, `93b2968`, migrations `…000002`/`…000003`): keep, restore, grouping and trash lock in one order; a hidden photo never stays without a group; a reopened group names no kept photo; a visible photo whose capture time changes is grouped again. Albums can still take a hidden photo by id (it stays out of their listing). Threshold 6 bits still to calibrate on real photos (#9).

**Goal:** group each user's near-identical photos automatically. In a group the user can pick one to keep; the others are hidden everywhere (not deleted, restorable). Doing nothing leaves the group as it is.

**Approved by the user (2026-11-22):** near-identical shots (visual hash) taken **at most 1 day apart**; hidden photos disappear **everywhere** (timeline, search, albums, people, places, Explore, phone); review on the **web first** (phone only stops showing hidden photos). Byte-identical files are already stored once (sha256), so this targets different files that look the same.

## Decisions

| Topic | Decision | Why |
|---|---|---|
| Visual hash | 64-bit perceptual hash (DCT pHash on a 32×32 grey copy of the oriented image), one per blob (`media.phash`, content-derived like the embedding). Computed by the media-exec child from the decoded image in the thumbnails step; existing images get it from a backfill `visual_hash:<sha256>` job that hashes the stored thumbnail in the child. | Survives resizing, re-compression, light edits; cheap; reuses the sandbox and the decoded image. |
| Similar | Hamming distance ≤ `DUPLICATE_MAX_DISTANCE` (start 6 of 64, provisional, calibrated in #9) **and** `taken_at` within 24 h. | The user's rule; the time window keeps unrelated look-alikes apart. |
| Groups | Per owner, never across users. A group has an anchor (its oldest photo); a photo joins only if within the distance of the anchor (no chaining across a day of bursts). Rebuilt incrementally by a debounced `group_duplicates:<owner>` job (like `cluster_faces`), queued when a photo gets its hash. Groups of one are not kept. | Bounded, explainable groups; per-owner privacy like faces. |
| Decisions | `POST keep {asset}` hides the other members (`assets.hidden_at`, `hidden_reason = 'duplicate'`) and marks the group resolved. `restore` unhides. No action = group stays open. A new photo joining a resolved group reopens it, visible until the user decides; already hidden photos stay hidden. | Never hide without a decision. |
| Hidden everywhere | Every photo query (timeline, months, search, albums, people/faces, places, tags, cameras, Explore counts, `/api/photos/{id}` for listing neighbours, sync listing for the phone) filters `hidden_at IS NULL`. Opening a hidden photo by id still works (the duplicates page shows it). Files page unchanged (it lists files, not photos). | One rule, tested per route. |
| Data safety | Migration adds columns/tables only, nothing dropped or rewritten. Deleting a photo removes it from its group; a group left with one member is deleted; deleting the kept photo leaves the hidden ones hidden (the group shows them under Hidden). Quotas unchanged (hidden files still count). | Reversible by construction. |
| API | `GET /api/duplicates?state=open|resolved` (keyset paged, members with size, resolution, date, camera, name), `POST /api/duplicates/{id}/keep`, `POST /api/duplicates/{id}/restore` (and per photo). Session + CSRF; the phone reads nothing new. 404 for another user's group. | Same auth rules as people corrections. |
| Web | Explore › Duplicates: open groups side by side with their metadata, a suggested keeper (largest resolution, then file size, then oldest), "Keep this", and a Resolved/Hidden tab with Restore. MASTER.md components only. | Web first, as decided. |

## Risks

- **A missed filter shows hidden photos somewhere:** a test per listing route seeds a hidden duplicate and asserts it is absent; another user's hidden state never affects the caller.
- **False groups** (two different moments): the anchor rule and the 24 h window limit them; nothing is hidden without a click; threshold provisional until #9.
- **Cost:** hashing is a few ms per photo; grouping compares only photos within ±24 h of each other per owner (index on `(owner_id, taken_at)` plus the hash), incremental.

## Tasks (each: fmt, clippy `-D warnings`, narrow tests, one commit; full gate at the end)

1. pHash in `squirrel-media` (golden tests: resized/re-encoded copies within the distance, different photos far) and in the media-exec thumbnails result; migration (`media.phash`, `assets.hidden_at/hidden_reason`, `duplicate_groups`, `duplicate_members`); store it.
2. `visual_hash` backfill job and stale rule; `group_duplicates:<owner>` job (anchor rule, 24 h window, reopen on new members), queued after hashing.
3. Hidden filter on every photo route, with the per-route tests.
4. Duplicates API (list, keep, restore), OpenAPI and `schema.d.ts`; phone `checkApiClient`.
5. Web Explore › Duplicates page (vitest for the keeper suggestion; E2E: two copies grouped, keep one, the other leaves the timeline and search, restore brings it back; another user sees nothing).
6. README paragraph and a 5-line As-built here.
