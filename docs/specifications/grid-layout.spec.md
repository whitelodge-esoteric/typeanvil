---
title: Grid Layout — Minimal Track/Placement Model
slug: /specifications/grid-layout
type: spec
status: draft
owner: maintainers
created: 2026-09-04
updated: 2026-09-27
sidebar_position: 21
tags: [layout, css-grid, engine, wpt]
spec_id: grid-layout
applies_to: engine 0.x
dependencies: [fragmentation-core, flexbox-fragmentation]
---

# Grid Layout — Minimal Track/Placement Model

## Overview

The `css-page/margin-boxes` corpus showed that 26 of 37 references
use `display: grid`. The engine fell back to block layout, so those refs
could never match. This spec defines the minimal, refs-shaped grid model
(not full css-grid): this is the subset the WPT corpus and the demo
corpus exercise.

## Behavior

1. `display: grid` / `inline-grid` shall compute to a grid container.
   `inline-grid` is treated as block-level in paged flow (the `inline-flex`
   model). The servo build's `layout.grid.enabled` pref shall be enabled at
   stylesheet parse (the existing column-feature preference pattern).
2. The engine shall read computed `grid-template-columns` /
   `grid-template-rows` from stylo and flatten each
   `TrackSize` to its breadth: `Breadth(b)` and `FitContent(b)` keep `b`;
   `Minmax(min, _)` takes `min`. A list containing `TrackRepeat` (i.e. any
   `repeat()`) is treated as NO explicit tracks (every track auto) — a
   documented limitation.
3. Items shall be the container's in-flow element children (display:none and
   out-of-flow excluded), auto-placed row-major in document order (css-grid-1
   §8.5 sparse packing). Grid containers shall be treated as block-level
   boxes by the item collector.
4. Column track sizing shall be: fixed lengths resolve against the
   container's inner width; percentages resolve against inner width; `auto`
   tracks take their items' max shrink-to-fit width, then share the leftover
   space equally (css-grid-1 §12.5 maximize + §12.7 stretch, the
   `justify-content: normal` default); `fr` tracks share remaining space
   proportionally to their flex values.
5. Row track sizing shall follow the same algorithm along the block axis;
   `auto` rows take the max of their items' measured heights at the assigned
   column width; percentages resolve against the page height (the v1
   definite-block-size proxy).
6. Items shall be laid out via the normal block path (`layout_box`) into
   their cell rect, so nested flex, tables, and blocks work unchanged.
7. Fragmentation: rows shall be monolithic (like flex lines). A row that
   does not fit the current fragmentainer moves whole to the next page.
   Items never split inside their cell. Resume state names the first
   unfinished item's block index. A later grid containment rule requires that a container with a
   definite block size whose own box fits the fragmentainer does not
   fragment at all — rows past the definite height are ink overflow of
   the box, clipped at the page edge; the monolithic-row break applies
   only when the container is auto-height or taller than the page.
   Resume placement starts at the fragmentainer top, never at the resumed
   row's full track offset.)
8. Zero-area background rects (empty auto-tracked cells) shall be skipped at
   PDF emit, not treated as fatal.

## Non-goals

Named lines/areas, explicit placement (`grid-column`/`grid-row`), `span`,
dense packing, `repeat()`, subgrid, masonry, `min-content`/`max-content`
track sizing beyond the auto maximum, `align/justify-self` non-default
values, baseline alignment.

## Acceptance Criteria

1. `monolithic-overflow-007/008`-style grid containment docs render with the
   grid container holding its oversized child (honest geometry, verified by
   pixel probe).
2. Full-suite gate: no true regressions (blank-on-both-sides
   caveat applies: blank-on-both-sides flips to honest FAIL are gains).
3. `cargo test` green, including grid unit tests for track sizing and
   placement.
4. The `dimensions-*` margin-boxes refs that need only grid show a large
   pixel-delta reduction (verified: dimensions-005 92040 → 6062 px,
   dimensions-008 101890 → 10403 px, dimensions-012 30302 → 4492 px).
