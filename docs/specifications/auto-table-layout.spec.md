---
title: Auto Table Layout — Column-Width Distribution
slug: /specifications/auto-table-layout
type: spec
status: draft
owner: elijah
created: 2026-08-20
updated: 2026-09-15
sidebar_position: 13
tags: [layout, tables, css-tables, engine, demo-parity]
spec_id: auto-table-layout
issue_id: CORE-81
applies_to: engine 0.x
dependencies: [tables-fragmentation, fragmentation-core, paged-media-css]
---

# Auto Table Layout — Column-Width Distribution

## Overview

The CORE-79 demo triage (2026-08-19) isolated the largest remaining
table-driven diff: **table-stress at 33.5% (TypeAnvil 20 pages vs Prince
45)**. The root cause is the column-width distribution in auto table layout.
TypeAnvil fits 4 data rows/page (row pitch 14.8pt); Prince fits 1–3 (pitch
31.5pt, ~2.1×) because Prince gives the Description column ~112pt so long
descriptions wrap to two lines, while TypeAnvil gives it its full
max-content width (~150pt+) so nothing wraps.

`measure_columns` in `engine/src/table.rs` (from CORE-61) is a
**max-content-only heuristic**: it measures each column's widest wrapped line
capped at the available width, and either keeps those widths as-is (when the
sum fits) or scales them down proportionally. The CSS auto table layout
algorithm (CSS2.1 §17.5.2.2, refined by css-tables-3 §10.4.2) instead uses a
**min-content/max-content basis** and distributes the available width in two
passes. This spec replaces the heuristic with that algorithm.

**Verified ground truth (2026-08-20, demo geometry: 5in × 3in page, 0.5in
margins → 288pt content width — the build-demo.sh flags):**

- Prince table-stress: 45 pages. Its Description column is narrow enough that
  long descriptions wrap to 2–3 lines (page-1 rows wrap: "Anvil," /
  "standard" / "(150 lb)") and the "On hand" / "Unit cost" headers wrap to
  two lines.
- TypeAnvil current: Description ≈ its max-content (~150pt), no wrapping →
  20 pages.
- At 288pt content the standard algorithm hits the **overflow branch**
  (sum(min) ≈ 344 > 288): columns take their min-content widths, so the
  Description column lands at ≈112pt (the width of the glued token
  "Disappearing/reappearing"). The min-content basis is therefore the
  page-count lever; strict max-content (≈150pt) is what TypeAnvil used
  before this spec.
- **Measured result (2026-08-20): table-stress moves 20 → 21 pages.** The
  standard algorithm floors here because at 112pt the Description column
  still fits most corpus descriptions (60–100pt) on one line; only the
  longest (~25%) wrap. Prince's 45 pages come from a **first-page column
  freeze**: Prince measures only the header + first-page rows, so its
  Description column is ≈57pt (the header's own width — it never sees the
  112pt "Disappearing/reappearing" row, which lands on page 2). Matching
  that freeze is a follow-up (CORE-89); the standard algorithm ships first.
- On an unconstrained two-column probe (long text + short number, room to
  spare, Letter geometry), Prince gives the long column its max-content
  (357pt) and the short column max + a share of extra (33pt) — the
  `extra ∝ max-content` branch. On a **constrained** probe (320pt fixed table
  width), Prince splits col1 ≈ 293pt / col2 ≈ 23pt, matching the css-tables-3
  middle branch within measurement error (predicted 297 / 22.9).
  Measurement method: char-box geometry from both PDFs
  (`demo/scripts/col_words.py`), the same technique CORE-79 used.

**Fitness function:** the table-stress corpus fixture page count moves
materially toward Prince's 45 (from 20), plus the parity probe below, plus
unit tests in `engine/tests/tables.rs` and the existing engine tests as
regression guard.

## Goals / Non-Goals

**Goals**

- Intrinsic per-column measurement: **min-content** (widest line when text
  breaks at every opportunity — the longest word/segment) and **max-content**
  (widest line at the breaker's soft opportunities), **uncapped** by the
  available width, over header + body + footer cells, including each cell's
  horizontal padding and border.
- Used table width resolution: `width: auto` → `min(avail, sum(max))`; a
  specified `width` (length or percentage, e.g. `width: 100%`) → that width
  resolved against the containing block, honored as the distribution target.
- Two-pass extra-width distribution per css-tables-3 §10.4.2 (first
  ∝ (max − min) capped at max, remainder ∝ max-content), replacing the
  single `scale = avail/total` heuristic.
- Determinism preserved: identical input → identical column widths.

**Non-Goals** (deferred, per wedge scope)

- `table-layout: fixed` — the engine does not read `table-layout`; `fixed`
  resolves to the auto algorithm (documented limitation, unchanged from
  CORE-61).
- Percentage column widths as *per-column* sizing (`<col>`, `th { width: % }`)
  — only the table's own width percentage is in scope.
- `rowspan` — row-spanning cells are not supported (a `rowspan` cell occupies
  its own grid slot only).
- Full css-tables-3 §10.4.3 spanning-cell distribution — a spanning cell
  contributes an **equal share** of its intrinsic to each spanned column
  (CORE-96 simplification, Prince-matching); the spec's clamping/redistribution
  details are not implemented.
- `border-collapse: separate` + `border-spacing` (defaults: collapse).
- Table-in-table measure (nested tables stay block-stacked).
- Tables inside multicol (CORE-78 fixed the hang; width parity there is a
  later pass).

## Behavior

The engine SHALL implement the following, stated as "shall" rules:

1. **Intrinsic min-content.** For each column, the min-content width SHALL be
   the maximum over its cells of (the widest **whitespace-delimited word**)
   plus the cell's horizontal padding and borders. This deliberately does
   NOT take UAX #14 soft break opportunities inside words: `/` (class SY) and
   hyphens stay glued, so "Disappearing/reappearing clothes" min-content is
   the width of "Disappearing/reappearing" (≈112pt at 9pt Arial) — the
   measured width of Prince's Description column on table-stress (verified
   2026-08-20). Empty cells SHALL contribute their padding + borders only
   (CORE-61 rule). **Standards-alignment note:** CSS sizing treats UAX #14
   break opportunities as min-content break points, so the standard measure
   is narrower; this rule deviates to match Prince's observed glue behavior.
   It is a documented exception under
   `docs/conventions/css-standards-alignment.md`.
2. **Intrinsic max-content.** For each column, the max-content width SHALL be
   the maximum over its cells of (the widest line when the cell text takes
   **no soft breaks** — the whole text on one line, split only at mandatory
   breaks — plus the cell's horizontal padding and borders), measured with
   **no cap** from the available width. (Strict CSS max-content; verified
   2026-08-20.)
3. **Uncapped measurement.** The intrinsic measures SHALL NOT clamp to the
   available width. The existing `measure_text_width` cap (`max_width =
   avail`) SHALL be removed for the intrinsic pass; the cap applies only to
   the *used* widths after distribution.
4. **Used table width.** The used table width SHALL be: the table's specified
   `width` when it resolves (a length, or a percentage resolved against the
   containing block width — carried from the cascade, see Interfaces), else
   `min(avail, sum(max))`. With `width: 100%` the used width is therefore the
   full content width (288pt at the demo geometry), matching Prince.
5. **Distribution (css-tables-3 §10.4.2, two passes).** Given `used` and
   per-column `min`/`max`:
   - If `used ≤ sum(min)`: each column SHALL take its min-content width (the
     table overflows the available width, no panic).
   - If `used ≥ sum(max)`: each column SHALL take its max-content width, and
     the extra `used − sum(max)` SHALL be distributed ∝ max-content.
   - Otherwise: assign every column its min-content width, then distribute
     `used − sum(min)` ∝ (max − min) over unsaturated columns, **capping each
     at its max-content and re-distributing any surplus among the remaining
     unsaturated columns**; any extra left after all columns saturate SHALL
     be distributed ∝ max-content to columns still at min-content (this is
     the branch that lands Description at 112pt on table-stress).
6. **Padding/borders inside columns.** Resolved column widths SHALL be the
   cell content + padding + border box widths (the existing convention);
   `measure_rows` and the row layout SHALL keep consuming `inner_w =
   col_w − padding − border` unchanged.
7. **Determinism.** The algorithm SHALL be pure over (styles, geometry): no
   HashMap iteration order, no wall clock, no randomness. Identical input →
   byte-identical widths, and therefore byte-identical PDFs.
8. **Fallback widths.** A column with zero intrinsic width (all cells empty
   with no padding/border) SHALL resolve to 0 and SHALL NOT panic; a table
   whose columns sum below `used` SHALL distribute the difference per rule 5.
9. **Spanning-cell measure and placement (`colspan`).** A cell with
   `colspan` > 1 SHALL contribute an **equal share** of its intrinsic
   min/max (`cell / span`) to each spanned column (css-tables-3 §10.4.3
   simplification, CORE-96) and SHALL occupy every spanned column slot in
   layout — its box spans the summed column widths and its row height is
   measured at the spanned width. This is what keeps a tfoot's
   `colspan="4"` total from inflating one column to its whole-text width
   (Prince parity, CORE-96). `rowspan` remains unsupported (Non-Goals).

## Interfaces

```rust
// engine/src/table.rs — replaces the current measure_columns body.

/// The column span of a table cell (the HTML `colspan` attribute; default 1).
/// `rowspan` is NOT supported (Non-Goals).
pub fn cell_colspan(dom: &Dom, cell: NodeId) -> usize

/// Resolved per-column widths plus the intrinsic basis (for tests/audit).
#[derive(Clone, Debug, Default)]
pub struct ColumnWidths {
    pub widths: Vec<Scalar>,
    pub min_widths: Vec<Scalar>,   // intrinsic min-content, uncapped
    pub max_widths: Vec<Scalar>,   // intrinsic max-content, uncapped
}

/// Measure each column's intrinsic min/max content width over its cells.
/// Public for unit tests; no available-width argument — intrinsics are
/// uncapped.
pub fn intrinsic_column_widths(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
) -> (Vec<Scalar>, Vec<Scalar>)  // (min_widths, max_widths)

/// Pure css-tables-3 two-pass distribution. Unit-testable with hand-built
/// min/max vectors.
pub fn distribute_column_widths(min_widths: &[Scalar], max_widths: &[Scalar], used_width: Scalar) -> Vec<Scalar>

/// Entry point: intrinsics + used-width resolution + distribution.
pub fn measure_columns(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
    used_width: Option<Scalar>,   // resolved table width; None = auto
) -> ColumnWidths
```

```rust
// engine/src/css.rs — ComputedStyle gains the raw percentage so table layout
// can resolve width: 100% against the containing block (the cascade drops
// percentages today; verified 2026-08-20, src/css.rs:603).
pub width_percent: Option<f64>,   // 0..=100, Some only when width is a %
```

```rust
// engine/src/layout.rs — layout_table_block / collect_table_state compute the
// used width once:
//   used = styles[table].width
//        .or_else(|| styles[table].width_percent.map(|p| avail * p / 100.0))
//        .unwrap_or_else(|| min(avail, sum(max)))   // auto branch
// and pass it to measure_columns.
```

## Acceptance Criteria

Each maps to a real test in `engine/tests/tables.rs` (new) or the demo
pipeline:

1. **Intrinsic min/max uncapped** — Given a cell "Disappearing/reappearing
   clothes", when measured, then min-content = the width of the word
   "Disappearing/reappearing" (≈112pt), max-content = the whole-text width
   (≈145pt), and neither equals the available width
   (`test_intrinsic_min_max_uncapped`).
2. **Distribution: room to spare** — Given min/max vectors whose sum(max) ≤
   used, when distributed, then each column = max + extra ∝ max
   (`test_distribute_extra_proportional_max`).
3. **Distribution: constrained** — Given min/max vectors with
   sum(min) < used < sum(max), when distributed, then each column =
   min + (max−min) share, capped at max, surplus redistributed
   (`test_distribute_middle_branch_caps_and_redistributes`).
4. **Distribution: overflow** — Given sum(min) > used, when distributed, then
   columns keep min widths and the sum overflows used without panic
   (`test_distribute_overflow_keeps_min`).
5. **Used width resolution** — Given the same 6-column table with
   `width: auto` vs `width: 100%` vs `width: 400pt`, when rendered, then the
   used widths differ as specified (`test_table_used_width_resolution`).
6. **Description column wraps (the CORE-79 lever)** — Given a 6-column table
   shaped like table-stress (SKU / long Description / 4 numeric) with
   `width: 100%` at the demo geometry (5in × 3in page, 0.5in margins → 288pt
   content), when measured, then the overflow branch applies (sum(min) > used)
   and the Description column ≈ its min-content ≈ 112pt (Prince-verified
   token width) and the long descriptions wrap to 2+ lines
   (`test_table_stress_description_column_wraps`).
7. **Table-stress page-count movement** — Given `demo/corpus/table-stress.html`
   rendered through the engine at the demo geometry (build-demo.sh flags:
   `--page-width 5in --page-height 3in`, 0.5in margins), when the page count
   is measured, then it is > 20 (measured 21; the old heuristic pinned 20 —
   see the Overview for why full 45-page parity needs the CORE-89 follow-up)
   (`test_table_stress_page_count_grows`).
8. **Two-column probe parity** — Given the two-column probe fixture (long
   text + short numeric, fixed table width of 320pt on a Letter page that
   forces the long cell to wrap), when rendered, then the long column's used
   width equals the Prince-verified constant ≈ 295pt within ±2% (wrapped vs
   unwrapped cell geometry compared in both PDFs during the demo pass)
   (`test_two_column_probe_width`).
9. **Regression** — Given the existing engine tests (incl. the 36 CORE-61
   baseline), when the change lands, then all stay green; the demo scoreboard
   regenerates and table-stress diff drops from 33.5%.

## Edge Cases

- Table with no rows / zero columns: empty `ColumnWidths`, no panic.
- Single-column table: column = used width (auto → min(avail, max)).
- A row whose cells' intrinsic widths exceed `used`: columns stay at min
  (rule 5 overflow branch), cells overflow horizontally like today, no panic.
- `width: 100%` inside a narrow fragmentainer: used = the fragmentainer's
  content width, not the page width (fragmentainer-aware).
- A table with only a header (no body): header cells drive the intrinsics.
- Cells with `white-space: nowrap` or long unbreakable tokens: min-content
  grows accordingly (already handled by the breaker).
- Nested tables: unchanged (block-stacked per CORE-61).
- Table inside multicol (CORE-78 regression guard): widths measured against
  the column content width; the termination fix must not regress.

## Verification

1. `cargo build` clean, `cargo test` all green (existing + new tables tests).
2. `python3 scripts/validate_docs.py` OK (this spec + updated specs).
3. Probe parity: render the two-column probe and table-stress with both
   engines; compare column boundaries and wrapped/unwrapped cell geometry via
   char-box extraction (`demo/scripts/col_words.py <pdf>`) — the
   technique that produced the ground truth above.
4. `demo/corpus/out/scoreboard.json` regenerated; table-stress `typeanvil_pages`
   moves 20 → 21 (see Overview; CORE-89 closes the residual gap) and the
   overall diff drops below 33.5%.
5. Close the loop in Linear (CORE-81) with What-was-built / Verification /
   Next pass; commit messages reference CORE-81.

## References

- CORE-79 second demo triage (parent issue; measurement evidence).
- CORE-61 `tables-fragmentation` spec (the feature this replaces the
  column-measure part of; §Behavior #3).
- css-tables-3 §10.4.2 (extra-width distribution), CSS2.1 §17.5.2.2 (auto
  table layout).
- `docs/conventions/css-standards-alignment.md` — the house rule this spec's
  UAX #14 min-content note is filed under (Behavior 1 deviation).
- Prince ground truth: `/tmp/ts-prince-5x3.pdf` (demo geometry, 45 pages),
  `/tmp/probe-constrained-prince.pdf` (320pt probe split 293/23),
  `demo/scripts/col_words.py` (char-box word dumps, 2026-08-20).
