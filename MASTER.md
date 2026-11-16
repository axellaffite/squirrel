# Squirrel design system

Single source of truth for Squirrel's visual layer. Every color, size, radius, shadow and duration
in `web/` comes from `web/src/lib/styles/tokens.css`, which implements this file. No magic numbers.

## Theses (validated 2026-11-14)

**Visual thesis.** A calm, gallery-grey home drive. A soft grey-gradient ground holds one large
rounded panel; a graphite rail carries navigation (sidebar from 1024 px, a floating graphite bottom
bar on phones); content sits on white cards with a single soft diffuse shadow, or on faintly tinted
wells. Headings are light and tight (Geist 500, letter-spacing -0.02 to -0.035em) but modest in
size; everything interactive is a pill or a circle; icons live in round wells. One accent, Orchid
(#9a3fd0 light, #d7a2ff dark), is spent only on the main action, progress and the active item,
with a soft coloured glow.

**Interaction thesis.** Calm and exact: 160 ms for small controls, 240 ms for cards, rows and
hover depth, 360 ms for sheets and dialogs, all on one ease, cubic-bezier(.22, 1, .36, 1). Presses
sink 1 px (translateY). Hover changes a background or a shadow only, never a size. A view's rows
arrive once per load, rising 8 px and fading in, 30 ms apart, capped at 8: no scroll-triggered
reveals. Never: bounce or overshoot, hover scaling, parallax, looping animations other than real
progress, animated width, height or position. With reduced motion, everything changes instantly.

**Allowed patterns.** One diffuse drop shadow (never a paired highlight/shadow); grey gradients on
the ground, the panel and the rail only; the accent glow on the primary button (and the round
upload button) only; at most one graphite card per screen for a featured figure; pill segmented
controls; a white pill inside a tinted pill track for the storage meter; tabular figures for sizes,
counts and dates.

**Brainstorm record.** 2026-11-14: the first validated direction (soft neumorphism, porcelain and
charcoal, paired raised/inset shadows, Manrope) was rejected once built. It was replaced the same
day by this reference-inspired direction: gallery grey and graphite with a single Orchid accent
(chosen over Violet #6e4ff6 and Iris #5b5bd6), Geist, and everything about one size smaller than
the first proposal (titles and controls 15–35 % smaller, icons unchanged at 20 px). Dark theme is
an extrapolation (the reference is light only): graphite ground and panel, #26272b cards, a lighter
Orchid. Product truth unchanged: "it's ours, at home", a private stash shared only with invited
people.

## Color

Contrast ratios are computed against the surface each colour actually sits on: the white (or
#26272b) card, the panel (light #ececee at its darkest, dark #1d1e21), the tinted well composited on
the panel (light #e2e2e5, dark #222325; on a card: light #f4f4f5, dark #303135), and the rail at its
lightest (light #4c4d52, dark #303136).

| Token | Light | Dark | Role | Contrast light / dark |
|---|---|---|---|---|
| `--ground` | #e4e4e7 → #cfcfd3 | #17181b → #0b0b0d | Page ground (gradient) | — |
| `--panel` | #f6f6f7 → #ececee | #1d1e21 → #18191b | The large rounded panel | — |
| `--card` | #ffffff | #26272b | Cards, row cards, dialogs, menus | — |
| `--tint` | rgb(20 20 30 / .045) | rgb(255 255 255 / .045) | Wells, secondary buttons, tracks | — |
| `--tint-strong` | rgb(20 20 30 / .085) | rgb(255 255 255 / .085) | Hover on tinted controls | — |
| `--ink` | #111214 | #f1f1f2 | Text, icons | card 18.7 / 13.2 · tint 14.5 / 13.9 |
| `--ink-muted` | #5c5e65 | #9fa1a8 | Metadata, captions | card 6.5 / 5.8 · panel 5.5 / 6.5 · tint 5.0 / 5.0 |
| `--edge` | #7f8189 | #80838b | 1 px edges of inputs and toggles | card 3.9 / 3.9 · tint 3.5 / 3.4 |
| `--accent` | #9a3fd0 | #d7a2ff | Primary fill, progress, focus ring, selection ring | card 5.3 / 7.5 · panel 4.5 / 8.3 |
| `--accent-ink` | #8a3ac0 | #d7a2ff | Accent as text (links, active badge) | card 6.1 / 7.5 · tint 4.7 / 7.9 |
| `--on-accent` | #ffffff | #1f0b2e | Text on `--accent` | 5.3 / 9.2 |
| `--danger` | #b3261e | #ff9a8f | Destructive text and fills | card 6.5 / 7.3 · tint 5.1 / — |
| `--on-danger` | #ffffff | #26272b | Text on `--danger` | 6.5 / 7.3 |
| `--success` | #1d6b44 | #74d3a3 | Completed states | card 6.5 / 8.2 |
| `--warning` | #7a4f00 | #f0c062 | Drop notices, quota near full | tint 5.5 / 7.7 |
| `--rail` | #4c4d52 → #2a2b2f | #303136 → #1d1e21 | Graphite rail (gradient) | — |
| `--rail-ink` | #f4f4f5 | #f1f1f2 | Rail text, active label, rail focus ring | 7.7 / 11.5 |
| `--rail-muted` | #c8c9cd | #a3a5ab | Inactive rail labels and icons | 5.1 / 5.3 |
| `--rail-well` | rgb(255 255 255 / .08) | rgb(255 255 255 / .07) | Round wells on the rail | — |
| `--rail-active` / `--rail-active-ink` | #ffffff / #8a3ac0 | #f1f1f2 / #8a3ac0 | Active item: white circle, orchid icon | 6.1 / 5.4 |
| `--track` / `--fill` | rgb(20 20 30 / .07) / #ffffff | rgb(255 255 255 / .07) / #e9e9ec | Meter track and its pill fill | — |

Derived with `color-mix`, never hand-picked: `--accent-soft` (selection text highlight),
`--danger-soft` (error alerts, 9 % danger over the card), `--glow` (accent at 45 %),
`--scrim` (behind dialogs). The storage meter's white fill on its tint track is decorative: the
same figure is always printed as text next to it.

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
`--well-pad` 16. Rail width 210 px. Touch targets at least 44 × 44 px everywhere: 36 px visuals
(small buttons, icon buttons, segments, breadcrumbs) carry an invisible 44 px hit area.

## Radii

`--radius-sm` 12 (code blocks, focus outlines on text links), `--radius-tile` 6 (photo tiles: 18 is
too round for dense 112 px tiles), `--radius-card` 18 (cards, row cards, alerts, menus), `--radius-well` 22 (tinted wells, empty states), `--radius-panel` 26 (the panel,
dialogs, auth card), `--radius-rail` 28 (the rail), `--radius-full` for every button, field,
segmented control, badge and icon well.

## Depth (shadows)

One diffuse drop shadow per surface, never a highlight/shadow pair.

| Token | Light | Use |
|---|---|---|
| `--shadow-card` | 0 1 2 / 4 %, 0 12 32 / 8 % | Cards, row cards, white icon wells |
| `--shadow-hover` | 0 1 2 / 5 %, 0 16 40 / 12 % | Hover on interactive cards and rows |
| `--shadow-pop` | 0 2 6 / 6 %, 0 24 64 / 18 % | Dialogs, menus, the floating phone rail |
| `--shadow-knob` | 0 2 8 / 8 % | Selected segment, meter fill, toggle knob |
| `--shadow-active` | 0 4 14 / 25 % black | Active rail item |
| `--glow` / `--glow-hover` | 0 8 22 accent 45 % / 0 10 28 accent 55 % | Primary button and round upload button only |
| `--panel-edge` | inset 0 1 0 white 60 % | Top hairline of the panel |

Dark mode replaces the card shadows with a faint top hairline plus a deeper black drop shadow.
Selection is a 2 px accent ring (`--select-ring`), stacked on the card shadow. On photos, the
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
- **Ground layer**: the gradient sits on a fixed `body::before` layer, never
  `background-attachment: fixed` (full repaints on mobile). Viewport heights use `svh`.
- **Layout tokens**: `--sheet-max` (90dvh, tallest a sheet or dialog gets), `--center-min` (60dvh,
  centred loading/error states), `--underline-offset` (0.2em, link underlines).
- **Photo tokens** (validated 2026-11-14): `--photo-row` (justified row height: 112 px on phones,
  168 px from 768 px, 208 px from 1440 px), `--photo-month-head` 48 px and `--photo-day-head` 32 px
  (heading blocks of the timeline), `--select-check` 24 px (selection circle), `--album-min` 160 px
  (album card minimum width), `--scrubber-w` 56 px, `--scrub-tick` 2 px, `--info-w` 360 px (lightbox
  info panel from 1024 px), `--locator-dot` 6 px. Grid gaps are `--space-1` (4 px).

## Components (states: default, hover, focus-visible, active, disabled)

Focus everywhere: 2 px `--accent` outline, 2 px offset (`--rail-ink` on the rail). Active: 1 px
press. Disabled: 45 % opacity, no shadow or glow, `cursor: not-allowed`. Hover only on
`(hover: hover)` devices.

- **Button / primary**: pill, `--accent` fill, `--on-accent` label 600, `--glow`; hover
  `--glow-hover`. Sizes: sm 36 px (44 px hit area), md 44 px, lg 48 px.
- **Button / secondary**: `--tint` fill, `--ink` label; hover `--tint-strong`.
- **Button / quiet**: transparent, `--ink-muted` label; hover `--tint-strong` fill and `--ink`.
- **Button / danger**: secondary with a `--danger` label; **danger-solid** `--danger` fill and
  `--on-danger` label, for the final confirmation only.
- **Icon button**: 36 px circle (44 px hit area), quiet by default (`--ink-muted`, hover
  `--tint-strong`), `fill="tint"` for dialog headers; always has an accessible name that includes
  its subject (e.g. "Rename report.pdf").
- **Text field**: pill, `--card` fill, 1 px `--edge`; hover edge `--ink-muted`; focus ring; invalid
  `--danger` edge plus an icon and message (never colour alone).
- **Segmented control** (theme picker): `--tint` pill track; chosen segment is a `--card` pill
  with `--shadow-knob` and `--ink` label; others `--ink-muted`, hover `--ink`.
- **Toggle**: `--tint` track with 1 px `--edge`, white knob; checked fills the track with
  `--accent`.
- **Card**: `--card`, `--radius-card`, `--shadow-card`; interactive cards hover `--shadow-hover`.
- **Well**: `--tint`, `--radius-well`, `--well-pad`. Holds row cards, the storage meter, forms.
- **Row card** (files, trash, invitations, recent): white, `--radius-card`, 54 px min height,
  36 px round `--tint` icon well with an `--ink` icon, name 500 + muted tabular metadata, quiet
  round actions on the right; the name is a link or button stretched over the row; hover
  `--shadow-hover`; selection (the row a dialog acts on) adds `--select-ring`. Phones: one round
  "Actions for …" button opens an actions sheet; from 768 px the actions are inline.
- **Rail item**: 36 px round `--rail-well` holding the icon, label beside it (sidebar) or under it
  (bottom bar); hover `--rail-well-hover` and `--rail-ink`; active white circle
  (`--rail-active`), orchid icon (`--rail-active-ink`), `--shadow-active`, label 600.
- **Shortcut** (home): a tinted pill holding a 40 px white round icon well and a label.
- **Meter** (storage): 38 px `--track` pill with 6 px padding, white pill fill clipped to the value
  (never narrower than a circle), figures printed as text beside it; warning fill from 90 %.
- **Progress** (uploads): 6 px `--track` pill, `--accent` fill, named `<progress>`.
- **Badge**: `--tint` pill, 12 px label 500 in the tone colour.
- **Alert**: `--danger-soft` (error) or `--tint` (success) with tone text, `--radius-card`.
- **Empty state**: `--tint` well, 56 px white round icon well with `--shadow-card`, title and muted
  line.
- **Dialog / sheet**: bottom sheet on phones, centred from 768 px (`wide` for previews);
  `--card`, `--radius-panel`, `--shadow-pop`, scrim, native `<dialog>` focus trap, round tint close
  button; footer buttons full width on phones.
- **Menu**: `--card`, `--radius-card`, `--shadow-pop`; opens down from the phone top row, up from
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
  as `--text-xs` tabular `--ink-muted` labels, with months as `--scrub-tick` ticks; the current month
  is a `--card` pill with `--shadow-knob` and `--ink` label; dragging shows the month label in that
  pill. Phones: a 44 px round `--card` handle on the right edge appears while scrolling (no persistent
  column).
- **Lightbox**: a full-viewport graphite stage (`--rail` gradient) with the photo centred; controls
  are `--rail-well` circles with `--rail-ink` icons (as rail items); the info panel is a `--card` sheet
  (bottom sheet on phones, right-side panel `--info-w` wide from 1024 px). The place locator is a
  `--tint` well with `--tint-strong` land and a `--locator-dot` `--accent` dot.
- **Selection bar**: a `--card` pill with `--shadow-pop` floating above the phone rail (top of the
  panel from 1024 px): count (tabular), then actions.

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
