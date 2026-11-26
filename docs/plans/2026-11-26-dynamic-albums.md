# Dynamic albums

Date: 2026-11-26. Status: built. Albums whose photos come from rules (people, tags, …) instead of
being picked by hand.

## Decisions (validated with the user)

- **Groups combined with AND**: people (any of / all of), tags (any of / all of), place (country →
  region → city), camera (make → model), kind, capture date range (days, both inclusive). An empty
  group or absent field does not constrain; at least one condition is required. No nested boolean
  tree.
- **Rules only**: contents are always computed live from the owner's own live, visible photos. No
  manual add or remove (409). The owner renames, picks a cover (a matching photo), edits the rules,
  deletes, and shares **read-only** (a write share is 409: nobody contributes).
- Clients: API, web, phone.

## Model and API

- `album_rules` (1:1 with `albums`, `ON DELETE CASCADE`): `people UUID[]`, `people_match`,
  `tags TEXT[]`, `tags_match`, `country`, `region` (`''` = no region, as photo filters), `city`,
  `camera_make`, `camera_model`, `kind`, `taken_from`/`taken_to DATE`. At most 20 people and tags.
- People are an array without a foreign key: a deleted person matches nothing (the album empties
  rather than widens); `people::merge` rewrites merged ids to the kept person.
- One SQL predicate, `squirrel_db::album_rules::RULE_MATCHES`, defines "matches" everywhere: album
  list (count, cover), items, "albums of this photo", read access through a shared album, and the
  preview (rules bound as a `WITH r AS (…)` row).
- `AlbumDto` gains `dynamic` and `rules` (owner only: rules name the owner's people).
- `POST /api/albums { name, rules }` creates one; `PATCH /api/albums/{id} { rules }` replaces them
  (422 on a manual album); `POST /api/albums/preview` (rules) → `{ count, newest ≤ 12 }`, under the
  search statement timeout (503 when too slow).

## Risks

- Cost: counts and covers scan the owner's photos per dynamic album on every album list. Fine for a
  home library; an index or cached counts would be the next step if it is not.
- Comment notifications fan out to albums through `album_items` only: a comment on a photo does not
  notify people a dynamic album is shared with.

## As built

- Migration `20261126000001_dynamic_albums.sql`; `squirrel_db::album_rules`; `DbError::DynamicAlbum`
  (409); API `album_rules_dto.rs`, `routes/albums.rs` (`preview_album`).
- Tests: DB `tests/dynamic_albums.rs`, API `tests/dynamic_albums.rs`, unit tests in
  `album_rules.rs` and `album_rules_dto.rs`.
