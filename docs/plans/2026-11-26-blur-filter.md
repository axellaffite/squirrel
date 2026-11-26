# Blur filter

Date: 2026-11-26. Status: built. Some of one's own photos stay blurred when the drive is open in
front of others, until one chooses to see them.

## Decisions

- **One filter per user, stored on the server**, written exactly as a dynamic album's rules
  (people, tags, place, camera, kind, dates, search text), so the same editor and the same SQL
  predicate (`album_rules::RULE_MATCHES`) serve both. It can be turned off without being forgotten.
- **Applies to the user's own photos only**: rules name the owner's people, and someone else's
  photo in a shared album is theirs to filter.
- **The server lists, clients blur**: `GET /api/photos/blurred` returns the matching node ids;
  every client checks a thumbnail's id before showing it. Nothing else in the API changes, so
  timeline, albums, covers, tag and place cards, search, the viewer and Files are all covered by
  one check per component. A blurred photo is never fetched: only its blurhash shows.
- **Revealing is per visit**: "Show" in the viewer reveals one photo, "Show blurred" on the
  timeline reveals all; both are forgotten on reload. The web remembers (localStorage) whether
  the filter blurred anything, and blurs every photo until the list arrives, so nothing flashes.

## Model and API

- `blur_filters` (1:1 with `users`, `ON DELETE CASCADE`): `enabled` plus `album_rules`' rule and
  search columns. People merges rewrite it as they rewrite album rules.
- `GET /api/me/blur-filter` → `{ enabled, rules | null }`; `PUT` the same (`rules: null` forgets
  it; rules checked as album rules, 422 / 503 alike). `GET /api/photos/blurred` → `{ node_ids }`,
  under the search statement timeout (503 `busy`). Session or device bearer.

## Not blurred

- Face crops (people avatars, face pickers) and the duplicates review, which needs the photos.

## As built

- Migration `20261126000003_blur_filters.sql`; `squirrel_db::blur_filter`; API
  `routes/blur_filter.rs`. Tests: DB and API `tests/blur_filter.rs`, web
  `photos/blurState.test.ts`, E2E `blur-filter.spec.ts`.
- Web: `photos/blurred.svelte.ts` (store), `PhotoTile`, `AlbumCover`, `LightboxPhoto`,
  `Lightbox` (no prefetch), Files `NodeVisual` and `Preview`, `/settings/blur-filter`.
