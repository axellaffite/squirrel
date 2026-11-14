# Squirrel design system

Single source of truth for Squirrel's visual layer. Every color, size, radius, shadow and duration
in `web/` comes from `web/src/lib/styles/tokens.css`, which implements this file. No magic numbers.

## Theses (validated 2026-11-14)

**Visual thesis.** A soft, moulded home drive: cool porcelain (#e3e8ef) by day and deep charcoal
(#25272b) by night, every surface shaped by a paired highlight and shadow, raised for cards and
pressed in for wells and active states. Controls stay legible through solid porcelain-indigo fills
(#3f55c4 light, #9fb0ff dark) and 1 px edges of at least 3:1. Manrope throughout, chosen for its
even digits and calm long names: body at 15–16 px, headings at 20–28 px in weights 650–700,
figures tabular. Airy spacing on an 8 px base, generous card padding, radii from 12 to 24 px, and
photos as the only saturated content on screen.

**Interaction thesis.** Soft and tactile: 240 ms on a long ease-out (cubic-bezier(.22,1,.36,1)),
160 ms for small controls, 360 ms for sheets and dialogs. Presses sink into an inset shadow; on
desktop, hover deepens the shadow slightly without scaling. A view's photos, files and rows fade
in rising 8 px, 30 ms apart and capped at 8, only when the view first loads: no scroll-triggered
reveals. Toggles glide, sheets rise 16 px from the bottom. Never: bounce or overshoot, parallax,
hover scaling, looping animations other than real progress, or animated width, height or
position. With reduced motion, everything changes instantly.

**Allowed patterns.** Dual neumorphic shadows (raised and inset); a soft gradient or blurred colour
preview only as a photo placeholder while it loads; tabular figures for sizes and counts; the
storage meter as a pressed-in track.

**Brainstorm record.** Full redesign. Product truth: "it's ours, at home", a private stash shared
only with invited people. Style: soft UI, readable (moulded surfaces, real fills and edges on
controls). Dials: variance 3 (centred, calm), motion 4 (standard, soft), density 3 (spacious).

## Color

Contrast is computed against the theme's `--surface`.

| Token | Light (porcelain) | Dark (charcoal) | Role | Contrast L / D |
|---|---|---|---|---|
| `--surface` | #e3e8ef | #25272b | App ground and every moulded surface | — |
| `--surface-hi` | #f8fafd | #30333a | Highlight half of the paired shadow | — |
| `--surface-lo` | #bfc7d3 | #18191c | Shadow half of the paired shadow | — |
| `--ink` | #29313c | #ebe9e5 | Primary text, icons | 10.7 / 12.3 |
| `--ink-muted` | #56627a | #a8a49d | Secondary text, captions | 5.0 / 6.0 |
| `--edge` | #7a8496 | #737781 | 1 px edges of inputs, toggles, outlined buttons | 3.06 / 3.34 |
| `--accent` | #3f55c4 | #9fb0ff | Primary fill, active nav, progress, focus ring | 5.2 / 7.2 |
| `--on-accent` | #ffffff | #141a33 | Text on `--accent` | 6.4 / 8.3 (vs accent) |
| `--danger` | #b3261e | #ff9a8f | Destructive text and fills | 5.3 / 7.3 |
| `--success` | #1d6b44 | #74d3a3 | Completed states | 5.3 / 8.3 |
| `--warning` | #7a4f00 | #f0c062 | Quota near full, retries | 5.8 / 8.9 |

Derived (computed with `color-mix`, never hand-picked): `--accent-soft` (accent 14 % over surface,
selection backgrounds), `--danger-soft`, `--scrim` (ink 40 %, behind dialogs).

## Typography

Manrope (variable, 400–800), self-hosted as woff2 under `web/static/fonts/`, `font-display: swap`,
fallback stack `"Manrope", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`.
Tabular figures (`font-variant-numeric: tabular-nums`) on every size, count, date column and
progress figure.

| Token | Size / line height | Weight | Use |
|---|---|---|---|
| `--text-xs` | 12 / 16 px | 600 | Badges, upper-case labels (letter-spacing .04em) |
| `--text-sm` | 13.5 / 20 px | 500 | Metadata, captions, table secondary columns |
| `--text-md` | 15 / 22 px (16 / 24 on ≥ 768 px) | 450 | Body, inputs, list rows |
| `--text-lg` | 18 / 26 px | 600 | Card titles, dialog titles |
| `--text-xl` | 20 / 28 px | 650 | Section headings |
| `--text-2xl` | 24 / 30 px | 700 | Page titles on phones |
| `--text-3xl` | 28 / 34 px | 700 | Page titles on ≥ 768 px |

Headings use `text-wrap: balance` and letter-spacing -0.01em. Body max line length 70ch.

## Spacing

8 px base, 4 px half-step: `--space-1` 4, `--space-2` 8, `--space-3` 12, `--space-4` 16,
`--space-5` 24, `--space-6` 32, `--space-7` 48, `--space-8` 64. Page gutter 16 px on phones,
24 px from 768 px, content max-width 1120 px. Card padding `--space-4` (phone) / `--space-5`.
Touch targets at least 44 × 44 px.

## Radii

`--radius-sm` 12 (chips, small buttons, thumbnails), `--radius-md` 16 (buttons, inputs, rows),
`--radius-lg` 20 (cards), `--radius-xl` 24 (sheets, dialogs, bottom nav), `--radius-full` 999.

## Depth (shadows)

Always a pair: highlight up-left, shadow down-right, from `--surface-hi` / `--surface-lo`.

| Token | Value (offset / blur) | Use |
|---|---|---|
| `--raise-1` | ±3 px / 7 px | Small controls, chips, thumbnails |
| `--raise-2` | ±6 px / 14 px | Cards, rows, buttons at rest |
| `--raise-3` | ±10 px / 24 px | Desktop hover on cards, floating upload button |
| `--inset-1` | inset ±2 px / 5 px | Pressed small controls, toggle track |
| `--inset-2` | inset ±4 px / 9 px | Search field, storage track, active nav item, pressed buttons |

Dialogs and sheets add a `--scrim` behind them; they are raised-3, never glass.

## Motion

| Token | Value | Use |
|---|---|---|
| `--dur-fast` | 160 ms | Toggles, presses, focus, small state changes |
| `--dur` | 240 ms | Cards, rows, list arrival, hover depth |
| `--dur-slow` | 360 ms | Sheets, dialogs, page-level transitions |
| `--ease` | cubic-bezier(.22, 1, .36, 1) | Everything (one curve) |
| `--rise` | 8 px | Arrival offset for list/grid items |
| `--rise-sheet` | 16 px | Sheet/dialog arrival offset |
| `--stagger` | 30 ms | Delay between arriving items, capped at 8 items |

Animate only `transform`, `opacity`, `box-shadow`, `background-color`, `color`. Arrival runs once per
view load. Exit: dialogs and sheets reverse their arrival at `--dur-fast`; removed rows fade out at
`--dur-fast`. `@media (prefers-reduced-motion: reduce)` sets every duration to 0 and removes rises.

## Component metrics

Also in `tokens.css`: `--icon-sm` 20 / `--icon-md` 24 px, `--ring-width` / `--ring-offset` 3 px,
`--tap` 44 px, `--row-min` 56 px, `--meter-track` 12 px, `--press` 1 px (pressed translateY),
`--disabled-opacity` .45, and layout widths (`--auth-max`, `--dialog-max`, `--menu-w`, `--name-max`).

## Theme

Follows the OS by default; the user menu offers System / Light / Dark, stored in `localStorage`
(`squirrel-theme`) and applied as `data-theme` on `<html>`. `static/theme-init.js` applies it
before first paint (a same-origin script, so the hash-mode CSP allows it without an inline script).

## Base components (all five states: default, hover, focus-visible, active, disabled)

- **Button / primary**: `--accent` fill, `--on-accent` label, `--radius-md`, `--raise-2`; hover
  (pointer devices) `--raise-3`; active `--inset-2` + translateY(1px); focus 3 px `--accent` ring
  offset 3 px; disabled 45 % opacity, no shadow, `cursor: not-allowed`.
- **Button / secondary**: `--surface` fill, `--ink` label, `--raise-2`, 1 px transparent edge
  (becomes `--edge` in forced-colors); same state rules.
- **Button / quiet**: no fill, `--ink-muted` → `--ink` on hover, inset on active. Used in rows.
- **Button / danger**: `--danger` label on secondary, or `--danger` fill for the final confirm.
- **Input / search**: `--inset-2` well, 1 px `--edge`, `--radius-md`, 44 px min height; focus adds
  the accent ring; invalid uses `--danger` edge plus a message (never colour alone).
- **Toggle**: 1 px `--edge` track with `--inset-1`; knob raised-1 on surface; checked knob fills
  `--accent`, track keeps the edge; glides at `--dur-fast`.
- **Card**: `--surface`, `--radius-lg`, `--raise-2`, padding per spacing; interactive cards get
  hover `--raise-3` and active `--inset-2`.
- **List row (file)**: 56 px min height, icon well (`--inset-1`, `--radius-sm`), name + metadata,
  quiet action buttons; selection uses `--accent-soft`.
- **Nav**: bottom bar on phones (`--radius-xl`, `--raise-2`, safe-area aware), top bar on ≥ 1024 px;
  active item `--inset-2` with accent icon and label.
- **Meter**: `--inset-2` track, accent fill via `transform: scaleX()`, tabular figures.
- **Dialog / sheet**: bottom sheet on phones, centred dialog ≥ 768 px; `--radius-xl`, `--raise-3`,
  scrim, focus trap (native `<dialog>`), arrival per motion tokens.
- **Icons**: inline SVG, 1.75 px stroke, `currentColor`, 20 / 24 px. No emoji as icons.

## Rules

1. No literal colours, sizes, radii, shadows or durations outside `tokens.css`.
2. Every interactive element has all five states and a visible focus ring.
3. Text and icons meet 4.5:1, control edges and the accent 3:1 against `--surface`.
4. Phone first: design at 375–412 px, then 768 / 1024 / 1440.
5. Keep E2E-relied labels, button names, headings and test ids unchanged.
