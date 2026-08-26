---
title: Inline-block atomic boxes
slug: /specifications/inline-block
type: spec
status: approved
owner: Elijah Boston
created: 2026-08-25
updated: 2026-08-25
sidebar_position: 24
tags: [css-display-3, layout, core-120]
spec_id: SPEC-inline-block
issue_id: CORE-120
applies_to: engine/src/layout.rs, engine/src/css.rs
dependencies:
  - SPEC-fragmentation-core
---

# Inline-block atomic boxes (CORE-120)

## Overview

`display: inline-block` is an outer display that makes the box inline-level
while its children lay out with block flow (css-display-3 §2.3 / CSS 2.1
§9.2.4). The engine currently has no `InlineBlock` display variant: stylo
resolves `inline-block` and the element folds into the parent's text run,
so its border, padding, background, and box geometry never paint. Eight WPT
flexbox rows (081a-d / 082a-d) use inline-block references and fail only on
this residual.

## Goals

- Atomic inline-level boxes that paint their own border/padding/background.
- Consecutive inline-block siblings flow side by side on one line.
- Baseline alignment with surrounding inline text.

## Non-Goals

- Inline fragmentation of an over-tall inline-block across pages (atomic
  monolithic placement; defer whole box).
- `display: inline` blockification rules beyond what stylo already computes.
- Inline-table / inline-list-item variants.

## Behavior

1. `ComputedStyle` shall expose `Display::InlineBlock` when the author sets
   `display: inline-block` (stylo `DisplayOutside::Inline` +
   `DisplayInside::FlowRoot`).
2. `collect_items_rec` shall emit an inline-block child as its own item so
   it reaches the block layout path (never folded into the parent text run).
3. An inline-block shall be laid out through the block path at its resolved
   width (`width`, `width_percent`, or shrink-to-fit from min/max content),
   honoring border, padding, background, and margin.
4. Consecutive inline-block items with room remaining on the current line
   shall place horizontally next to each other; an item that does not fit
   wraps to a new line.
5. An inline-block whose height exceeds the remaining page space shall be
   deferred whole to the next fragmentainer (no mid-box split).
6. The inline-block's baseline shall be the baseline of its last line box
   (css-display-3 §2.3); a box with no line boxes aligns its bottom margin
   edge with the text baseline.
7. Surrounding text lines shall wrap around placed inline-blocks using the
   same intrusion mechanism as floats.

## Interfaces

- `css.rs`: add `Display::InlineBlock`; map stylo
  `(Inline, FlowRoot)` → `InlineBlock`.
- `layout.rs`: new `Item::Atomic(child)` (or reuse `Item::Block` with an
  inline-flow flag) handled in the bare-text/line placement loop;
  `place_atomic_box` helper returns the advance width and baseline offset.

## Acceptance Criteria

- Given a paragraph with two 50%-wide bordered inline-block divs, when
  rendered, then both paint their borders side by side on one line.
- Given an inline-block taller than the remaining page space, when rendered,
  then the box appears whole on the following page.
- Unit test: an inline-block child paints its border as a box (pixel scan or
  fragment-tree assertion), not folded into the parent text run.
- Harness gate: css-break flexbox bucket fixed − regressed ≥ 0, targeting
  rows 081a-d / 082a-d.

## Edge Cases

- `width` exceeding the line: box takes full width alone.
- Nested inline-block inside inline-block: inner lays out via the same path.
- `break-before`/`break-after` on an inline-block propagate like other
  atomic boxes (already parsed by the breaks pass).
- Whitespace between inline-blocks collapses per normal inline whitespace
  processing.

## References

- css-display-3 §2.3; CSS 2.1 §9.2.4, §10.3.9 (shrink-to-fit width).
- Linear CORE-120; classification table in
  `flexbox-fragmentation.spec.md` (2026-08-24); source issue CORE-114.
