# Design audit, 2026-11-14

Final audit (genjutsu paint, Phase 5) of the web UI after the grey + Orchid rebuild (433dcd8, 806828e)
and the phone performance fixes (315177e, 614d6b4). Reference: `MASTER.md` (validated theses) and
`web/src/lib/styles/tokens.css`. The approved look is unchanged: every fix below is invisible at rest,
or restores motion that MASTER.md already specifies.

## Static pass (`design-audit/scripts/audit.py`)

The script detects only `web/src/lib` as a root when pointed at `web/src`, so it was run twice:
on `web/src` (83 files) and on `web/src/routes` (10 files). `app.css` is covered by neither run
and was read by hand.

| Run | Result |
|---|---|
| `web/src` (all groups) | 27 checks, 4 with findings, 1 not applicable |
| `web/src/routes` | 27 checks, 2 with findings, 1 not applicable |
| `--group tells`, both roots | 18 checks, 0 findings |

Not applicable (not checked, not passed): *Inline style object on an animated element*, no
`.jsx` / `.tsx` files.

Every finding, confronted:

- **CRITICAL outline-none**: `lib/components/ui/ListRow.svelte:138` and
  `lib/components/ui/Toggle.svelte:84`. Not a gap: the ring is moved, not removed. ListRow draws it
  on the whole row (`.row:has(.row-link:focus-visible)`, line 141), Toggle on its track (line 87).
  Confirmed in the browser: tabbing the files page shows a 2 px `#9a3fd0` ring on the `li` for every
  row link (see Measurements).
- **CRITICAL no-reduced-motion** (routes run only): artefact of scanning `routes/` without `lib/`.
  `tokens.css` zeroes every duration and rise under `prefers-reduced-motion: reduce`.
- **IMPORTANT hover-no-transition**, 22 in `lib`, 5 in `routes`: all false positives. Each rule
  changes a property that the element's base rule already transitions (e.g. `Button.svelte:80`,
  `IconButton.svelte:80`, `AppNav.svelte:99/111`, `+page.svelte:260/306/386`,
  `+layout.svelte:219/231`). The script only looks inside the hover block.
- **IMPORTANT js-driven-animation**: no timer drives anything visual. `ConfirmButton.svelte:37` is the
  8 s disarm timeout, `upload/retry.ts:28` the retry backoff, the rest are tests.
- **NICE-TO-HAVE will-change-broad**: `Dialog.svelte:104`. Kept: dialogs are only mounted while open
  (or fading out), so the layer exists only while the sheet moves. The comment explains why.
- **Tells**: none, and no em dash, filler verb, placeholder identity or invented figure in the markup.
  Manual read of copy held in script (nav labels, status badges, home shortcuts, upload status
  strings): all real data or plain labels, no tell.

Inventory: one easing (`--ease`), durations only from tokens (the one literal, `30ms`, is the
`--stagger` token), radii only `--radius-*` tokens, fonts Geist + the mono stack.

## Checklist with evidence

### Tokens only

- Grep of `*.svelte` / `*.css` outside `tokens.css` and `fonts.css` for hex, `rgb()`, `px`, `ms`,
  `rem`, `em`, `vh`: before the fixes, four literal values remained:
  `Dialog.svelte` `max-height: 90dvh`, `+layout.svelte` `.center { min-height: 60dvh }`,
  `app.css` `text-underline-offset: 0.2em`, and `outline: 2px` in the forced-colours blocks of
  `ListRow.svelte` and `ThemePicker.svelte`. **Fixed** (new tokens `--sheet-max`, `--center-min`,
  `--underline-offset`; outlines use `--ring-width`).
- Remaining literals are allowed or structural: 1 px borders, media-query breakpoints, the 1 px
  `.visually-hidden` technique, full-viewport `100svh` / `100vw`, z-index layers (10 rail, 20 menu,
  30 dialog), the home column ratio `1.15fr 1fr` (`routes/+page.svelte:192`).

### States and focus

- Five states per component read in code: Button (`Button.svelte:110-171`), IconButton (`:93-126`),
  TextField (`:55-98`), Toggle, ThemePicker segments, rail items, user-menu trigger and log-out,
  shortcuts, the round upload button (`routes/+page.svelte:248-284`), row cards, breadcrumbs, move
  dialog folders, action sheet items. Hover is wrapped in `(hover: hover)` everywhere.
- Presses: three elements sank 1 px instantly because `transform` was missing from their transition:
  ListRow row, breadcrumb links, move-dialog folders. **Fixed** (160 ms, `--ease`).
- Focus (keyboard tab through home, files, trash and invitations at 390 and 1280 px): every stop
  shows a 2 px ring; `#9a3fd0` on the panel and cards, `#f4f4f5` (`--rail-ink`) on the rail.

### Contrast (computed from token values, composited on the real surface)

Light: `--ink-muted` #5c5e65 on card 6.47, on panel #ececee 5.49, on tint over panel #e2e2e5 5.01;
`--accent-ink` #8a3ac0 on panel 5.16, on tint over card 5.54; white on `--accent` 5.26;
`--danger` #b3261e on `--danger-soft` #f8ebeb 5.63; `--success` #1d6b44 on tint over panel 5.02;
`--warning` #7a4f00 on panel (drop notice) 6.04; `--rail-muted` on rail top #4c4d52 5.10;
`--rail-active-ink` on white 6.09. Controls: `--edge` on card 3.89, on tint over panel 3.01 (lowest,
passes); accent focus ring on tint over panel 4.07; progress fill on its track 4.57.

Dark: `--ink-muted` #9fa1a8 on card #26272b 5.78, on tint over card 5.03; `--accent-ink` on tint over
card 6.50; `--on-accent` on accent 9.17; `--danger` on `--danger-soft` 6.15; `--on-danger` on danger
7.30; `--success` on tint 8.68; `--warning` on panel 9.86; `--rail-muted` on rail top 5.27;
`--rail-active-ink` on #f1f1f2 5.40; `--edge` on card 3.93.

All text ≥ 4.5:1, all control edges and rings ≥ 3:1.

### Touch targets (390 px, Playwright, hit-testing ±22 px around each control's centre)

- Phone-top wordmark link ("Squirrel", home) was 24 px tall. **Fixed**: 44 px (`+layout.svelte:191`),
  the row already held the 44 px account button, so nothing moves.
- Breadcrumb links: 44 px tall hit area already, but a very short folder name left it narrower than
  44 px. **Fixed**: the invisible hit area is at least 44 px wide.
- Everything else passes: rail items 44 × 64, icon buttons and small buttons 36 px visuals with a
  44 px hit area, row links stretched over 54 px row cards, theme segments 44 px through the radio.
  The probe flagged exact-44 px controls (upload button, "All files", icon-only "Restore") at the
  sub-pixel edge; checked by hand, they are 44 × 44.

### Motion

- Transitioned properties found on every page at all widths: `background-color`, `border-color`,
  `box-shadow`, `color`, `transform` (plus `opacity` / `display` / `overlay` on dialogs). No layout
  property is transitioned or animated.
- Arrival: on desktop only the `arrive` keyframes (opacity + transform, 240 ms) run; on a
  coarse-pointer phone no animation runs at all (`app.css:161`), as decided in 614d6b4.
- One ease, three durations, all from tokens; reduced motion zeroes them (`tokens.css` end).
- **Dialog exits never played.** `Dialog.svelte` defines the reverse transition at `--dur-fast`, but
  every parent unmounts the dialog on `close`, so the element vanished at once. **Fixed**: the
  `<dialog>` stays mounted for `--dur-fast` after closing (`out:linger|global`, 0 ms with reduced
  motion), is `inert` and takes no clicks meanwhile, and stays `position: fixed` so it cannot jump
  where the top-layer `overlay` transition is unsupported. `FolderView` keys its dialog block so a
  dialog opened during that fade gets a fresh instance (otherwise Svelte resumed the closing one and
  it stayed shut). Measured: 60 ms after Escape the sheet is at opacity 0.07, translateY 14.8 px,
  removed by 180 ms; with reduced motion it is removed at once; reopening another file immediately
  opens it.
- Menus (account menu) arrive with `@starting-style` and close instantly. MASTER asks exits only of
  dialogs, so this is left as is (see suggestions).

### Layout and themes

- No horizontal overflow at 390, 768, 1024 and 1440 px, light and dark, on home, files, trash and
  invitations (`scrollWidth − innerWidth = 0` everywhere).
- Re-shot after the fixes (light and dark, 390 and 1280): home, files root, empty folder, file list,
  text and image previews, new-folder dialog, action sheet, move dialog, account menu. Identical to
  the approved screens apart from seed data.

### Phone performance hazards

- None found in computed styles on any page: no `backdrop-filter`, no `background-attachment: fixed`
  (the ground gradient is on the composited `body::before`, `app.css:35`), no `filter`.
- Box-shadow transitions only run on hover devices; on phones `--shadow-hover` equals
  `--shadow-card`, so rows never animate a shadow. Shadows on phones are the lighter set
  (`tokens.css`, `max-width: 47.99rem`).
- The floating rail and the ground are on their own layers (`translateZ(0)`).
- No layout thrash in scroll or animation paths (no scroll listeners; the only measured style read is
  the dialog's one `getComputedStyle` when it closes).

### Other

- No emoji used as icons: inline Lucide SVGs, `aria-hidden` when decorative (`Icon.svelte:25`).
- Forced colours: Toggle lost its knob (fills are dropped). **Fixed** with system colours. Toggle is
  not used by any page yet.

## Fixes applied (one commit, `fix(web): design audit fixes`)

1. `tokens.css`: `--sheet-max`, `--center-min`, `--underline-offset` replace three literals.
2. `Dialog.svelte`: exit transition now plays; `inert` + no pointer events while fading; fixed
   positioning while closing.
3. `FolderView.svelte`: `{#key dialog}` around the dialogs.
4. `ListRow.svelte`, `Breadcrumbs.svelte`, `MoveDialog.svelte`: press transition on `transform`.
5. `+layout.svelte`: 44 px phone wordmark link; `--center-min`.
6. `Breadcrumbs.svelte`: hit area at least 44 px wide.
7. `ListRow.svelte`, `ThemePicker.svelte`: forced-colours outlines use `--ring-width`.
8. `Toggle.svelte`: forced-colours knob and track.
9. `app.css`: `--underline-offset`.

## UNVERIFIED (needs a real device or browser)

- Firefox for Android: smoothness of the new 160 ms dialog exit, and that the sheet fades in place.
  Firefox does not transition `overlay`, so the closing dialog leaves the top layer (and its scrim
  disappears) at once; `position: fixed` and `z-index: 30` should keep it where it was. Only
  Chromium was available here.
- Safari / iOS: dialog entry and exit (`@starting-style`, `allow-discrete`), `svh` units, safe-area
  insets on the floating rail and the bottom sheet.
- Forced colours (Windows High Contrast): focus rings, row borders, the Toggle and Meter fallbacks
  are written but not seen.
- Screen readers: the `inert` closing dialog is hidden from assistive tech in browsers; Playwright's
  `getByRole('dialog')` still counts it for those 160 ms, so future E2E tests should wait for the
  previous dialog to be gone or scope to `dialog[open]`.
- Frame timing of row hover shadows on low-end desktops (box-shadow is a paint, not a composite).

## For the user (would change the design, or MASTER.md)

- **`border-color` is animated** on text fields and the Toggle track (160 ms on hover and focus).
  MASTER.md's whitelist is `transform`, `opacity`, `box-shadow`, `background-color`, `color`. It is a
  cheap paint, so the suggestion is to add `border-color` to the whitelist rather than make the edge
  snap. Left unchanged.
- **Account menu exit.** It closes instantly while it opens with a 240 ms rise. Giving it a matching
  exit means keeping it mounted (and giving the two theme radio groups distinct names); MASTER does
  not ask for it today.
- **Upload panel and alerts** appear and disappear without motion. Consistent with MASTER (no rule
  for them), noted only for completeness.
- MASTER.md's token tables do not list the three new tokens (`--sheet-max`, `--center-min`,
  `--underline-offset`); they encode existing values, so no re-validation is needed, but you may
  want them added to the Layout section.
