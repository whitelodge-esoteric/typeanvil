---
title: Table Backgrounds (Cell + Row + Group)
slug: /specifications/table-backgrounds
type: spec
status: approved
owner: maintainers
created: 2026-08-20
updated: 2026-09-27
sidebar_position: 16
tags: [layout, tables, css-tables, backgrounds, engine]
spec_id: table-backgrounds
applies_to: engine 0.x
dependencies: [tables-fragmentation, auto-table-layout, table-first-page-column-freeze]
---

# Table Backgrounds (Cell + Row + Group)

## Overview

`background-color` on table parts must paint. The implementation addressed two
gaps:

1. **Cell bg + border did not coexist.** `layout_table_cell` overwrote
   `FragmentContent::Background` with `FragmentContent::Border` whenever any
   border was present, so every corpus table with `th, td { border; background }`
   rendered its colored headers and total rows WHITE (invoice p1: 0 `#a8dadc`
   px vs Prince 4,016). Fixed by the table-fragmentation change: keep the Background
2. **Row/group-level backgrounds did not paint at all.** `tr`, `thead`,
   `tbody`, and `tfoot` blocks emitted no fill (the triage repro's `#00ff00`
   was absent). Fixed here: the row and group fragments carry their own
   `Background` content.

Paint order follows css-tables-3: table → column groups → columns → row
groups → rows → cells, with borders and text on top. The pdf emitter already
collects Backgrounds before Borders before Text in a pre-order walk, so
putting the fill on the row/group fragment (the parent of the cells) gives
the correct z-order with no emitter change.

## Goals

- A cell with `background-color` AND a border renders both (fill under, border
  stroke on top).
- A row (`display: table-row`) with `background-color` paints a fill spanning
  the table width and the row height.
- A group (`thead`/`tbody`/`tfoot`) with `background-color` paints a fill
  spanning the table width and the rows it placed in the current fragmentainer.
- Row/group fills paint UNDER cell fills; all fills paint under borders and
  text.
- Determinism: identical input → byte-identical PDF (fills are paint-only;
  layout and pagination are unchanged).

## Non-Goals

- `table`-, `colgroup`-, and `col`-level backgrounds (out of scope for
  this spec; same pattern extends later).
- `border-collapse: separate` (the engine is collapse-only).
- Background images, gradients, `background-clip`/`origin`/`position` —
  `background-color` only.

## Behavior

1. A cell whose `background-color` is set AND whose border-box has any
   border width > 0 shall render the fill AND the border: the fragment keeps
   `FragmentContent::Background` and gains a child fragment with
   `FragmentContent::Border`.
2. A cell whose `background-color` is set and which has no border shall render
   the fill (unchanged from the block path).
3. A row (`display: table-row`) whose `background-color` is set shall emit a
   `Background` fill on its row fragment, sized to the table's available width
   and the row's measured height, when the row height is positive.
4. A group (`thead`/`tbody`/`tfoot`) whose `background-color` is set shall
   emit a `Background` fill on its group fragment, sized to the table's
   available width and the height of the rows placed in this fragmentainer,
   when that height is positive.
5. Paint order shall be: group fills, then row fills, then cell fills, then
   borders, then text. The pdf emitter's pre-order collection satisfies this
   because a parent's fill is collected before its children's fills, and all
   fills draw before borders and text.
6. A group that continues across pages shall paint its fill only over the
   rows placed on each page (the fragment's slice height), never the whole
   group.
7. Row/group fills shall not change layout or pagination: `used` heights and
   break tokens are unaffected by the presence of a fill.
8. A zero-height row/group shall emit no fill.

## Interfaces

No public API changes. Internal changes in `engine/src/layout.rs`:

- `layout_table_row`: after building the row fragment, set
  `fragment.content = FragmentContent::Background(bg)` when
  `styles[id].background_color` is `Some` and `row_height > 0`.
- `layout_table_group`: same on the group fragment with `height > 0`.

`pdf.rs::collect` is unchanged (pre-order walk already orders fills before
borders before text).

## Acceptance Criteria

1. **Cell coexistence.** Given a table cell with `background-color` and
   `border`, when laid out, Then a `Background` fragment whose children
   include a `Border` fragment exists. → `test_cell_background_survives_border`
   in `engine/tests/tables.rs`.
2. **Row fill.** Given `tr { background-color }` with bordered cells that also
   have backgrounds, when laid out, Then each row renders as a full-width
   (content-box width) `Background` fragment whose children are cell
   `Background` fragments. → `test_row_background_paints_under_cell_backgrounds`.
3. **Group fill.** Given `thead { background-color }`, when laid out, Then
   exactly one full-width `Background` fragment (the thead) exists above
   row/cell fills. → `test_group_background_paints_under_rows`.
4. **Pixel verification.** Given a repro with a red thead, green zebra rows,
   blue cells, and borders, when rendered at 216 DPI and pixel-scanned, Then
   red, green, and blue exact-hex counts are all > 30k px and border pixels
   are present. Verified 2026-08-20: red 38,997 / green 38,618 / blue 39,872 /
   border 11,355 / text 1,078 (1-page 5in×3in render).

## Edge Cases

- **Row that does not fit** returns an empty deferred fragment (no fill) —
  correct, nothing is placed.
- **Group resume bookkeeping** (`seen_all_children` empty-token path) returns
  a zero-height fragment — the height guard skips the fill.
- **Footer repetition**: the footer group is laid out on every fragment that
  contains table content (including the last); its fill follows the same
  rule, so a colored `tfoot` repeats its band per page.

## References

- `demo/corpus/TRIAGE.md` — pixel-scan evidence and minimal repros.
- `engine/src/layout.rs` — table cell, row, and group background fragments.
- css-tables-3 §16.2 (table painting order).
