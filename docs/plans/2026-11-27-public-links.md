# Public links

Date: 2026-11-27. Status: built. Lifts "public links" from the v1 out-of-scope list.

## Decisions (validated with the user)

- Anyone with the link can see a **folder (its subtree), a file or an album**, read-only, without
  an account. All three kinds from the start.
- **Optional expiry** (a date and time) and **optional password**, chosen when the link is made.
- Served to the internet through **Tailscale Funnel**, on a **separate listener** that serves only
  the public routes and the web pages they need. The tailnet listener keeps everything else.

## Choices made while planning

- **Several links per item**, each with its own expiry and password. A link is never edited:
  revoke it and make another.
- **Tokens are stored hashed** (SHA-256, like sessions and invitations). The URL is shown once,
  when the link is made; later the owner sees the link's date, expiry and whether it has a
  password, not its URL. Making another link is one click.
- **No "view only" switch**: whatever a browser can display can be saved, so the visitor can
  download originals and zips. Not offering a switch avoids promising what the server can't enforce.
- **The password** follows the account password rules (10 to 256 characters, Argon2id, the
  shared hashing permits). Unlocking sets an HttpOnly cookie scoped to
  `/api/public/<token>`, valid 7 days: `<issued unix time>.<HMAC-SHA256>` keyed by the link's
  password hash, over the link id and the issue time. Nothing is stored; revoking the link ends
  it. Unlock attempts have the login rate limit (per client address).
- **What a visitor sees** is the owner's item and nothing above it: no owner ids, parent ids
  above the link, paths above it, faces, people, places, tags or comments. The owner's display
  name is shown ("Shared by …").
- **Not found is one answer**: an unknown, expired or revoked token, a trashed item, or a disabled
  owner is `404 not_found`, the same as everywhere else.
- **Limits**: downloads and preview renders are counted per link (the global caps still apply).
  `GET /api/public/{token}` (the page's first call) has the invitation check's rate limit.
  The tokens are 256-bit, so guessing them is hopeless.
- **Configuration**: `SQUIRREL_PUBLIC_URL` (e.g. `https://home.tail1234.ts.net:8443`) turns
  the feature on and is the base of the links. `SQUIRREL_PUBLIC_BIND` (e.g. `127.0.0.1:8081`)
  starts the public listener, which Funnel exposes. Without `SQUIRREL_PUBLIC_URL`, links can't be
  made (409). The tailnet listener also answers `/api/public/**`, so links work inside the tailnet.

## Model

- `public_links(id, owner_id, node_id | album_id, token_hash UNIQUE, password_hash NULL,
  expires_at NULL, created_at)`, composite FKs to `(node_id, owner_id)` / `(album_id, owner_id)`
  cascading (deleting the item deletes its links).
- Resolver `squirrel_db::public_links::find_active(token_hash)`: the link if not expired, its
  owner not disabled, and its node live. `node_in_link(link, id)`: a live node that is the
  link's file, or the link's folder or a descendant of it, or a visible photo of the link's
  album (rule match for a dynamic album).

## API

Owner (session + CSRF, or device bearer), tag `sharing`:
- `GET /api/public-links?node_id|album_id` → `{ enabled, links: [PublicLinkDto] }`.
- `POST /api/public-links` `{ node_id | album_id, expires_at?, password? }` → 201
  `{ link, url }`. 409 when public links are not set up; 422 for a past expiry or a bad password.
- `DELETE /api/public-links/{id}` → 204.

Visitor (no credentials; the unlock cookie when there is a password), tag `public`:
- `GET /api/public/{token}` → `{ locked: true }` or `{ locked: false, kind, name, shared_by,
  expires_at, root }` (for a file: the file; for an album: its count).
- `POST /api/public/{token}/unlock` `{ password }` → 204 + cookie; 401 `invalid_credentials`.
- `GET /api/public/{token}/nodes?parent_id&cursor&limit` → the folder, its path from the link
  root, a page of children.
- `GET /api/public/{token}/album/items?cursor&limit` → a timeline page.
- `GET /api/public/{token}/files/{id}/thumbnail` and `/content` → as the private routes.
- `POST /api/public/{token}/archive` `{ node_ids }` → ticket; `GET
  /api/public/{token}/archive/{ticket}` → the zip.
- Everything but the first two is 401 `unauthorized` while locked.

## Web

- Share dialog: a "Public link" section (owner only). It lists the links (date, expiry, password
  yes/no, Revoke) and makes one (expiry and password optional), then shows its URL once with Copy.
  When the server has no public URL it says so.
- `/s/[token]`: a page without the app shell or a session. Password form when locked; a folder
  browser (breadcrumbs from the link root), an album grid, or a single file, with a viewer and
  downloads.

## Risks

- **Escaping the link**: every visitor route resolves the node through `node_in_link`. Tests
  cover a node outside the folder, another user's node, a trashed node, a photo removed from the
  album, an expired or revoked link, and a disabled owner.
- **Exposing the private app**: the public listener answers only `/api/public/**`, `/s/**` and
  the built assets. Tested at the router level.
- **Tokens in logs**: `/api/public/<token>` paths appear in request logs at `RUST_LOG=debug`
  (as invitation tokens do). Documented.

## Tasks

1. Migration + `squirrel-db::public_links` (CRUD, resolver, `node_in_link`), DB tests.
2. API: owner routes, visitor routes, unlock cookie, public router, `serve` flags; OpenAPI;
   API tests.
3. Web: share dialog section, `/s/[token]` page, layout bypass; unit tests, E2E.
4. Deploy guide: Funnel setup; README configuration table.

## Not in this version

Visitor comments or uploads, editing a link, view counts, the phone app UI.

## As built

- Migration `20261127000001_public_links.sql`; `squirrel_db::public_links` (`find_active`,
  `node_in_link`). API: owner routes `routes/public_links.rs`, visitor routes `routes/public/`
  (`mod.rs` link resolution, info and unlock; `browse.rs`; `files.rs`), unlock cookie
  `public_unlock.rs`, DTOs `public_links_dto.rs`. Content, thumbnail and zip serving were split
  out of their private routes (`content::serve`, `thumbnails::serve`, `archive::ticket_for` /
  `archive::stream`) and are keyed by a user or a link id.
- Routers: `squirrel_api::public_router` (public listener: public routes + `spa::serve_public`,
  which serves `index.html` only for `/s/{token}`); `router` also mounts the public routes.
  `serve` flags `--public-url` / `--public-bind` (empty `SQUIRREL_PUBLIC_URL` = off).
- Errors: `link_locked` (401) while a password is needed.
- Web: `PublicLinksSection` in the Share dialog; `/s/[token]` page outside the app shell (the
  root layout loads no session there), with `PublicFolder`, `PublicAlbum`, `PublicViewer`, and
  a client of its own (`lib/public/api.ts`) without the session middleware.
- Deploy: compose publishes `127.0.0.1:8081`; `deploy/README.md` "Public links" (Funnel on 8443).
- Tests: DB `tests/public_links.rs` (7), API `tests/public_links.rs` (10) + route matrix, unit
  tests (cookie, URL, token shape, public pages, CLI), web `publicLinks.test.ts`,
  `public/view.test.ts`, guard; E2E `public-links.spec.ts` (public listener on E2E port + 1).
