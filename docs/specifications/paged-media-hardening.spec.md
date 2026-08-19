---
title: Paged-Media Hardening
slug: /specifications/paged-media-hardening
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-08-18
sidebar_position: 11
tags: [engine, css, paged-media, page]
spec_id: paged-media-hardening
issue_id: CORE-66
applies_to: engine 0.x
dependencies: [paged-media-css, wpt-conformance-harness]
---

# Paged-Media Hardening

## Overview

CORE-52 implemented the `@page` layer — size, margins, margin boxes, named
pages, `:first`/`:left`/`:right` — well enough for the demo documents. The
CORE-60 baseline (2026-08-17) shows the layer is **partial**: 46 css-page
print-reftest failures cluster in:

- `page-box-*` — page box background painting
- `page-margin-*` — margin shorthand (1–4 values), auto margins, `:left` /
  `:right` margin differences (CORE-52 tested only `margin-top` for
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

**Fitness function:** the 46-test bucket from the CORE-60 baseline via the
harness, with no regression in the 164 currently-passing tests.

## Goals / Non-Goals

**Goals** (one per WPT bucket)

- **Page box background** (`page-box-*`): the page box's `background` paints
  on the rendered page (background-color at minimum; background-image if the
  existing paint path supports it).
- **Margin shorthand** (`page-margin-*`): `margin: 1–4 values` expands to
  per-side margins per css-page-3; `auto` margins resolve per spec.
- **`:left`/`:right` margins**: all four margin longhands resolve per page
  pseudo (not just `margin-top`).
- **`size` overrides** (`page-size-*`): `size` accepts `px` lengths and all
  css-page size keywords; a `size` on a named page overrides the default
  page's size.
- **Named page sizes** (`page-name-*`): the `page: <name>` property switches
  to the named page's full geometry (size + margins).
- **Orientation** (`page-orientation-*`): `landscape` / `portrait` (and the
  css-page-3 `rotate-*` keywords the tests exercise) swap page geometry.
- **Rule specificity** (`page-rule-specificity-*`): overlapping `@page`
  rules resolve by the documented total order (name match > pseudo > source
  order), matching the WPT expectations.
- **Subpixel sizes** (`subpixel-page-size-*`): page geometry stays in
  `Scalar` (f64 PostScript points) end-to-end — no integer rounding in the
  fragmentainer build.
- **Empty root** (`root-element-display-none`): a document whose root
  computes `display: none` still produces a valid (empty) page, per the WPT
  expectation — no panic, no zero-page degenerate output.
- Determinism preserved throughout.

**Non-Goals** (deferred; scope stays honest)

- New paged-media features beyond the buckets: `:blank`, `bleed`, `marks`,
  `page`-context `content` extensions, crop/registration marks.
- `@page` selector support beyond the current `:first`/`:left`/`:right`
  subset.
- Margin-box layout changes (positioning/geometry of margin boxes beyond the
  current fixed 10pt text context) — only what the failing tests require.

## Behavior

The engine shall:

1. Parse the `margin` shorthand inside `@page` rules in the `paged.rs`
   author-CSS pass: 1–4 values expand to per-side margins per css-page-3
   (1 → all; 2 → vertical/horizontal; 3 → top/horizontal/bottom; 4 →
   top/right/bottom/left); `auto` margins resolve per spec (equal split for
   the auto sides, zero when both auto on an axis is not the test's
   expectation — resolve per css-page-3).
2. Resolve all four margin longhands per page pseudo: `:left`/`:right`
   (and `:first`) margins apply to the pages they select, exactly like
   `margin-top` already does.
3. Extend `size` parsing: accept `px` lengths (and `pt`/other absolute
   units already handled) plus the css-page size keywords
   (`A4`, `letter`, …) and `landscape` / `portrait` orientation keywords;
   a named page's `size` overrides the default page's size when `page:
   <name>` is in effect.
4. Apply orientation: `landscape` / `portrait` (and `rotate-*`) swap the page
   geometry (width ↔ height) before margins and content-box resolution.
5. Resolve overlapping `@page` rules by the documented total order — name
   match, then pseudo, then source order — and match the
   `page-rule-specificity-*` expectations (no specificity weighting beyond
   that order; verify against the tests).
6. Keep page geometry in `Scalar` (f64 pt) end-to-end: `px` sizes convert
   exactly, subpixel sizes survive the fragmentainer build without integer
   rounding.
7. Handle `display: none` on the root element: produce a valid empty page
   (per the WPT bucket) with no panic and no degenerate zero-page output.
8. Paint the page box background (`page-box-*`) from the resolved `PageSpec`
   in `pdf.rs`.
9. Stay deterministic: rule resolution remains a total order over
   (name, pseudo, source order); no new nondeterminism sources.

## Interfaces

### `engine/src/paged.rs`

- Extend `PageRule` parsing: `margin` shorthand (1–4 values) with `auto`;
  `size` keywords + `px` lengths + orientation keywords; `page-orientation`
  where the tests use it.
- Extend `resolve_page_spec` (or the per-pseudo margin resolution) so all
  four sides resolve per pseudo, not just `margin-top`.
- Keep the documented total order for rule selection; adjust only where the
  `page-rule-specificity-*` tests demand a correction.

### `engine/src/layout.rs` (fragmentainer build)

- The fragmentainer's `PageGeometry` uses the resolved `PageSpec` with
  orientation applied (width/height swap) before margins/content box.
- The empty-root path: when the root computes `display: none`, still build one
  empty page fragmentainer from the resolved page spec (no crash).

### `engine/src/pdf.rs`

- Paint the page box background from the resolved `PageSpec` (background
  color; background image only if the existing paint path supports it —
  otherwise the `page-box-*` tests that need it define the requirement).

### `engine/src/css.rs`

- No stylo work: `size`/`page`/`page-orientation` are gecko-only (verified),
  and the engine's own pass already owns the `@page` layer. Only changes
  needed if a bucket reveals a missing longhand that IS compiled in the servo
  build (e.g. `page`-context properties — check per test).

## Acceptance Criteria

Each criterion maps to the named WPT bucket via the harness (CORE-60
baseline) and/or unit tests in `engine/tests/paged_media.rs`.

1. **`page-box-*`**: page box background renders on the page (unit test
   asserting the background is painted + the bucket passes via the harness).
2. **`page-margin-*`**: `margin: 1–4 values` expands correctly; `auto`
   margins resolve per css-page-3; `:left`/`:right` margin differences apply
   (all four sides).
3. **`page-size-*`**: `size: 210px 297px` (and the tested override forms)
   produces the expected page box; `page-size-006/009/013/014` pass.
4. **`page-name-*`**: a named page's `size` applies when `page: <name>` is in
   effect; the bucket passes.
5. **`page-orientation-*`**: `landscape`/`portrait` swap page geometry
   correctly; the bucket passes.
6. **`page-rule-specificity-*`**: overlapping `@page` rules resolve per the
   documented total order; the bucket passes.
7. **`subpixel-page-size-*`**: subpixel page sizes survive layout without
   rounding (assert geometry in `Scalar`); the bucket passes.
8. **`root-element-display-none`**: a doc with `display: none` root produces a
   valid empty page, no panic; the test passes.
9. **No regression**: the 164 currently-passing tests stay green; the report
   + invoice demos still render (`bash scripts/build-demo.sh`).
10. **Determinism**: two renders of a paged-media doc are byte-identical.

## Edge Cases

- `margin: 0 auto` (and other auto combos): per-side resolution per
  css-page-3; no panic on any 1–4 value count.
- `size` with only one dimension (e.g. `size: landscape`): the keyword
  implies the pair; a single length uses the default for the other axis.
- Orientation + named page: the named page's orientation applies with its
  size.
- Subpixel margins + subpixel size: both survive in `Scalar`; margin-box
  geometry follows the same arithmetic.
- Root `display: none` with an explicit `@page` background: the single empty
  page still paints the background.
- Conflicting rules with equal name/pseudo: source order decides (already the
  documented rule; keep it).

## References

- css-page-3 (page box, margins, size, orientation):
  https://drafts.csswg.org/css-page-3/
- Base `@page` layer this hardens: `paged-media-css.spec.md` (CORE-52)
- CORE-60 baseline (46 css-page failures; 164 passing): Linear CORE-60
- `engine/src/paged.rs` module doc (author-CSS pass; servo build compiles
  neither `@page` nor the paged-media longhands)
- stylo 0.20.0 `properties/longhands.toml` (verified 2026-08-18): `size`,
  `page`, `page-orientation` are gecko-only in the servo build.
