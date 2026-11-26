# Squirrel design system

Single source of truth for Squirrel's visual layer. Every color, size, radius, shadow and duration
in `web/` comes from `web/src/lib/styles/tokens.css`, which implements this file. No magic numbers.

## Theses (validated 2026-11-26, finish pass over 2026-11-14)

**Visual thesis.** Squirrel stays gallery grey with one Orchid accent (#9a3fd0 light, #d7a2ff dark),
set in Geist (kept: it already ships self-hosted, with the tabular figures every size and date
needs), but finished. The sidebar is a blurred photo wash the owner picks in Settings › Appearance:
four built-in washes (Forest by default, Beach, Dusk, Graphite) or one of their own photos, decoded
from its blurhash, remembered per device like the theme. It sits under a dark scrim whose strength
is computed so the lightest point of the image still gives the labels 4.5:1, with white ink, small
section labels and a translucent white active row. To its right, one near-flat light panel holds
white surfaces drawn with 1 px hairlines and a near-flat shadow instead of soft diffuse depth.
Radii step down to 8 px for buttons, fields, icon buttons and icon tiles, 12 px for cards, 18 px for
the panel and sidebar; pills remain only for chips, badges, segment tracks and avatars. Nested
corners are concentric everywhere: an inner radius is the outer radius minus the padding between
them. File, shared and trash lists are real tables: header band, 52 px rows, hairline separators,
tabular size and date columns, kind chips. Pages have three tiers: a title with a one-line
subtitle and actions on the right, small section labels, then content. Spacing stays on the 8 px
base, a notch denser in lists. The primary button has no glow. Dark mode follows the same rules on
graphite. The Compose app takes the same tokens; its bottom bar is a floating white full pill
(64 px, hairline, soft shadow, 6 px padding) with icon-only tabs (accessible names kept), every tab
state marked only by a top notch, and an Orchid circle in the centre that opens a small menu of the
app's existing add actions; sheets open over blurred content.

**Interaction thesis.** Calm and exact, timings unchanged: 160 ms for small controls, 240 ms for
rows, cards and menus, 360 ms for sheets, dialogs and the sidebar photo cross-fade, all on
cubic-bezier(.22, 1, .36, 1). Hover changes a background or a border colour only: rows take a tint,
cards darken their hairline. Presses sink 1 px. A view's rows arrive once per load, rising 8 px,
30 ms apart, capped at 8, with no scroll-triggered reveals. Never: bounce or overshoot, hover
scaling, parallax or slow drift on the sidebar photo, animated blur radius, looping animation other
than real progress, or animated width, height or position. With reduced motion, everything changes
instantly.

**Allowed patterns.** The blurred photo wash (a built-in preset or the owner's photo), on the web
sidebar and its picker tiles only, always under its computed scrim. Content blurred behind a dialog
or sheet (a modal backdrop only, never a translucent panel). 1 px hairlines on cards, tables and the
panel; one near-flat shadow per surface, menus and dialogs keep a deeper pop shadow. A grey gradient
on the ground only. Pills for chips, badges, segment tracks and avatars. Tabular figures for sizes,
counts and dates. On the phone bar: icon-only tabs, a top-edge notch for every tab state, and one
Orchid circle for the add menu. The unread dot drawn as a radial gradient. At most one graphite card
per screen for a featured figure (the phone Backup status card, kept from 2026-11-14).

**Brainstorm record.** 2026-11-14: soft neumorphism was rejected once built and replaced by gallery
grey, graphite and Orchid. 2026-11-26 finish pass (partial mode: Geist, Orchid, layout, nav labels and
token names kept): references were a storage dashboard, a file explorer and an app with a blurred
photographic sidebar. The phone bar went through a photo-wash version (rejected: muddy on a short
strip), a docked bar and a floating white bar, then the full pill with notch states (chosen). The
sidebar default is a preset, not the latest photo, so a private photo never shows on a shared screen.

## Color

Contrast ratios are computed against the surface each colour actually sits on: the white (or
#26272b) card, the panel (light #ececee at its darkest, dark #1d1e21), the tinted well composited on
the panel (light #e2e2e5, dark #222325; on a card: light #f4f4f5, dark #303135), and the rail at its
lightest (light #4c4d52, dark #303136).

| Token | Light | Dark | Role | Contrast light / dark |
|---|---|---|---|---|
| `--ground` | #e6e6e9 → #dadade | #111214 → #08080a | Page ground (gradient) | — |
| `--panel` | #f8f8f9 → #f5f5f6 | #1a1b1e → #18191c | The large rounded panel, near-flat | — |
| `--card` | #ffffff | #222327 | Cards, tables, dialogs, menus | — |
| `--hair` | rgb(20 20 30 / .08) | rgb(255 255 255 / .07) | 1 px edge of every card, table, menu and the panel | — |
| `--hair-strong` | rgb(20 20 30 / .14) | rgb(255 255 255 / .12) | Card hover edge, secondary button border | — |
| `--head` | rgb(20 20 30 / .025) | rgb(255 255 255 / .025) | Table header band | — |
| `--tint` | rgb(20 20 30 / .045) | rgb(255 255 255 / .045) | Wells, secondary buttons, tracks | — |
| `--tint-strong` | rgb(20 20 30 / .085) | rgb(255 255 255 / .085) | Hover on tinted controls | — |
| `--ink` | #111214 | #f1f1f2 | Text, icons | card 18.7 / 13.2 · tint 14.5 / 13.9 |
| `--ink-muted` | #5c5e65 | #9fa1a8 | Metadata, captions | card 6.5 / 5.8 · panel 5.5 / 6.5 · tint 5.0 / 5.0 |
| `--edge` | #7f8189 | #80838b | 1 px edges of inputs and toggles | card 3.9 / 3.9 · tint 3.5 / 3.4 |
| `--accent` | #9a3fd0 | #d7a2ff | Primary fill, progress, focus ring, selection ring | card 5.3 / 7.5 · panel 4.5 / 8.3 |
| `--accent-ink` | #8a3ac0 | #d7a2ff | Accent as text (links, active badge) | card 6.1 / 7.5 · tint 4.7 / 7.9 |
| `--on-accent` | #ffffff | #1f0b2e | Text on `--accent` | 5.3 / 9.2 |
| `--danger` | #b3261e | #ff9a8f | Destructive text and fills | card 6.5 / 7.3 · tint 5.1 / — |
| `--on-danger` | #ffffff | #222327 | Text on `--danger` | 6.5 / 7.3 |
| `--success` | #1d6b44 | #74d3a3 | Completed states | card 6.5 / 8.2 |
| `--warning` | #7a4f00 | #f0c062 | Drop notices, quota near full | tint 5.5 / 7.7 |
| `--rail` | #4c4d52 → #2a2b2f | #303136 → #1d1e21 | The Graphite sidebar preset and the fallback behind any wash; the lightbox stage | — |
| `--rail-scrim` | #0c0d0f | #0c0d0f | Scrim over the sidebar wash; its alpha is computed per wash (floor 22 %) | — |
| `--rail-ink` | #ffffff | #ffffff | Sidebar text, active label, sidebar focus ring | ≥ 4.5 on any wash (computed) |
| `--rail-muted` | rgb(255 255 255 / .82) | same | Inactive items and section labels | ≥ 4.5 on the wash's lightest point (computed) |
| `--rail-well` / `--rail-well-hover` | rgb(255 255 255 / .10) / .06 | same | Storage block and account controls / nav item hover | — |
| `--rail-active` / `--rail-active-ink` | rgb(255 255 255 / .17) / #ffffff | same | Active item: translucent white row, white ink | — |
| `--track` / `--fill` | rgb(20 20 30 / .07) / `--accent` | rgb(255 255 255 / .07) / `--accent` | Meter track and its fill | — |

Derived with `color-mix`, never hand-picked: `--accent-soft` (selection text highlight, selected
table rows), `--danger-soft` (error alerts, 9 % danger over the card), `--scrim` (behind dialogs). The storage meter's fill is decorative: the same figure is always printed as text next to it.

## Typography

Geist (variable, 100–900, OFL), self-hosted as woff2 (latin + latin-ext) under `web/static/fonts/`,
`font-display: swap`, fallback `ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto`.
Tabular figures on every size, count, date and progress figure. Monospace (`--font-mono`) only for
text previews and invitation links.

| Token | Size / line height | Use |
|---|---|---|
| `--text-xs` | 12 / 16 px | Badges, rail labels on phones, shortcut labels on phones |
| `--text-sm` | 13 / 18 px | Metadata, captions, hints, field labels |
| `--text-md` | 14 / 20 px (15 / 22 from 768 px) | Body, inputs, buttons, row names |
| `--text-lg` | 16 / 22 px | Card titles, storage heading, empty-state titles |
| `--text-xl` | 19 / 24 px | Section headings, dialog titles, wordmark |
| `--text-2xl` | 24 / 28 px | Page titles on phones |
| `--text-3xl` | 30 / 34 px | Page titles from 768 px |
| `--text-display` | 28 px (36 px from 768 px), line height 1.05 | The home greeting only |

Weights: body 400, labels 500 (`--weight-label`), headings 500 (`--weight-heading`), emphasis
600 (primary buttons, active rail label, wordmark). Tracking: page titles and greeting -0.035em,
section headings -0.02em, labels -0.01em. Headings use `text-wrap: balance`; body max 70ch.

## Spacing

8 px base with a 4 px half-step: `--space-1` 4, `--space-2` 8, `--space-3` 12, `--space-4` 16,
`--space-5` 24, `--space-6` 32, `--space-7` 48, `--space-8` 64. Layout: `--frame` (ground around
the panel) 12 px, 22 px from 1024 px; `--panel-pad` 18 / 24 (768) / 28 (1024); `--card-pad` 14 / 16;
`--well-pad` 16. Sidebar width 228 px (`--rail-w`), padding 16 top and bottom, 12 on the sides
(`--rail-pad-y`, `--rail-pad-x`), items 36 px (`--nav-item`), 18 px between groups. Tables: header
band `--table-head` 38 px, rows `--row-min` 52 px. Phone bar: `--rail-bar-h` 64 px, `--rail-bar-pad` 6 px, tab notch `--notch-w` × `--notch-h`
(20 × 3 px; `--notch-w-focus` 32 px for keyboard focus). Sidebar items are `--nav-gap` (2 px) apart. Touch targets at least 44 × 44 px everywhere: 36 px visuals
(small buttons, icon buttons, segments, breadcrumbs) carry an invisible 44 px hit area.

## Radii

`--radius-tile` 6 (photo tiles), `--radius-control` 8 (buttons, fields, icon buttons, icon tiles,
sidebar items), `--radius-sm` 8 (menu items, code blocks), `--radius-card` 12 (cards, tables,
alerts, menus), `--radius-well` 12 (wells, empty states), `--radius-panel` 18 (the panel, dialogs,
auth card), `--radius-rail` 18 (the sidebar), `--radius-full` for chips, badges, segment tracks and
their thumbs, toggles, avatars, the phone bar and its centre button. Nested corners are concentric:
inner radius = outer radius − padding (a 12 px menu with 4 px padding holds 8 px items; a table's
selection ring is inset 4 px with an 8 px radius).

## Depth (shadows)

A 1 px `--hair` edge plus one near-flat shadow per surface; never a highlight/shadow pair.

| Token | Light | Use |
|---|---|---|
| `--shadow-card` | 0 1 2 / 4 % | Cards, tables, white buttons |
| `--shadow-hover` | 0 1 2 / 5 %, 0 6 16 / 6 % | Hover on interactive cards; the phone bar and its centre button |
| `--shadow-pop` | 0 4 12 / 8 %, 0 20 48 / 14 % | Dialogs, menus, sheets |
| `--shadow-knob` | 0 1 3 / 10 % | Selected segment, toggle knob |
| `--shadow-active` | 0 1 2 / 12 % | Rarely used: a pressed white control |
| `--panel-edge` | inset 0 1 0 white 60 % | Top hairline of the panel |

Dark mode uses the same hairlines with deeper black shadows. Selection on cards and photos is a 2 px
accent ring (`--select-ring`); in tables, a selected row gets an `--accent-soft` fill and an inset
ring (see Table). On photos, the
hover selection circle is a white 70 % ring (`--photo-ring`) over a soft `--photo-shade`, so it
reads on bright and dark images alike.

## Motion

| Token | Value | Use |
|---|---|---|
| `--dur-fast` | 160 ms | Presses, colour and background changes, toggles |
| `--dur` | 240 ms | Card and row shadows, list arrival, menus |
| `--dur-slow` | 360 ms | Sheets and dialogs |
| `--ease` | cubic-bezier(.22, 1, .36, 1) | Everything (one curve) |
| `--rise` | 8 px | Arrival offset for rows and sections |
| `--rise-sheet` | 16 px | Sheet and dialog arrival offset |
| `--stagger` | 30 ms | Delay between arriving items, capped at 8 |

Animate only `transform`, `opacity`, `box-shadow`, `background-color`, `border-color`, `color` (and
the meter's `clip-path`). Dialogs reverse their arrival at `--dur-fast`. `prefers-reduced-motion: reduce` sets
every duration to 0 and removes the rises. On touch devices (`pointer: coarse`) the list/section
arrival is skipped (content appears at once): it stuttered on Firefox for Android (2026-11-14).
Presses, sheets and dialogs still animate there.

## Layout

- **Ground**: `--ground` gradient on the body. Auth pages put the wordmark and one white card on it.
- **Shell (signed in)**: from 1024 px a two-column grid, the rail (210 px, sticky, full height) and
  the panel. Below 1024 px the panel fills the width and the rail floats at the bottom
  (`--radius-rail`, `--shadow-pop`, 12 px from the edges, safe-area aware); the panel shows a small
  top row with the wordmark and the account menu.
- **Page**: breadcrumbs (inside folders), title (`h1`), muted description, actions on the right
  (full width under the title on phones).
  A sub-page (every page under Settings) puts a tinted `chevron-left` icon button named "Back to
  <parent>" before its title, linking to the parent page (`Page` component, `back` prop).
- **Ground layer**: the gradient sits on a fixed `body::before` layer, never
  `background-attachment: fixed` (full repaints on mobile). Viewport heights use `svh`.
- **Layout tokens**: `--sheet-max` (90dvh, tallest a sheet or dialog gets), `--center-min` (60dvh,
  centred loading/error states), `--underline-offset` (0.2em, link underlines).
- **Photo tokens** (validated 2026-11-14): `--photo-row` (justified row height: 112 px on phones,
  168 px from 768 px, 208 px from 1440 px), `--photo-month-head` 48 px and `--photo-day-head` 32 px
  (heading blocks of the timeline), `--select-check` 24 px (selection circle), `--album-min` 160 px
  (album card minimum width), `--album-ratio` 4 / 3 (album covers), `--picker-max` 352 px (tallest
  the album picker's list grows before it scrolls), `--scrubber-w` 56 px, `--scrub-tick` 2 px, `--info-w` 360 px (lightbox
  info panel from 1024 px), `--locator-dot` 6 px. Grid gaps are `--space-1` (4 px).
- **Explore tokens** (validated 2026-11-18): `--chip-h` 36 px (filter chips, = `--control-sm`),
  `--chip-clear` 20 px (a chip's clear icon), `--face-sm` 40 px, `--face-md` 64 px, `--face-lg`
  112 px (face avatars), `--person-min` 112 px (people grid minimum column), `--face-box` 2 px
  (face box outline in the lightbox).

## Components (states: default, hover, focus-visible, active, disabled)

Focus everywhere: 2 px `--accent` outline, 2 px offset (`--rail-ink` on the rail). Active: 1 px
press. Disabled: 45 % opacity, no shadow or glow, `cursor: not-allowed`. Hover only on
`(hover: hover)` devices.

- **Button / primary**: `--radius-control`, `--accent` fill, `--on-accent` label 600,
  `--shadow-card`; hover darkens the fill (accent 88 % with ink). Sizes: sm 36 px (44 px hit
  area, 12 px side padding), md 44 px (16), lg 48 px (24). Sized to the label, never stretched
  except `full`.
- **Button / secondary**: `--card` fill, 1 px `--hair-strong` border, `--ink` label, `--shadow-card`;
  hover darkens the border.
- **Button / quiet**: transparent, `--ink-muted` label; hover `--tint-strong` fill and `--ink`.
- **Button / danger**: secondary with a `--danger` label; **danger-solid** `--danger` fill and
  `--on-danger` label, for the final confirmation only.
- **Icon button**: 36 px, `--radius-control` (44 px hit area), quiet by default (`--ink-muted`, hover
  `--tint-strong`), `fill="tint"` for dialog headers; always has an accessible name that includes
  its subject (e.g. "Rename report.pdf").
- **Text field**: `--radius-control`, `--card` fill, 1 px `--edge`; hover edge `--ink-muted`; focus ring; invalid
  `--danger` edge plus an icon and message (never colour alone).
- **Segmented control** (theme picker): `--tint` pill track; chosen segment is a `--card` pill
  with `--shadow-knob` and `--ink` label; others `--ink-muted`, hover `--ink`.
- **Search field**: the text field with a leading 20 px `search` icon in `--ink-muted` and a
  visually hidden label. Results are row cards; matched words in snippets are `<mark>` with an
  `--accent-soft` background and `--ink` text (the text-highlight colour).
- **Toggle**: `--tint` track with 1 px `--edge`, white knob; checked fills the track with
  `--accent`.
- **Card**: `--card`, 1 px `--hair`, `--radius-card`, `--shadow-card`; interactive cards hover
  `--hair-strong` and `--shadow-hover`.
- **Table** (files, shared, trash, search results): one `--card` with `--hair` edge and
  `--radius-card`; a `--head` header band (`--table-head`, 12.5 px 500 `--ink-muted` labels), rows
  `--row-min` separated by `--hair`, a 32 px `--tint` icon tile (`--radius-control`), name 500,
  tabular `--ink-muted` size and date columns, kind as a badge, a quiet `more` icon button last.
  Hover: `--tint` fill. Keyboard focus: a 2 px `--accent` ring inset 4 px (radius 8). Selected:
  `--accent-soft` fill plus a 1 px accent ring inset 4 px. Phones keep name and date (size under
  the name).
- **Well**: `--tint`, `--radius-well`, `--well-pad`. Holds forms and empty states.
- **Row card** (files, trash, invitations, recent): white, `--radius-card`, 54 px min height,
  36 px round `--tint` icon well with an `--ink` icon, name 500 + muted tabular metadata, quiet
  round actions on the right; the name is a link or button stretched over the row; hover
  `--shadow-hover`; selection (the row a dialog acts on) adds `--select-ring`. Phones: one round
  "Actions for …" button opens an actions sheet; from 768 px the actions are inline.
- **Sidebar item**: `--nav-item` (36 px) row, `--radius-control`, 18 px icon and label 500 in
  `--rail-muted`; hover `--rail-well-hover` and `--rail-ink`; active `--rail-active` fill,
  `--rail-ink`, label 600; focus a 2 px `--rail-ink` ring inside the row. Section labels
  ("Library", "Sharing") are 12.5 px 500 `--rail-muted`.
- **Sidebar wash**: the sidebar's background, in `web/src/lib/sidebar.ts` (not tokens: radial washes
  have no native meaning). Presets Forest (default), Beach, Dusk, Graphite, or the owner's photo
  decoded from its blurhash; stored per device (`squirrel-sidebar`). Over it a `--rail-scrim` layer
  whose alpha is the smallest that gives `--rail-muted` 4.5:1 on the wash's lightest point (floor
  22 %). Changing the wash cross-fades in `--dur-slow`.
- **Shortcut** (home): a tinted pill holding a 40 px white round icon well and a label.
- **Meter** (storage): `--meter-track` (6 px) `--track` pill, `--fill` (accent) clipped to the value,
  figures printed as text beside it; warning fill from 90 %. On the sidebar: `--rail-well` block,
  `--rail-ink` fill on a translucent white track.
- **Progress** (uploads): 6 px `--track` pill, `--accent` fill, named `<progress>`.
- **Badge**: `--tint` pill, 12 px label 500 in the tone colour.
- **Alert**: `--radius-card`; error: `--danger-soft` background, `--danger` text; success: `--tint`
  background, `--success` text; information (e.g. "Preparing 12 photos…"): `--tint` background,
  `--ink` text.
- **Empty state**: `--tint` well, 56 px white round icon well with `--shadow-card`, title and muted
  line.
- **Dialog / sheet**: bottom sheet on phones, centred from 768 px (`wide` for previews);
  `--card`, `--radius-panel`, `--shadow-pop`, scrim with the page behind blurred 8 px (static), native `<dialog>` focus trap, round tint close
  button; footer buttons full width on phones.
- **Menu**: `--card`, 1 px `--hair`, `--radius-card`, 4 px padding, `--radius-sm` items, `--shadow-pop`; opens down from the phone top row, up from
  the rail.
- **Icons**: inline SVG (Lucide shapes), 1.75 px stroke, `currentColor`, 20 px. Decorative icons are
  `aria-hidden`. No emoji as icons.

### Photos (validated 2026-11-14)

- **Photo tile**: the image fills the tile (`object-fit: cover`), `--radius-tile` (6 px), no shadow;
  while loading, the tile shows the photo's blurhash (or its average colour), else `--tint` with a
  20 px `image` icon in `--ink-muted`. Hover (hover devices): a 24 px round selection circle appears
  top-left (`--photo-ring`). Selected: `--select-ring` and a filled `--accent` check circle with an
  `--on-accent` check. Focus: the global ring. Unsupported previews: `--tint` with the `image-off`
  icon. Tiles never animate in: the blurhash to image swap is instant.
- **Justified grid**: rows `--photo-row` high (112 px phones, 168 px from 768 px, 208 px from 1440 px),
  gaps `--space-1`; day labels `--text-sm` 500 `--ink-muted` tabular above each day; month headings
  `--text-lg` `--weight-heading` above each month. Months not loaded yet show `--tint` rows of the
  same height, never fake photos.
- **Scrubber** (from 768 px): a slim column (`--scrubber-w`) on the panel's right edge listing years
  as `--text-xs` tabular `--ink-muted` labels, with months as `--scrub-tick` `--edge` ticks; the current month
  is a `--card` pill with `--shadow-knob` and `--ink` label; dragging shows the month label in that
  pill. Phones: a 44 px round `--card` handle on the right edge appears while scrolling (no persistent
  column).
- **Lightbox**: a full-viewport graphite stage (`--rail` gradient) with the photo centred; controls
  are `--rail-well` circles with `--rail-ink` icons (as rail items); the info panel is a `--card` sheet
  (bottom sheet on phones, right-side panel `--info-w` wide from 1024 px). The place locator is a
  `--tint` well with `--tint-strong` land and a `--locator-dot` `--accent` dot.
- **Selection bar**: a `--card` pill with `--shadow-pop` floating above the phone rail (top of the
  panel from 1024 px): count (tabular), then small actions (icon only below 480 px, named for
  assistive tech).
- **Album card**: a white card (`--radius-card`, `--shadow-card`, hover `--shadow-hover`) holding a
  `--album-ratio` (4:3) cover with `--radius-tile` (the cover's blurhash colour while loading; an empty album shows
  `--tint` with the `library` icon), the name 500 and the count `--ink-muted` tabular. Two columns on
  phones, `auto-fill` of `--album-min` from 768 px.
- **Album strip** (phone timeline): above the first month, scrolling away with it, a section heading
  "Albums" with a quiet "See all" (to the albums), then one row of album cards `--album-min` wide,
  `--space-3` apart, that scrolls sideways: the 10 most recently updated albums, newest first.
  Hidden while there is no album.
- **Album picker**: a dialog / bottom sheet listing albums by name as rows in a `--tint` well (a
  40 px round white well with the cover, name, count), "New album…" first; the list scrolls past
  `--picker-max`.

### Explore and people (validated 2026-11-18)

No new colour: every value below is an existing token or one of the Explore tokens above.

- **Filter chip**: a `--chip-h` (36 px, 44 px hit area) pill on `--tint`, `--ink-muted` label with
  its leading icon (person avatar, `tag`, `map-pin`, `camera`, `image`). An active chip is a chosen
  segment: `--card` fill, `--shadow-knob`, `--ink` label, and a trailing `--chip-clear` (20 px) `x`
  button named "Remove filter …" with its subject ("Remove filter Paris"). Chips wrap; "Clear
  filters" is a quiet button after them. Hover `--tint-strong` (inactive) or `--shadow-hover`
  (active).
- **Face avatar**: a circle (`--radius-full`) of `--face-sm` 40, `--face-md` 64 or `--face-lg` 112
  px, `overflow: hidden`, on `--tint`. It is a **client-side crop** of the photo's thumbnail to the
  face box (`object-fit: cover`, the image scaled by 1 / box size and offset by the box): no face
  image exists on the server. Small faces (box height under about 0.08) crop the preview instead.
  While loading, or without a face, the circle shows the `user` icon in `--ink-muted`.
- **People grid**: `auto-fill` of `--person-min` (112 px) columns, 3 columns on phones; each person is
  a `--face-lg` avatar over the name (500, or "Unnamed" in `--ink-muted`) and the count
  (`--ink-muted`, tabular).
- **Face box** (lightbox, only while "People" is open in the info panel): a `--face-box` (2 px)
  `--photo-ring` outline, `--radius-tile`, over the face on the photo; no fill.
- **Tag and place cards**: the album card unchanged (cover, name, count). A place card adds the
  locator (well, `--tint-strong` land, `--locator-dot` `--accent` dot) under its name. Places are
  grouped under country headings (section heading style); no map tiles, no map library.

### Phone app (validated 2026-11-16)

The Android and iOS app (Compose Multiplatform, `mobile/`) implements this file once for both
platforms. It adds no value: every token comes from `tokens.css` through the generated
`mobile/theme/.../Tokens.kt` (`web/scripts/native-tokens.mjs`, drift-checked), never typed in.

- **Which values**: the phone column of `tokens.css` (below 768 px): `--text-md` 14 / 20, the phone
  `--shadow-card` / `--shadow-pop`, `--photo-row` 112. `1rem` is 16 dp; text sizes are `sp`.
  Gradients become two-stop vertical brushes; `color-mix` tokens are resolved by the generator.
- **Touch targets**: at least 48 dp on phones (`--control-lg`, the Android and iOS guideline),
  for every button, field, toggle, row and rail item; visuals keep their web sizes (a 36 dp
  icon button still draws 36 dp inside its 48 dp hit area). Icon-only buttons always have an
  accessible name.
- **Font**: Geist, the same variable font as the web, bundled as TTF (the platforms cannot load
  woff2), weights 400 / 500 / 600 and the tracking above.
- **Shell**: ground gradient and the floating white bottom bar, safe-area aware (see Bottom bar); the top row holds
  the wordmark and the account menu (server, device name, "Sign out"). Sign-in screens are the auth
  pages: wordmark and one white card on the ground.
- **Page header**: one per page, the page title (`--text-2xl`). A sub-page (Backup settings)
  puts a round back icon button (`chevron-left`, named "Back to …") before its own title instead
  of repeating the parent's; the system back gesture does the same.
- **Backup status card**: the Backup screen's one graphite card (allowed pattern "at most one
  graphite card per screen"): `--rail` gradient, `--rail-ink` featured figure ("1 204 left"),
  `--rail-muted` secondary line ("3.2 GB"), tabular figures, `--radius-card`. Progress inside it is
  the **Progress** component.
- **Permission notice**: an information **Alert** (`--tint`, `--ink`) with two buttons, secondary
  "Allow all photos" and quiet "Change selection", shown whenever photo access is partial.
- **Bottom bar**: a full pill (`--rail-bar-h` 64 dp, `--rail-bar-pad` 6 dp, `--card`, 1 dp `--hair`,
  `--shadow-hover`) floating 12 dp above the bottom inset. Five equal columns: Photos, Files, the
  Orchid add button, Shared, Backup. Tabs are icon-only (22 dp, content description = the tab name).
  Every tab state is a top notch (20 × 3 dp, rounded bottom corners) hanging just inside the top
  hairline: none at rest (`--ink-muted` icon); hover `--edge` notch, `--ink` icon; pressed `--accent`
  notch and icon, icon sinks 1 dp; selected `--accent` notch and `--accent-ink` icon; keyboard focus a
  32 dp `--ink` notch. No fills or circles on tabs. The add button is an `--accent` circle filling the
  inner height (`--rail-bar-h` − 2 × pad − 2), `--on-accent` plus icon that turns 45° while its menu
  is open, `--shadow-hover`. Its menu (a Menu above the bar, with an arrow) holds Upload files and New
  folder, into the folder open in Files, otherwise the root.
- **Lists**: rows grouped in one `--card` with `--hair` separators and 32 dp icon tiles, not separate
  row cards.
- **Gallery**: the photo tokens unchanged: rows `--photo-row` (112), tiles `--radius-tile` (6), gaps
  `--space-1`, month and day headings as on the web.
- **Motion**: list and section arrival is skipped (touch devices, see "Motion"); presses, sheets and
  dialogs animate with the same durations and ease. The system's reduced-motion setting (Android
  animator scale 0, iOS Reduce Motion) makes every duration 0.

## Theme

Follows the OS by default; the account menu offers System / Light / Dark, stored in
`localStorage` (`squirrel-theme`) and applied as `data-theme` on `<html>`. `static/theme-init.js`
applies it before first paint (a same-origin script, so the hash-mode CSP allows it).

## Rules

1. No literal colours, sizes, radii, shadows or durations outside `tokens.css` (1 px borders and
   media-query breakpoints excepted).
2. Every interactive element has all five states and a visible focus ring.
3. Text and icons meet 4.5:1, control edges and the accent 3:1, against the surface they sit on.
4. The accent appears only on the main action, progress, the active item, focus and selection.
5. Phone first: design at 375–412 px, then 768 / 1024 / 1440. Touch targets at least 44 px.
6. Show only real data: no invented statistics, counts or placeholders posing as content.
7. Keep E2E-relied labels, button names, headings and test ids unchanged.
