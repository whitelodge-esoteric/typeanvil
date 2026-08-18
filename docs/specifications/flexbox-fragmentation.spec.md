---
title: Flexbox Fragmentation
slug: /specifications/flexbox-fragmentation
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-08-18
sidebar_position: 10
tags: [engine, layout, css-flexbox, css-break, fragmentation]
spec_id: flexbox-fragmentation
issue_id: CORE-65
applies_to: engine 0.x
dependencies: [fragmentation-core, wpt-conformance-harness]
---

# Flexbox Fragmentation

## Overview

The CORE-60 baseline (2026-08-17) showed 12 failing flexbox fragmentation
print-reftests — the largest css-break gap outside the CORE-54 children —
because the engine has **no flex layout at all**. Block layout is
fragmentation-first (CORE-51) but handles only block and inline boxes.

Flex adds the research brief's hard case: **two-pass modes × fragmentation**.
The flex algorithm (taffy is the reference implementation) needs a measure
pass (cross-axis sizing, `stretch`) before layout, and fragmentation must
resume that state across pages. Flex *wrapping* is explicitly not viable here
— it needs inline/fragmentation machinery the engine does not have.

**Path chosen: stylo + taffy model.** Verified 2026-08-18 against stylo 0.20.0:

- `display: flex` / `inline-flex` parse in the servo build
  (`values/specified/box.rs` maps `"flex"` → `DisplayInside::Flex`).
- `flex-direction`, `flex-wrap`, `flex-basis`, `flex-grow`, `flex-shrink`,
  `align-content`, `align-items`, `align-self`, `justify-content`,
  `justify-items`, `justify-self`, `order`, `row-gap`, `column-gap` — all
  first-class longhands, NOT gecko-gated and NOT pref-gated; the servo build
  compiles them. Cascade, specificity, and `!important` come from stylo; no
  manual author-CSS pass needed.

The layout algorithm follows taffy's flex model (grow/shrink/basis
resolution), adapted to the fragment tree and the two-pass-measure-before-
fragmentation rule below.

**Fitness function:** the 12 named css-break flexbox print-reftests via the
harness, growing from there.

## Goals / Non-Goals

**Goals**

- `display: flex` (block-level) and `inline-flex` (treated as a block-level
  flex container in paged flow) create a flex container whose children are
  flex items.
- Main axis from `flex-direction` (`row` | `row-reverse` | `column` |
  `column-reverse`); cross axis perpendicular. Basic `flex-wrap: nowrap`
  only.
- Flex sizing: `flex-grow` / `flex-shrink` / `flex-basis` resolution —
  `flex: 1` ⇒ grow 1, shrink 1, basis 0%; the resolution algorithm follows
  taffy's reference model.
- Cross-axis sizing incl. `align-items/align-self: stretch`, resolved in a
  **measure pass before fragmentation** (the two-pass rule).
- Gaps: `row-gap` / `column-gap` between items.
- **Fragmentation**: the flex container and its items fragment across pages
  via break tokens; `break-inside: avoid` on an item moves the whole item;
  a single item line never slices (CORE-51 monolithic rule); the container
  resumes deterministically.
- Determinism: flex resolution is a pure function of measure; identical input
  → byte-identical PDF.

**Non-Goals** (deferred; scope stays honest)

- `flex-wrap: wrap` — needs inline/fragmentation machinery the engine does
  not have (explicitly excluded by the issue).
- `order` beyond computed (layout order = document order until `order` lands).
- Advanced alignment (`align-content: space-between` etc.), `justify-content`
  distribution beyond basic start/center/end/space-between.
- Nested flex; flex inside floats (CORE-62), multicol (CORE-63), or abspos
  (CORE-64) — interaction specs land after the individual features.
- `min-width: auto` intrinsic minimums beyond the basic content-based rule.

## Behavior

The engine shall:

1. Compute the flex container's properties at cascade time from stylo:
   `ComputedStyle` gains `flex_direction`, `flex_wrap`, `flex_grow`,
   `flex_shrink`, `flex_basis`, `align_items`, `align_self`,
   `justify_content`, `order`, `row_gap`, `column_gap`, read in
   `css.rs::convert` (no manual author-CSS pass).
2. Create a flex container for `display: flex` / `inline-flex`: children
   become flex items (in-flow block-level children; anonymous-item wrapping
   deferred with flex-wrap).
3. Lay out the flex line: resolve main-axis sizes with the
   grow/shrink/basis algorithm (taffy reference), placing items along the
   main axis with gaps; cross-axis sizes per `align-items`/`align-self`
   (default `stretch`).
4. **Run the cross-axis measure pass before fragmentation**: when
   `align-items/align-self: stretch` (the default) is in effect, measure all
   items' cross sizes first, fix the largest as the line's cross size, then
   fragment — the two-pass rule from the research brief.
5. Fragment the container: the container produces fragments per page via
   break tokens; item fragments nest in the container's token tree;
   `break-inside: avoid` on an item moves the whole item to the next
   fragmentainer.
6. Never slice an item line: a single item (or item line) taller than the
   fragmentainer overflows it (CORE-51 monolithic rule), with last-resort
   breakpoints placing it.
7. Resume deterministically: the flex line state (item order, accumulated
   sizes) is carried in the break token; page N+1 resumes without relayout of
   page N's items.
8. Keep the CLI contract and determinism guarantees unchanged.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle`:

  ```rust
  /// Flex container properties (stylo computed; struct = position).
  pub flex_direction: FlexDirection, // Row | RowReverse | Column | ColumnReverse
  pub flex_wrap: FlexWrap,           // Nowrap | Wrap | WrapReverse (computed)
  pub flex_grow: f64,                // NonNegativeNumber
  pub flex_shrink: f64,              // NonNegativeNumber
  pub flex_basis: FlexBasis,         // Auto | Content | Size (stylo)
  pub align_items: ItemPlacement,    // stylo computed
  pub align_self: SelfAlignment,     // stylo computed
  pub justify_content: ContentDistribution,
  pub order: i32,
  pub row_gap: Scalar,               // default 0
  pub column_gap: Scalar,            // default 0
  ```

- In `convert`, read the corresponding `clone_*()` accessors from stylo's
  computed structs. `ComputedStyle::initial()` mirrors stylo's initial
  values (grow 0, shrink 1, basis auto, gaps 0).

### `engine/src/layout.rs` / new `engine/src/flex.rs`

- `flex.rs` implements the flex layout algorithm (taffy reference): main/cross
  axis resolution, grow/shrink/basis, the cross-axis measure pass, and item
  placement with gaps.
- `layout.rs` dispatches to `flex.rs` when a box computes
  `display: flex | inline-flex`; otherwise the block path is unchanged.
- Fragmentation integration: flex items lay out with break tokens exactly
  like block children (the fragment tree's existing resume machinery); the
  container's token carries the flex line state (behavior 7).

### `engine/src/frag.rs`

- No new fragment kind required: flex containers and items are ordinary
  fragments; the flex line state rides in the container's break token.

### `engine/src/pdf.rs`

- Paint flex fragments in tree order (in-flow pass; no reordering until
  `order` lands).

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/flex.rs` (helpers mirror
`tests/typography.rs`). The named WPT tests run via the harness.

1. **Row split.** Given a row flex container with two `flex: 1` items, the
   items share the container's inline size equally (minus gaps).
2. **Column stack.** Given a column flex container with `gap: 10pt`, items
   stack on the cross axis and the container's height equals the sum of item
   heights plus gaps.
3. **Stretch two-pass.** Given `align-items: stretch` (default) with one
   taller item, all items' cross size equals the tallest, resolved before
   fragmentation.
4. **Fragment across pages.** Given a flex container taller than a page, the
   container fragments; a single item line never slices;
   `break-inside: avoid` on an item moves the whole item to the next page.
5. **WPT targets.** The 12 failing css-break flexbox print-reftests pass via
   the harness: `multi-line-row-flex-fragmentation-064/081/082`,
   `single-line-column-flex-fragmentation-068c/d`,
   `single-line-row-flex-fragmentation-042` (full list from the CORE-60
   baseline).
6. **Determinism.** Two renders of a flex doc are byte-identical; the full
   existing engine test suite still passes.

## Edge Cases

- Zero flex items: empty container, no crash.
- Single item with `flex: 1`: fills the main axis.
- Item taller than the fragmentainer: monolithic overflow (behavior 6).
- `flex-basis: auto` vs `0%`: basis auto uses the item's content size;
  `flex: 1` uses 0% — both resolve per taffy.
- `row-reverse` / `column-reverse`: item order flips along the main axis
  (placement only; `order` values beyond document order deferred).
- Gaps larger than the container: items shrink to minimum content size.
- `inline-flex` inside a paragraph: treated as a block-level flex container in
  paged flow (per goal 1; true inline-fragment behavior deferred).

## References

- css-flexbox-1: https://drafts.csswg.org/css-flexbox-1/
- css-break-3 (fragmentation of flex containers):
  https://drafts.csswg.org/css-break-3/
- taffy (flex algorithm reference):
  https://github.com/DioxusLabs/taffy
- Research brief (two-pass modes × fragmentation hard case):
  `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
- Fragment tree this builds on: `fragmentation-core.spec.md`
- CORE-60 baseline (12 failing flexbox print-reftests): Linear CORE-60
- stylo 0.20.0 `properties/longhands.toml` + `values/specified/box.rs`
  (verified 2026-08-18): all flex longhands compiled in the servo build;
  `display: flex`/`inline-flex` parse.
