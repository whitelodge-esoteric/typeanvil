---
title: Table First-Page Column Freeze (Prince Parity)
slug: /specifications/table-first-page-column-freeze
type: spec
status: draft
owner: maintainers
created: 2026-08-20
updated: 2026-09-27
sidebar_position: 14
tags: [layout, tables, css-tables, engine, demo-parity]
spec_id: table-first-page-column-freeze
applies_to: engine 0.x
dependencies: [auto-table-layout, tables-fragmentation, fragmentation-core, paged-media-css]
---

# Table First-Page Column Freeze (Prince Parity)

## Overview

The standard css-tables-3 auto table layout (intrinsic
min/max over **all** cells + two-pass distribution) and the constrained
two-column probe now matches Prince within ±2%. But **table-stress only moved
20 → 21 pages** (Prince: 45). The measured root cause,
2026-08-20): **Prince freezes column widths from its FIRST page's content** —
it measures only the header + the rows that fit the first fragmentainer. On
table-stress its Description column is ≈57pt (the *header's own width*),
because the 112pt glued token "Disappearing/reappearing" sits in row 13
(page 2) and never enters the measure. The standard CSS algorithm measures
ALL rows → Description = 112pt → most corpus descriptions (60–100pt) still fit
one line → 21 pages.

This spec amends the **measure set** of `auto-table-layout`: when a table
fragments across fragmentainers, the intrinsic min/max pass measures only the
header + the body rows whose top edge lands in the first fragmentainer (+ the
footer), and those widths are frozen for the whole table. The distribution
algorithm (css-tables-3 §10.4.2, auto-table-layout §Behavior 5) is **unchanged**
— only the inputs change. A table that fits a single fragmentainer measures
all rows exactly as the standard all-rows measure does, so single-page tables, the two-column probe,
and the WPT subset are byte-for-byte unchanged (the regression guard).

**Conformance note:** css-tables-3 §10.4.1 defines table width from *all*
rows; the first-page freeze is a deliberate, documented deviation to match
Prince's observed behavior — filed under
`docs/conventions/css-standards-alignment.md`. The wedge is Prince-parity
output, and this deviation is scoped to fragmented tables only — the same
spirit as the standard-measure UAX#14 glue note. The engine shall NOT apply the
freeze to single-fragmentainer tables.

**Measured result (2026-08-20, first-freeze landing):** table-stress moves
21 → **42 pages** (Prince: 45) at the demo geometry; the Description column
freezes at the header's own width (the 112pt "Disappearing/reappearing" token
in row 34 never enters the measure). Overall diff on the corpus drops
33.07% → 29.06%. The constrained two-column probe stays within ±2% of Prince
(295pt), and the css-break table WPT subset does not regress.

**Follow-up fixes (same day):** the frozen widths were NOT the full story —
the remaining 40v45 gap came from (a) the tfoot not repeating per page
(tables-fragmentation rule 8 was unimplemented), (b) `th` not bold in the UA
defaults (Prince bolds it), (c) the colspan-blind measure inflating the On
hand column, and (d) row heights omitting the collapsed row-start border.
With all four fixed, table-stress renders **45 pages** (Prince: 45) and the
frozen widths land within ~0.4–6pt of Prince's measured columns. See
`auto-table-layout` §Behavior 9 (colspan), `ua-print-defaults` §Behavior 8
(bold th), and `tables-fragmentation` rule 8 (footer repeat).

**Fitness function:** table-stress `typeanvil_pages` ≥ 40 (from 21; Prince
45), the two-column probe still within ±2% of Prince, the css-break table WPT
subset does not regress, and the full existing tables suite stays green.

## Goals / Non-Goals

**Goals**

- A deterministic **measure-scope rule**: which body rows contribute to the
  intrinsic min/max pass, resolved once per table at its first layout.
- **Freeze-once semantics**: the frozen `ColumnWidths` are computed at the
  table's first fragmentainer and reused by every continuation fragment;
  no fragmentainer after the first re-runs the freeze resolution.
- **Fixed-point with a hard cap**: "rows that fit the first fragmentainer"
  is self-referential (rows fit depends on widths, widths depend on rows
  measured), so the scope resolves via a bounded iteration — hard cap 3
  passes (the same convergence cap as `counter(pages)`) — and the
  last computed scope freezes. Always terminates, always deterministic.
- Single-fragmentainer tables measure all rows (scope `All`), identical to
  the standard table-layout behavior.
- Determinism preserved: identical input → identical scope → identical widths
  → byte-identical PDF.

**Non-Goals** (unchanged from `auto-table-layout.spec.md`)

- `table-layout: fixed` resolution (still the auto algorithm).
- Per-column percentage widths (`<col>`, `th { width: % }`).
- `rowspan`/`colspan` width-sharing contributions.
- `border-collapse: separate` + `border-spacing`.
- Nested-table measure (inner tables resolve their own scope independently).
- Table-in-multicol width parity (the termination guard is covered by the
  multicol integration test; the freeze applies
  with the multicol column as the first fragmentainer, but width parity there
  remains a later pass).

## Behavior

The engine SHALL implement the following, stated as "shall" rules:

1. **Measure scope.** The intrinsic min/max pass SHALL measure the header row
   group, the first `k` body row groups, and the footer row group, where `k`
   is the number of body rows whose **top edge lies within the first
   fragmentainer's content area** (a row that starts on page 1 counts even if
   it breaks to page 2; a row is NOT counted if its top edge is past the
   fragmentainer's content bottom). `k` SHALL be resolved by rule 3.
2. **Single-fragmentainer tables are unchanged.** When the table's entire
   content fits the first fragmentainer, the scope SHALL be `All` (header +
   every body row + footer) — identical to the standard all-rows measure — and the freeze SHALL NOT
   apply. This is the regression guard for the two-column probe, the WPT
   subset, and every existing single-page tables test.
3. **Bounded fixed-point resolution.** The freeze scope SHALL be resolved by
   iteration at the table's first layout:
   - Pass 1: measure with scope `All`, distribute at the used width
     (auto-table-layout §Behavior 4/5), and lay out the first fragmentainer;
     count `k` per rule 1.
   - Pass 2..N: re-measure with scope `FirstPage { body_rows: k }`, re-lay
     the first fragmentainer, re-count `k`; repeat until `k` stabilizes.
   - The iteration SHALL hard-cap at 3 passes; on the cap, the scope from the
     last pass SHALL freeze. The cap guarantees termination regardless of
     oscillation between narrow/wide widths (see Edge Cases).
4. **Freeze once.** The frozen `ColumnWidths` SHALL be computed exactly once,
     at the table's first fragmentainer layout, and SHALL be threaded through
     the table's fragment/resume state so every continuation fragmentainer
     reuses them. No fragmentainer after the first SHALL re-run the freeze
     resolution or re-measure intrinsics.
5. **Distribution unchanged.** Given the frozen intrinsics, the used-width
     resolution and the two-pass css-tables-3 distribution SHALL be exactly
     auto-table-layout §Behavior 4–5. The freeze only changes which cells
     feed the intrinsic min/max; it does not add or remove distribution
     branches. In particular, the overflow branch (`used ≤ sum(min)`) still
     applies with the frozen intrinsics, and the freeze may move a table off
     the overflow branch (a frozen sum(min) can drop below `used`) — that is
     expected, not an error.
6. **Determinism.** The scope resolution and freeze SHALL be pure over (dom,
     styles, geometry): no HashMap iteration order, no wall clock, no
     randomness, no relayout-from-scratch per page. Identical input →
     identical scope → identical widths → byte-identical PDF.
7. **Header/footer-only tables.** A table with no body rows SHALL resolve to
     scope `All` (header + footer only); `k = 0` SHALL NOT panic and SHALL
     mean "no body rows measured", equivalent to `All` when no body exists.
8. **First fragmentainer definition.** The "first fragmentainer" SHALL be the
     fragmentainer in which the table's first content lands: a partial page
     when the table starts mid-page, the first multicol column when the table
     sits in a multicol container (the `table_fragments_inside_multicol` termination guard must
     not regress), and its content height is the height available at that
     point.

## Interfaces

```rust
// engine/src/table.rs — freeze-scope additions/changes.

/// Which rows contribute to the intrinsic measure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeasureScope {
    /// Header + every body row + footer (standard behavior; single-
    /// fragmentainer tables, and pass 1 of the freeze resolution).
    All,
    /// Header + the first `body_rows` body rows + footer (frozen scope).
    FirstPage { body_rows: usize },
}

/// Measure each column's intrinsic min/max content width over the scoped
/// cells (min = widest whitespace-delimited word, max = no-soft-break text,
/// both uncapped — auto-table-layout §Behavior 1–3, unchanged). Public for
/// unit tests; scope replaces the implicit all-cells measure.
pub fn intrinsic_column_widths(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    scope: MeasureScope,
) -> (Vec<Scalar>, Vec<Scalar>)  // (min_widths, max_widths)

/// Pure css-tables-3 two-pass distribution, unchanged by the freeze.
pub fn distribute_column_widths(min_widths: &[Scalar], max_widths: &[Scalar], used_width: Scalar) -> Vec<Scalar>

/// Resolve the frozen scope at the table's first layout: bounded fixed-point
/// (rule 3), pure over (dom, styles, geometry). Returns `All` when the table
/// fits a single fragmentainer.
pub fn resolve_freeze_scope(
    dom: &Dom,
    styles: &[ComputedStyle],
    table_id: NodeId,
    avail_width: Scalar,
    used_width: Option<Scalar>,
    first_fragmentainer_height: Scalar,
) -> MeasureScope
```

```rust
// engine/src/layout.rs — table fragment state gains the frozen widths.
// struct TableFragmentState { ... frozen_widths: ColumnWidths, ... }
// First fragmentainer: scope = resolve_freeze_scope(...) → measure_columns
// with that scope → store ColumnWidths in state. Continuation
// fragmentainers: read frozen_widths from the resume token chain; never
// re-resolve.
```

`measure_columns` SHALL keep its existing signature (no new parameter) and
gain the scope via the freeze state: the layout layer resolves the scope
once and passes it down, so the public seam stays stable.

## Acceptance Criteria

Each maps to a real test in `engine/tests/tables.rs` (new) or the demo
pipeline:

1. **Freeze excludes late wide rows** — Given a table that fragments across
   2+ pages where a later body row (e.g. row 13) carries a 112pt glued token
   ("Disappearing/reappearing"), when the frozen scope resolves, then the
   Description column's min-content ≈ the header's own min-content (≈57pt),
   NOT 112pt (`test_freeze_excludes_late_wide_rows`).
2. **Single-fragmentainer no-op** — Given a table that fits one
   fragmentainer, when measured with the freeze resolution, then the frozen
   widths equal the `All`-scope widths exactly (`test_freeze_noop_single_fragmentainer`).
3. **Freeze-once across pages** — Given a table fragmenting over 3+ pages,
   when pages 2..n are laid out, then their per-column widths equal page 1's
   frozen widths (asserted on the fragment state / resume chain)
   (`test_freeze_widths_stable_across_pages`).
4. **Determinism** — Given the same multi-page table rendered twice, when
   compared, then the PDFs are byte-identical (`test_freeze_deterministic`).
5. **Header-shorter-than-fragmentainer edge** — Given a first fragmentainer
   shorter than the header alone, when the scope resolves, then it degrades
   to `FirstPage { body_rows: 0 }` without panic and the table lays out
   (`test_freeze_header_only_scope`).
6. **Table-stress page-count movement** — Given `demo/corpus/table-stress.html`
   at the demo geometry (build-demo.sh flags: `--page-width 5in --page-height
   3in`, 0.5in margins → 288pt content), when the page count is measured,
   then `typeanvil_pages` ≥ 40 (from 21; Prince 45)
   (`test_table_stress_freeze_pages_ge_40`).
7. **Two-column probe parity preserved** — Given the constrained two-column
   probe (320pt fixed table width, Letter geometry — a single-fragmentainer
   table), when rendered, then the long column's used width stays within ±2%
   of the Prince-verified constant ≈ 295pt (`test_two_column_probe_width`
   stays green — the freeze must not move it).
8. **WPT subset no regression** — Given the css-break table print-reftest
   subset through the harness, when re-run, then the pass count does not drop
   below the established table baseline (all affected tables fit one fragmentainer →
   scope `All` → unchanged).
9. **Regression** — Given the existing engine tests (incl. the full
   `engine/tests/tables.rs` suite), when the change lands, then all stay green; the demo
   scoreboard regenerates and table-stress moves from 21 toward 45.

## Edge Cases

- **Scope oscillation.** Wider columns → taller rows → fewer rows fit (k
  shrinks); narrower columns → more rows fit (k grows). The fixed point can
  ping-pong between a wide and a narrow reading of the same first page. The
  hard cap 3 (rule 3) makes the result deterministic — the last computed
  scope freezes; never re-derive beyond the cap.
- Table starts mid-page: the first fragmentainer is the partial page; its
  content height (not the full page height) drives `k`.
- Row spans the fragmentainer boundary: counted (its top edge is on page 1)
  and its whole content contributes to the measure, not just the fragment
  that fits.
- No body rows: scope `All` (header + footer), no panic.
- Table inside multicol: first fragmentainer = first column content area;
  the termination guard must not regress (regression test
  `table_fragments_inside_multicol` stays green).
- Nested tables: the outer table's freeze applies to its own first
  fragmentainer; inner tables resolve their own scope; no interaction.
- Empty table / zero rows: scope `All`, empty `ColumnWidths`, no panic
  (the empty-table rule).
- Freeze moving a table off the overflow branch: the established distribution
  rules still apply verbatim to the frozen intrinsics; no new branch is
  introduced.
- Determinism of the fixed point: iteration order is fixed (pass 1 `All`,
  then `k`-driven), so the same input always takes the same path.

## Verification

1. `cargo build` clean, `cargo test` all green (existing + new tables tests).
2. `python3 scripts/validate_docs.py` OK (this spec + amended
   `auto-table-layout` if touched).
3. Demo regen: table-stress `typeanvil_pages` ≥ 40 in the regenerated
   `demo/corpus/out/scoreboard.json`; two-column probe width unchanged within ±2%
   (char-box extraction, `demo/scripts/col_words.py`).
4. Harness: css-break table subset pass count does not regress from the
   recorded baseline.
5. Record the implementation, verification, and any remaining parity work in
   repository documentation or release notes.

## References

- Measured root cause: Prince first-page freeze, Description ≈57pt = header
  width.
- `auto-table-layout.spec.md` — the algorithm this amends (measure set
  only; distribution unchanged).
- Demo triage measurement evidence: table-stress 21 vs 45.
- Prince ground truth: `/tmp/ts-prince-5x3.pdf` (demo geometry, 45 pages,
  Description ≈57pt measured 2026-08-20), `demo/scripts/col_words.py`
  (char-box word dumps).
- css-tables-3 §10.4.1 (all-rows width — the deviation this spec documents),
  §10.4.2 (distribution, unchanged); CSS2.1 §17.5.2.2.
- `paged-media-css.spec.md` — the bounded `counter(pages)` convergence pattern
  (hard cap 3) used here.
  — the same termination strategy used here.
