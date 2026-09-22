---
title: Tables × Page Breaks
slug: /specifications/tables-fragmentation
type: spec
status: approved
owner: elijah
created: 2026-08-17
updated: 2026-09-21
sidebar_position: 4
tags: [layout, tables, fragmentation, css-tables, engine]
spec_id: tables-fragmentation
issue_id: CORE-61, CORE-116, CORE-235
applies_to: engine 0.x
dependencies: [fragmentation-core, paged-media-css, wpt-conformance-harness]
---

# Tables × Page Breaks

## Overview

The Prince-beating feature. WeasyPrint's known weakness is super-linear
multi-page tables (relayout-from-scratch per page); Prince's differentiator is
tables that fragment cleanly with **repeating header rows, no mid-cell slicing,
and borders that survive breaks**. TypeAnvil gets this for free in the engine
because CORE-51 made fragmentation the core: the fragment tree + break tokens
already carry state across pages, so a table is just a family of boxes with
**rows as breakpoints**.

Today `display: table|table-row|table-cell|table-header-group|table-footer-group`
fall through to block/inline (the `Display` enum has only `Block | Inline |
None`), so a `<table>` renders as stacked blocks and the CORE-60 baseline shows
the gap (`table-fragmentation-002a/b`, `fixedpos-in-footer-forced-break`).

The scope is the wedge: **fragmentation of a styled table**, not the full
css-tables-3 suite. Column/row sizing, border-collapse, header/footer
repetition, rows as monolithic break units — enough that a real report or
invoice table paginates correctly. The WPT fitness function is the css-break
table print-reftest subset (table/ + table/repeated-section/).

**Fitness function:** css-break table print-reftests via the harness, plus unit
tests in `engine/tests/` and the existing 36 engine tests as regression guard.

## Goals / Non-Goals

**Goals**

- Real table layout pass: `display: table` → table wrapper box; row groups
  (header/footer/body), rows, cells with column widths; `border-collapse:
  collapse` with `border-spacing: 0`; cell padding; `thead`/`table-header-group`
  and `tfoot`/`table-footer-group` semantics.
- Fragmentation: a table taller than the fragmentainer breaks **at row
  boundaries** — rows are breakpoints. A row may split only when a cell's
  content is itself taller than the fragmentainer; then the cell fragments and
  its row continues (the css-break "monolithic row unless a cell forces a split"
  rule).
- **Repeating header:** `thead` / `table-header-group` is re-laid-out at the
  top of every fragment after the first. **Footer:** `tfoot` /
  `table-footer-group` renders at the bottom of the fragment it closes (and
  repeats on continuation fragments).
- Borders survive breaks: the border between two rows stays with the
  row-start side; the table's outer border box is not lost across fragments.
- `break-inside: avoid` on rows/groups honored via the existing appeal scoring;
  monolithic cells overflow rather than slice.
- Deterministic pagination: no relayout-from-scratch — continuation uses break
  tokens exactly like CORE-51 (O(n) multi-page tables).

**Non-Goals** (deferred, per wedge scope)

- Auto table layout details: percentage column widths, `table-layout: auto`
  algorithm beyond the greedy measure pass needed for fixed/auto columns.
- Row/column spans (`rowspan`/`colspan`) — render as unsupported (fall back to
  per-cell block stacking with a documented limitation), full css-tables-3
  spans deferred.
- ~~`border-collapse: separate` + `border-spacing`~~ — **SUPERSEDED by CORE-235**:
  see §Behavior 16–18. The separate model (CSS initial) is now supported.
- Captions (`<caption>`), table in table (nested tables), tables inside
  multicol, RTL tables.
- Full css-tables WPT suite — only the css-break table print-reftests are the
  fitness gate.

## Behavior

The engine SHALL implement the following, stated as "shall" rules:

1. **Display mapping.** The `Display` enum SHALL gain `Table`, `TableRowGroup`,
   `TableHeaderGroup`, `TableFooterGroup`, `TableRow`, `TableCell` variants,
   mapped from the CSS display longhands (`table`, `table-row-group`,
   `table-header-group`, `table-footer-group`, `table-row`, `table-cell`).
   Unknown/other table display values (`table-column`, `table-caption`,
   `inline-table`, `table-column-group`) SHALL resolve to `Block` with a
   documented limitation, never a panic.
2. **Anonymous table boxes.** A `table-row-group/header/footer` without a
   `table` ancestor, or a `table-row`/`table-cell` without its group/row
   ancestor, SHALL be wrapped in anonymous table/row-group/row boxes per
   css-tables-3 §17 so a bare `<tr>`/`<td>` still renders. (Simplified: wrap in
   the nearest missing level in the table-family chain during box-tree build.)
3. **Column measure pass.** Before laying out rows, the table SHALL measure
   each column's min/max content width over its cells (the same
   `break_paragraph` measure used for text, capped at the fragmentainer
   content width). Table width SHALL be the sum of column widths, clamped to
   the available width; with `width: auto`, the table uses the min of the sum
   and the available width; with an explicit `width`, columns scale
   proportionally from their measured max widths (simplified fixed+auto
   hybrid).
4. **Row layout.** Each row SHALL lay out its cells side by side at the
   resolved column widths; row height SHALL be the max of its cells' heights;
   cells SHALL have their padding/borders inside the column width.
5. **border-collapse: collapse.** With `border-collapse: collapse`, adjacent
   cell/row/table borders SHALL collapse
   to a single border: 1px lines at shared edges, the table's outer border at
   the table box edge. `border-spacing` SHALL be treated as 0.
6. **Rows as breakpoints.** A table taller than the fragmentainer SHALL break
   only between rows, never through a row, unless a cell's content is itself
   taller than the fragmentainer (then the row SHALL fragment and continue on
   the next fragmentainer with its remaining cells/borders — the
   break-inside:avoid-per-row rule of css-break-3).
7. **Repeating header.** After the first fragment, every continuation fragment
   of a table SHALL re-lay-out the `thead`/`table-header-group` at its top,
   drawn with its own background/borders, consuming its own height (not
   stealing content space from the first fragment).
8. **Footer placement.** A `tfoot`/`table-footer-group` SHALL be placed at the
   bottom of the fragment that contains its preceding body rows (bottom of the
   last fragment of that group's content), and SHALL repeat on each
   continuation fragment that still has body rows below it. A table that fits
   on one fragmentainer SHALL render header, body, footer in document order.
9. **Borders across breaks.** The row-start border SHALL be drawn on the
   fragment that starts the row; the table's outer border box SHALL be
   preserved across fragments (top border on first fragment, side borders on
   every fragment, bottom border on the last).
10. **break-inside: avoid.** `break-inside: avoid` on a row/group SHALL be
    honored via the existing break-appeal scoring (CORE-51): if the row doesn't
    fit, it moves whole to the next fragmentainer rather than splitting,
    unless the row alone is taller than the fragmentainer (then it fragments —
    rule 6).
11. **Monolithic overflow.** A cell/row taller than the fragmentainer SHALL
    overflow, never slice (CORE-51 rule), and the table continues on the next
    fragmentainer with the overflowed content carried by break tokens.
12. **Determinism.** Pagination SHALL be token-based (CORE-51): laying out
    page N+1 passes the table's outgoing break token; finished rows are
    skipped, the interrupted row resumes. A 100-row table spanning 10 pages
    SHALL lay out in linear time (no relayout of earlier pages).
13. **Generated content / TOC unchanged.** Tables SHALL NOT disturb existing
    paged-media features: a table inside a page with margin boxes, running
    headers, or `target-counter` resolution renders correctly (regression
    guard: report + invoice demos, CORE-52 tests).
14. **Row deferral is bounded (CORE-109).** A row that does not fit the
    remaining space of the current fragmentainer, but fits one full
    fragmentainer, SHALL defer to the next fragmentainer at most ONCE. If,
    on the continuation fragmentainer, the row still does not fit (for
    example a repeating `thead` consumes space on every page so no page can
    ever hold the row), it SHALL be placed anyway and overflow past the
    bottom edge (the monolithic-overflow rule) rather than defer again.
    Deferring twice for the same row would re-create the same break-before
    token on every page forever — pagination MUST terminate in bounded time
    for every input. (Prince 16.2 probed 2026-08-23: it places such a row on
    the next page; only when the row fits NO fragmentainer at all does it
    drop the row with "dropping unpackable block" — TypeAnvil instead
    force-places it, preferring visible overflow over silent content loss.)
15. **Oversized-row cell continuation survives (CORE-116).** When a row
    fragments because a cell crosses a fragmentainer boundary, the row SHALL
    emit an outgoing break token carrying each laid-out cell's state:
    continuations for cells still in flight and done-markers for cells that
    finished. On the next fragmentainer the interrupted cell SHALL resume,
    finished cells SHALL NOT re-render, cells after the break point SHALL
    start fresh there, and rows following the fragmented row SHALL continue
    at the resumed row's used height — no table content past an oversized
    row may be dropped or duplicated. A CONTINUATION pass of a row that has
    already split SHALL place immediately; deferring it would loop forever
    against a repeating header (the CORE-109 trap).
16. **border-collapse: separate (CORE-235).** `border-collapse: separate` is
    the CSS-initial value and SHALL be the engine's UA default for `table`
    (the UA stylesheet SHALL NOT force `collapse`). Under `separate`, each
    cell SHALL keep its own four borders; `border-spacing` SHALL add gaps
    between adjacent cells (horizontal and vertical), and between the
    table's border/padding box edge and the outermost cells (one spacing
    gap per side, matching Chrome). The table's own border box SHALL paint
    around the whole table (slice semantics: top band on the first fragment,
    side bands on every fragment, bottom band on the last).
17. **Separate-model sizing (CORE-235).** Under `border-collapse: separate`,
    the column grid SHALL be sized inside the table's border + padding +
    border-spacing chrome: an explicit `width` is the table's border-box
    width, and the usable column width is that width minus
    `border-left/right`, `padding-left/right`, and `border-spacing * (N+1)`
    for N columns. Cells SHALL be offset from the table's left border-box
    edge by `border-left + padding-left + spacing-home`, and each subsequent
    column by its width plus one horizontal spacing gap. Row height SHALL
    include the cell's border-bottom (measure_rows), and rows SHALL be
    separated by vertical border-spacing; the table block SHALL consume
    `border-top + padding-top + spacing` before its first row (first
    fragment only) and `spacing + padding-bottom + border-bottom` after its
    last row (last fragment only).
18. **Border-box height includes bottom border (CORE-235).** A finished
    (non-continuing) box's border-box height SHALL include its bottom
    border: block, flex-container, and grid-container height calculations
    add `border-bottom` after `padding-bottom` when the box did not break,
    so a bordered box's bottom edge and a following sibling's `y` match the
    equivalent `display:block` box. Slice continuation SHALL apply to border
    painting: a box that fragments paints its top border band only on its
    first fragment and its bottom band only on its last fragment (side
    bands on every fragment).

## Acceptance Criteria

Each maps to a real test in `engine/tests/tables.rs` (new) or the WPT harness:

1. **Basic table** — Given `<table><tr><th>A<th>B<tr><td>1<td>2`, when
   rendered, then two columns of equal width with header row and data row lay
   out side by side (`test_basic_table_cells_side_by_side`).
2. **Repeating header** — Given a 30-row table with `<thead>` and a
   3-fragmentainer page, when rendered, then every page after the first shows
   the header row at top (`test_repeating_header_every_page`).
3. **No mid-row slicing** — Given a table with a row taller than one
   fragmentainer's remaining space but shorter than a full fragmentainer, when
   rendered, then the row moves whole to the next fragmentainer
   (`test_row_moves_whole_not_sliced`).
4. **Row fragmentation on oversized cell** — Given a cell containing a
   monolithic block taller than the fragmentainer, when rendered, then the row
   splits and the cell continues on the next fragmentainer
   (`test_oversized_cell_fragments_row`).
5. **border-collapse** — Given a table with `border-collapse: collapse`,
   1px borders on cells, when rendered, then shared edges render as single
   lines and the outer border box is complete
   (`test_border_collapse_single_lines`).
6. **Footer placement** — Given a table with `<tfoot>` and body rows spanning
   two fragmentainers, when rendered, then the footer sits at the bottom of
   the fragment closing the body AND repeats on every continuation fragment
   (`test_footer_repeats_every_fragment`; Prince 16.2 repeats the tfoot on
   every page — verified 2026-08-20 on table-stress pages 1-44 and invoice
   pages 1-4).
7. **O(n) multi-page** — Given a 100-row × 10-page table, when laid out, then
   it completes with correct page count and row content preserved
   (`test_tables_pagination_linear_100x10`).
8. **Regression** — Given the existing 36 engine tests + the CORE-52 report
   and invoice demos, when the table pass lands, then all 36 stay green and
   both demos render (invoice now with a real table if the fixture is
   converted).
9. **WPT fitness** — Given the harness, when `--engine cli` runs the css-break
   table print-reftests (`css/css-break/table/`), then the previously failing
   table tests pass (baseline: 3 failing from CORE-60) and none of the
   currently-passing tests regress (gate on the CORE-60 baseline).
10. **Giant-row deadlock guard (CORE-109)** — Given a table whose row fits a
    full fragmentainer but not a fragmentainer shrunk by the repeating
    header, when rendered, then pagination terminates in bounded time, the
    row defers once and is then placed whole
    (`test_giant_row_defers_at_most_once`). A row taller than any
    fragmentainer still places monolithically without hanging
    (`test_row_taller_than_page_still_places_monolithically`).
11. **Oversized-row content survives (CORE-116)** — Given a cell taller than
    one fragmentainer followed by more rows, when rendered, then every line
    of every cell appears exactly once across pages and later rows render —
    nothing dropped or duplicated
    (`tests/core116_oversized_row.rs::oversized_row_cell_content_continues_across_pages`,
    `::oversized_row_resume_does_not_duplicate_finished_cells`).
12. **Separate model (CORE-235)** — Given the four WPT fixtures
    `table-fragmentation-001{a,b,c,d}-print.html` (5×3in page, 0.5in
    margins, `border-spacing: 0.25in`, table `border: 0.25in`, cell
    `border: 0.25in`, gold `content` block), when rendered by the harness at
    the default geometry, then the test side matches its reference side:
    `001a/001b` on 2 pages matching the Chromium oracle's 2 pages
    (page count and per-page ink), and `001c/001d` on the same page count
    as their references. The table's own border box, the cell borders
    (including border-bottom), the border-spacing gaps, and the slice
    border bands on continuation fragments all render.

## Edge Cases

- A table with no rows / zero rows: renders as empty table box, no panic.
- A row whose cells sum wider than the fragmentainer: columns clamp to
  available width, cells shrink proportionally, no horizontal overflow panic.
- A header taller than the fragmentainer: the header itself fragments
  (monolithic rule), body rows follow on subsequent fragmentainers.
- `table` with `display: none` on ancestor: whole table suppressed (existing
  CORE-51 rule).
- A table directly inside another table's cell: deferred (Non-Goals) — the
  inner table renders as block-stacked rows with a documented limitation,
  never a panic.
- Empty cells: zero-content cells still contribute column measure (padding
  + borders), per css-tables-3.
- `colspan`: a spanning cell contributes an equal share of its intrinsic to
  each spanned column and occupies every spanned slot in measure AND layout
  (CORE-96, auto-table-layout §Behavior 9). `rowspan`: not supported — a
  `rowspan` cell occupies one grid slot (documented limitation, no panic).

## Verification

1. `cargo build` clean, `cargo test` all green (36 existing + new tables
   tests).
2. `python3 scripts/validate_docs.py` OK (this spec + any updated specs).
3. Harness: run the css-break table print-reftest subset via the CORE-60
   command (`--engine cli --cli-cmd "engine/target/debug/typeanvil render"`),
   confirm the 3 baseline failures now pass and the gate holds.
4. Visual: rasterize the table demo (new fixture or converted invoice) and
   confirm repeating header, intact borders, no mid-row slice.
5. Close the loop in Linear (CORE-61) with What-was-built / Verification /
   Next pass, commit messages referencing CORE-61.
