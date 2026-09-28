---
title: "Inline Base Direction (direction: rtl)"
slug: /specifications/inline-base-direction
type: spec
status: in-review
owner: maintainers
created: 2026-09-12
updated: 2026-09-27
sidebar_position: 46
tags: [engine, css, writing-modes, rtl, stylo]
spec_id: inline-base-direction
applies_to: engine 0.x
dependencies: [fragmentation-core, paged-media-css]
---

# Inline Base Direction (direction: rtl)

## Overview

The engine had no `direction` support: rtl text laid out LTR and
`text-align: start` always mapped to the left edge. WPT
`page-left-right-002` cannot pass without it — its reference simulates
rtl block placement with per-div margins, but the test side requires
real rtl (the fixture sets `direction: rtl` on `:root` and uses
`@page :left`/`:right` margin asymmetry).

This spec covers **base-direction resolution only** (css-writing-modes-1
§1.2, css-logical-1). The full CSS bidirectional algorithm (reordering of
mixed-direction inline content, `unicode-bidi: embed/override/plaintext`)
is a recorded non-goal.

## Goals

1. Parse and cascade `direction: ltr | rtl` through stylo to
   `ComputedStyle` (inherited property, initial `ltr`).
2. `text-align: start`/`end` resolve through the element's own inline base
   direction (css-writing-modes-1 §2.2).
3. Block-level boxes anchor at the inline-END edge under rtl
   (css2.1 §10.3.3 resolution mirrored): under-constrained leftover space
   lands in the inline-start (left) margin; over-constrained resolution
   drops the start margin; a lone `margin-left: auto` pushes the box to the
   right edge.
4. The `:left`/`:right` page-progression flips under root rtl
   (css-page-3 §4.1): page 1 is a `:left` page.

## Non-Goals

- The CSS bidirectional (bidi) algorithm for mixed-direction runs: an RTL
  string inside an LTR paragraph keeps its glyph order; only base
  alignment/placement flips. Bidi reordering is tracked for a future issue
  (needs HarfRust run splitting).
- `unicode-bidi: plaintext/embed/override`.
- Vertical writing modes (`writing-mode: vertical-*`): logical block-axis
  properties (`margin-block-*` etc.) still assume horizontal-tb.
- Logical longhands in the ELEMENT context (`margin-inline-start` on
  regular boxes): the page context maps them (paged.rs), and the
  fixture suite only exercises page-level logical props; element-level
  logical longhands stay a known gap.

## Behavior

1. `direction` parses via stylo's servo build; `ComputedStyle` carries
   `rtl: bool` (false = ltr). It is inherited (stylo's inherited-box
   struct).
2. `text-align: start` renders flush to the LEFT edge under ltr and the
   RIGHT edge under rtl; `text-align: end` mirrors. `left`/`right` stay
   physical.
3. A block-level box whose resolved width leaves free space anchors at
   `origin + avail − margin_right − width` under rtl (over-constrained
   case included — the left margin computes to 0 and the box pulls to the
   end edge subject to the honored end margin; oracle-verified: a
   `width:100px; margin-right:500px` box under rtl on a 600px page sits
   at the LEFT edge, a plain `width:100px` box hugs the RIGHT edge).
4. `@page :left` matches odd 1-based page indices under root rtl (page 1
   is a `:left` page); `:right` matches even. Without rtl the parity is
   the historical one. `:first` is direction-independent.

## Interfaces

- `ComputedStyle.rtl: bool` (css.rs) — read in `convert` from
  `values.get_inherited_box().clone_direction()`.
- `Ctx::frag_border_x_rtl(style, origin_x, avail_width, width)` (layout.rs)
  — the end-anchored border-box x under rtl.
- `Ctx::aligned_x` reads `style.rtl` for Start/End resolution.
- `resolve_page_spec(rules, page_name, global_index, cli, inherit_margins,
  rtl_progression)` (paged.rs) — new trailing flag; `pseudo_matches` gains
  the same parameter.

## Acceptance Criteria

- **AC1 — rtl text-align start is right-flushed.** A minimal rtl div's
  text renders flush right. Verified by Chromium oracle probe
  (`/tmp/core166-min-probe.py`, 2026-09-12): start-aligned line ends at
  the right content edge.
- **AC2 — page-left-right-002 passes.** Harness filter
  `page-left-right-002`: FAIL → PASS after the fix (0.0% → 100.0%).
- **AC3 — zero WPT regressions.** Full-suite A/B against the release-tip
  binary: no status flips. Record the result with the release verification.
- **AC4 — parity unit tests.** `engine/src/paged.rs::pseudo_parity`
  covers both progressions including `:first` invariance.

## Edge Cases

- An rtl document with NO explicit `direction` on root: unchanged
  behavior (`rtl: false`).
- `margin-left: auto` under rtl: stylo computes auto → 0pt in
  `lp_or_auto_to_pt`, and the end-anchor formula puts the box at the right
  edge — matching Chromium (oracle-verified).
- Page rules where BOTH `:left` and `:right` declare the same property:
  parity flip changes WHICH rule wins per page, not the cascade order.

## References

- css-writing-modes-1 §1.2, §2.2 (inline base direction, logical
  directions)
- css2.1 §10.3.3 (block-level, non-replaced normal-flow margin
  resolution)
- css-page-3 §4.1 (page progression, `:left`/`:right`)
- Chromium oracle probes + WPT `page-left-right-002-print` (2026-09-12)
- Named pages: `paged-media-css.spec.md`.
