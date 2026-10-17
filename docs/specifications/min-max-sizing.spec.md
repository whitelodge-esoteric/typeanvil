---
title: "Min/Max Sizing — min-width, max-width, min-height, max-height"
slug: /specifications/min-max-sizing
type: spec
status: draft
owner: maintainers
created: 2026-10-16
updated: 2026-10-16
sidebar_position: 40
tags: [engine, layout, sizing, css2]
spec_id: min-max-sizing
issue_id: CORE-388
applies_to: engine 0.x
dependencies: [images]
---

# Min/Max Sizing

## Overview

Before CORE-388, `ComputedStyle` had no field for `min-width`, `max-width`,
`min-height` or `max-height`. The engine dropped every such declaration. The
weasyprint poster showed it: `#sponsors img { max-height: 60px }` was ignored
and the logos rendered at natural size.

## Goals

1. Model the four properties on `ComputedStyle` as `Option<SizeLimit>`
   (a length in pt, or a percentage fraction).
2. Clamp block widths, block heights and replaced-element sizes as CSS2.1
   §10.4 and §10.7 describe.
3. Flex items take their clamped size through the same box layout.

## Non-Goals

1. Percentage `min-height` / `max-height` resolve to none (as `height`
   percentages do against an indefinite chain).
2. Flex base-size and hypothetical-main-size clamping inside the flex
   algorithm is not part of this change. A flex item's own `max-width` and
   `max-height` clamp its box when it is laid out (see Behavior 6). Clamped
   sizes do not feed the flex line-packing computation.
3. A flex item margin-box width hand-off was tried and removed. It changed
   `css-page/page-margin-auto-and-non-zero-print` from PASS to FAIL
   (ref items widened from 25..214 to 25..239 px). It stays a follow-up.

## Behavior

1. A non-replaced block's used border-box width SHALL be clamped by
   `max-width`, then by `min-width`. `min-width` wins over `max-width`.
2. `box-sizing: content-box` limits SHALL add padding and border before
   the clamp. `border-box` limits apply as given.
3. Width percentages SHALL resolve against the containing block width.
4. A replaced `<img>` SHALL follow the CSS2.1 §10.4 constraint-violation
   table and keep its aspect ratio where the table says so
   (see `images.spec.md`, Behavior 6).
5. The used width SHALL never resolve below zero.
6. A fresh block box's used border-box height SHALL be clamped by
   `max-height`, then by `min-height` (CSS2.1 §10.7). `min-height` wins.
   Resumed fragments are not clamped again.

## Acceptance Criteria

| # | Criterion | Test |
|---|-----------|------|
| 1 | `max-height: 60px` on a 483×557 px image gives 45 pt height, ratio kept | `img_max_height_and_width_clamp_keep_ratio` |
| 2 | `min-width: 200px` grows a 96×48 px image to 150×75 pt | `img_min_width_grows_keep_ratio` |
| 3 | `max-width: 100px` clamps a block to 75 pt | `block_max_width_clamps` |
| 4 | `min-width` beats `width` | `block_min_width_wins_over_width` |
| 5 | `min-height: 60px` grows an empty block to 45 pt; `max-height: 40px` caps a 200px block to 30 pt | `block_min_height_grows_and_max_height_caps_declared` |
| 6 | Four flex items with `max-width: 100px; max-height: 40px` in a wrapping row are each 75×30 pt | `flex_items_in_wrapping_row_respect_max_width_and_height` |

## References

- CSS2.1 §10.4 (min/max width), §10.7 (min/max height).
- `docs/specifications/images.spec.md`.
- CORE-388, CORE-456 (poster page fit follow-up).
