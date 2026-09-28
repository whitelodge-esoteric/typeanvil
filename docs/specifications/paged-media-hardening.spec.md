---
title: Paged-Media Hardening
slug: /specifications/paged-media-hardening
type: spec
status: draft
owner: maintainers
created: 2026-08-18
updated: 2026-09-27
sidebar_position: 11
tags: [engine, css, paged-media, page]
spec_id: paged-media-hardening
applies_to: engine 0.x
dependencies: [paged-media-css, wpt-conformance-harness]
---

# Paged-Media Hardening

## Overview

The `@page` layer — size, margins, margin boxes, named
pages, `:first`/`:left`/`:right` — well enough for the demo documents. The
initial WPT conformance baseline (2026-08-17) shows the layer is **partial**: 46 css-page
print-reftest failures cluster in:

- `page-box-*` — page box background painting
- `page-margin-*` — margin shorthand (1–4 values), auto margins, `:left` /
  `:right` margin differences (the initial implementation tested only `margin-top` for
  `:left`/`:right`)
- `page-size-006/009/013/014` — `px` lengths and `size` overrides
- `page-name-*` — named page sizes
- `page-orientation-*` — landscape / portrait keywords
- `page-rule-specificity-*` — resolution between overlapping `@page` rules
- `subpixel-page-size-*` — non-integer page geometry
- `root-element-display-none` — document with no rendered root

**Ground truth (verified 2026-08-18 against stylo 0.20.0
`properties/longhands.toml`):** `size` (type `PageSize`), `page-orientation`
(type `PageOrientation`), and `page` (type `PageName`) are **gecko-only** —
the servo build does NOT compile them, exactly like `break-*` and
`column-fill`. `paged.rs`'s module doc states the same: the servo build
compiles neither `@page` nor the paged-media longhands, so the layer is a
small, deterministic author-CSS pass. **All hardening in this issue happens in
that engine-owned pass (`paged.rs` + the fragmentainer build) — stylo cannot
help here.**

**Result (verified 2026-08-19):** the css-page harness suite moves
**114/215 → 126/215** — 12 tests fixed, **zero regressions** in the
previously-passing set. Fixed buckets: `page-margin-005`,
`page-name-001`, `page-name-unnamed-trailing-001`,
`page-orientation-*` ×3, `page-rule-specificity-*` ×3, `page-size-009`,
`page-size-013`, `page-size-014`. The remaining css-page failures split
into (a) tests that need inline `style=""` break/page properties through
the full cascade, (b) tests that need the css-page-3 page-name-change
forced-break semantics (Chromium's rule is subtle: flat sibling name
changes break, nested re-entry chains do not), and (c) harness-level
reference-padding cases — see Deferred below.

**Fitness function:** the css-page bucket from the initial WPT conformance baseline via the
harness, with no regression in the previously-passing tests. Satisfied:
**0 regressions, +12 fixed.**

## Goals / Non-Goals

**Goals** (one per WPT bucket)

- **Page box background** (`page-box-*`): the page box's `background` paints
  on the rendered page (background-color at minimum; background-image if the
  existing paint path supports it).
- **Margin shorthand** (`page-margin-*`): `margin: 1–4 values` expands to
  per-side margins per css-page-3; `auto` margins resolve per spec.
- **`:left`/`:right` margins**: all four margin longhands resolve per page
  pseudo (not just `margin-top`).
- **`size` overrides** (`page-size-*`): `size` accepts `px` lengths, all
  css-page size keywords (A4/A5/A3/B4/B5/JIS-B4/JIS-B5/letter/legal/ledger),
  and `landscape`/`portrait`; a `size` on a named page overrides the default
  page's size.
- **Named page sizes** (`page-name-*`): the `page: <name>` property switches
  to the named page's full geometry (size + margins).
- **Orientation** (`page-orientation-*`): `landscape` / `portrait` (and the
  css-page-3 `rotate-*` keywords the tests exercise) swap page geometry; the
  `rotate-*` values rotate the laid-out content in `pdf.rs`.
- **Rule specificity** (`page-rule-specificity-*`): overlapping `@page`
  rules resolve by the documented total order (name match > pseudo > source
  order), matching the WPT expectations.
- **Selector support**: structural pseudos (`:first-of-type`,
  `:nth-of-type(N)`, `:root`) match in the author-CSS selector pass, so
  `break-*`/`page` rules using them apply.
- **Subpixel sizes** (`subpixel-page-size-*`): page geometry stays in
  `Scalar` (f64 PostScript points) end-to-end — no integer rounding in the
  fragmentainer build.
- **Empty root** (`root-element-display-none`): a document whose root
  computes `display: none` still produces a valid (empty) page, per the WPT
  expectation — no panic, no zero-page degenerate output.
- **Harness reference padding**: the compare step pads a short reference's
  pages with its last page when `reftest-pages` names pages the reference
  lacks (wptrunner print-reftest semantics), fixing the "no pages to
  compare" class of harness failures.
- Determinism preserved throughout.

**Non-Goals** (deferred; scope stays honest)

- Full inline `style=""` cascade through stylo. Tried and reverted for
  the fixed-viewport decision: applying inline `width`/`height`/`background` (e.g. the
  `width:100%` tables and `100vw`/`100vh` divs in the fixtures) broke
  self-consistency in the monolithic-overflow/fixedpos suites whose shared
  references are authored against the previous behavior. The engine's
  hand-rolled passes still read inline `page:`, `break-*` (stylesheet only
  now), and `border-*-color`. A per-property inline path is the follow-up.
- The css-page-3 page-name-change forced break. Chromium's observed rule
  (probed 2026-08-19): a break fires before a box when the LAST page-named
  box in its subtree differs from the current page name, but nested
  re-entry chains (`a > b > c > a`) collapse to one page and empty
  intermediate pages merge. Getting this exactly right (incl. empty-page
  merging) is a follow-up; a naive nearest-ancestor rule regresses the
  page-name-siblings/propagated suites.
- New paged-media features beyond the buckets: `:blank`, `bleed`, `marks`,
  `page`-context `content` extensions, crop/registration marks.
- `@page` selector support beyond the current `:first`/`:left`/`:right`
  subset.
- Margin-box layout changes (positioning/geometry of margin boxes beyond the
  current fixed 10pt text context) — only what the failing tests require.
- The stylo viewport stays at the engine's historical 1024×768 (the
  paged-media content-box viewport was tried — it fixes `page-size-009`'s
  `100vw`/`100vh` div but regresses the monolithic-overflow/fixedpos
  suites whose references assume the fixed viewport). `vw`/`vh`
  correctness is tracked separately.

## Behavior

The engine shall:

1. Parse the `margin` shorthand inside `@page` rules in the `paged.rs`
   author-CSS pass: 1–4 values expand to per-side margins per css-page-3
   (1 → all; 2 → vertical/horizontal; 3 → top/horizontal/bottom; 4 →
   top/right/bottom/left); `auto` margins resolve per spec, including
   `%`/`em`/`inherit` length forms.
2. Resolve all four margin longhands per page pseudo: `:left`/`:right`
   (and `:first`) margins apply to the pages they select, exactly like
   `margin-top` already does.
3. Extend `size` parsing: accept `px` lengths (and `pt`/other absolute
   units already handled) plus the css-page size keywords (`A5`, `A4`,
   `A3`, `B5`, `B4`, `JIS-B5`, `JIS-B4`, `letter`, `legal`, `ledger`) and
   `landscape` / `portrait` orientation keywords. Keyword sizes are computed
   with the same mm→pt formula as `parse_length` (`mm * 72 / 25.4`) so
   `size: a5` is byte-identical to `size: 148mm 210mm`; a named page's
   `size` overrides the default page's size when `page: <name>` is in
   effect.
4. Apply orientation: `landscape` / `portrait` swap the page geometry
   (width ↔ height) before margins and content-box resolution; `rotate-*`
   values set the fragmentainer's orientation, which `pdf.rs` applies as a
   90° content rotation (krilla `push_transform`/`pop`).
5. Resolve overlapping `@page` rules by the documented total order — name
   match, then pseudo, then source order — and match the
   `page-rule-specificity-*` expectations (no specificity weighting beyond
   that order; verified against the tests).
6. Match structural pseudos in the author-CSS selector pass:
   `:first-of-type`, `:nth-of-type(N)`, and `:root` (css-selectors-4
   subset), so stylesheet `break-*`/`page` rules using them apply.
7. Keep page geometry in `Scalar` (f64 pt) end-to-end: `px` sizes convert
   exactly, subpixel sizes survive the fragmentainer build without integer
   rounding.
8. Handle `display: none` on the root element: produce a valid empty page
   (per the WPT bucket) with no panic and no degenerate zero-page output.
9. Paint the page box background (`page-box-*`) from the resolved `PageSpec`
   in `pdf.rs`; the fragmentainer carries the resolved background and
   orientation.
10. Resolve the page name at the top of a page from a fresh box's effective
    `page` even when that box is a table (`active_page_name` descends table
    items as well as blocks).
11. Compare short references correctly: when `reftest-pages` names pages the
    reference lacks, pad the reference's page list with its LAST page
    (wptrunner print-reftest semantics).
12. Stay deterministic: rule resolution remains a total order over
    (name, pseudo, source order); no new nondeterminism sources.

## Interfaces

### `harness/compare.py`

- Reference-page padding: when the `reftest-pages` selection names pages
  beyond the reference's page count, pad with the reference's last page
  (implemented at the image-list level, since `_select_pages` drops
  out-of-range pages before comparison). Unit tests in
  `tests/test_compare.py`.

### `engine/src/paged.rs`

- `PageRule` parsing: `margin` shorthand (1–4 values) with `auto`/`%`/`em`/
  `inherit`; `size` keywords + `px` lengths + orientation keywords
  (`SizeDecl::{Abs, Portrait, Landscape}`); `page-orientation` (`rotate-*`);
  `width`/`height` page-box overrides; `background` (via
  `css::parse_css_color`).
- `resolve_page_spec`: auto-margin centering (equal split for both-auto,
  end-margin absorption when overconstrained) and page-area width/height
  overrides; orientation handled with explicit f64 min/max helpers (Scalar
  has no `Ord`).

### `engine/src/css.rs`

- `PseudoClass` enum (`FirstOfType`, `NthOfType(n)`, `Root`) in the
  author-CSS simple-selector matcher (used by the breaks/paged_props/borders
  passes).
- `parse_css_color` (hex + named, incl. the CSS basic color set) shared by
  the `@page` background pass.
- `border-*-color` longhands in the borders pass (used by the orientation
  notrefs).
- No stylo inline-style wiring (tried, reverted — see Non-Goals).

### `engine/src/layout.rs` (fragmentainer build)

- The fragmentainer's `PageGeometry` uses the resolved `PageSpec` with
  orientation applied (width/height swap) before margins/content box.
- The empty-root path: when the root computes `display: none`, still build
  one empty page fragmentainer from the resolved page spec (no crash).
- `active_page_name` descends table items so a table's `page: <name>` can
  select the named page's geometry.

### `engine/src/pdf.rs`

- Paints the page box background from the resolved `PageSpec` (background
  color) before drawing content.
- Applies the fragmentainer's `page-orientation` as a whole-page rotation
  (krilla `Surface::push_transform` with a balanced `pop` before
  `finish()`). A 90° rotation (`rotate-left`/`rotate-right`) also SWAPS the
  page box dimensions (`PageSettings::from_wh`), so the rotated content
  stays inside the box; a 180° rotation keeps the box.

## Acceptance Criteria

Each criterion maps to the named WPT bucket via the harness (initial WPT conformance
baseline) and/or unit tests in `engine/tests/paged_media.rs` and
`tests/test_compare.py`.

1. **`page-box-*`**: page box background renders on the page (unit test
   asserting the background is painted + the bucket passes via the harness).
2. **`page-margin-*`**: `margin: 1–4 values` expands correctly; `auto`
   margins resolve per css-page-3; `:left`/`:right` margin differences apply
   (all four sides). `page-margin-005` passes.
3. **`page-size-*`**: `size: 210px 297px` (and the tested override forms)
   produces the expected page box; `page-size-009/011/012/013/014` pass.
4. **`page-name-*`**: a named page's `size` applies when `page: <name>` is in
   effect; `page-name-001` and `page-name-unnamed-trailing-001` pass.
5. **`page-orientation-*`**: `landscape`/`portrait` swap page geometry
   correctly; the `page-orientation-*` bucket passes (3 tests fixed).
6. **`page-rule-specificity-*`**: overlapping `@page` rules resolve per the
   documented total order; the bucket passes (3 tests fixed).
7. **`subpixel-page-size-*`**: subpixel page sizes survive layout without
   rounding (assert geometry in `Scalar`); no regression in the bucket.
8. **`root-element-display-none`**: a doc with `display: none` root produces
   a valid empty page, no panic; no regression.
9. **No regression**: all previously-passing harness tests stay green
   (verified: 0 regressions across the css-page suite; full-suite run
   pending at time of writing); the report + invoice demos still render
   (`bash scripts/build-demo.sh`).
10. **Determinism**: two renders of a paged-media doc are byte-identical.

## Edge Cases

- `margin: 0 auto` (and other auto combos): per-side resolution per
  css-page-3; no panic on any 1–4 value count.
- `size` with only one dimension (e.g. `size: landscape`): the keyword
  implies the pair; a single length uses the default for the other axis.
- Keyword-vs-mm equivalence: `size: a5` must equal `size: 148mm 210mm`
  exactly (same mm→pt formula) — the WPT references spell keywords out as
  mm, so rounding differences show up as pixel diffs.
- Orientation + named page: the named page's orientation applies with its
  size.
- Subpixel margins + subpixel size: both survive in `Scalar`; margin-box
  geometry follows the same arithmetic.
- Root `display: none` with an explicit `@page` background: the single empty
  page still paints the background.
- A page name on a table (`page: square`): the named page's geometry applies
  at page start.
- Conflicting rules with equal name/pseudo: source order decides (already the
  documented rule; keep it).

## Deferred (documented follow-ups)

- Inline `style=""` through the full cascade (width/height/background) —
  reverted; hand-rolled passes keep inline `page:` and `border-*-color`.
- css-page-3 page-name-change forced break with Chromium's exact semantics
  (subtree-last-name rule + empty-page merging).
- Paged-media viewport for `vw`/`vh` (content-box viewport reverted to the
  historical 1024×768).

## References

- css-page-3 (page box, margins, size, orientation):
  https://drafts.csswg.org/css-page-3/
- Base `@page` layer this hardens: `paged-media-css.spec.md`
- Baseline: 46 css-page failures and 164 passing in the recorded harness run
- `engine/src/paged.rs` module doc (author-CSS pass; servo build compiles
  neither `@page` nor the paged-media longhands)
- stylo 0.20.0 `properties/longhands.toml` (verified 2026-08-18): `size`,
  `page`, `page-orientation` are gecko-only in the servo build.
- Chromium page-name-break probes (2026-08-19): minimal fixtures under
  `/tmp/probe_*.html` documenting the subtree-last-name rule.
