---
title: Multi-Column Layout
slug: /specifications/multicol
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-08-19
sidebar_position: 8
tags: [engine, layout, css-multicol, fragmentation]
spec_id: multicol
issue_id: CORE-63
applies_to: engine 0.x
dependencies: [fragmentation-core, paged-media-css, wpt-conformance-harness]
---

# Multi-Column Layout

## Overview

css-multicol (744 WPT files) is the largest conformance surface after
css-page. Three hard parts: **balancing** (column heights made as equal as
possible — needs measure passes), **spanners** (an element spanning all
columns interrupts the column flow), and **nested multicol** (multicol under
print constrains inner content by every enclosing context).

The CORE-51 model already generalizes to this: fragmentainers are first-class
fragments with no source box, and the fragmentation-core spec's goal #3 names
columns explicitly as a future fragmentainer kind. The engine today has no
multicol support — the only "column" code is table-column width measurement.

**Path chosen: stylo for the longhands, engine for the layout.** Verified
2026-08-18 against stylo 0.20.0 `properties/longhands.toml`:

- `column-count`, `column-width`, `column-span` — compiled in the servo build
  but **pref-gated** (`servo_pref = "layout.columns.enabled"`): the engine must
  enable that pref in its stylo runtime pref plumbing (mechanism to verify at
  implementation; the pref gates whether the properties are honored).
- `column-gap`, `row-gap`, `column-rule-color/style/width` — compiled, no
  gates.
- `column-fill` — **gecko-only**: the servo build does not compile it.
  `balance` is therefore the default and the only promised fill mode
  (`column-fill: auto` is a non-goal until the servo build gains the
  longhand).

**Fitness function:** css-multicol WPT print-reftests via the harness — a
growing subset (basic balance → spanning → nested), 744 files total is the
fitness target.

## Goals / Non-Goals

**Goals**

- Column geometry: `column-count` / `column-width` resolution via the css
  multicol auto algorithm (width as ideal column width, count as the cap).
- Columns as fragmentainers: content flows into column fragmentainers in
  inline progression within one page, block progression across pages.
- **Balanced columns**: measure passes distribute content so column heights
  are as equal as possible; the number of passes is bounded (constant per
  layout).
- **Spanners** (`column-span: all`): content before a spanner occupies
  columns, the spanner spans the full content width, content after resumes in
  fresh columns.
- **Nested multicol**: an inner multicol container is constrained by every
  enclosing fragmentainer context (natural consequence of nested
  fragmentainers in the tree).
- Multicol container fragmentation: when the container itself breaks across
  pages, the remaining content resumes on the next page as a fresh balanced
  set (sequential-sets semantics), never slicing a line.
- Determinism: geometry resolved deterministically; measure passes bounded.

**Non-Goals** (deferred; scope stays honest)

- `column-fill: auto` (gecko-only longhand in stylo 0.20 — see above).
- `columns` shorthand beyond what stylo computes from the two longhands.
- Floats (CORE-62), abspos (CORE-64), or flex (CORE-65) inside multicol — the
  interaction specs land after the individual features.
- **Column rules** (`column-rule-*`): the longhands compute cleanly but rule
  painting in the gap is deferred; the gap renders as background this pass.
- Vertical writing modes; `column-span` values other than `none | all`;
  regions or multicol-in-margin-boxes.

## Behavior

The engine shall:

1. Compute `column-count`, `column-width`, `column-span`, `column-gap`, and
   `column-rule-*` at cascade time from stylo (with the
   `layout.columns.enabled` pref enabled so the servo build honors them):
   `ComputedStyle` gains the resolved column fields, read in
   `css.rs::convert`.
2. Create a **column fragmentainer** for a box with a resolved
   `column-count > 1` (or `column-width` set): columns are fragmentainers in
   the fragment tree, inline-progressing within the page, block-progressing
   across pages.
3. Resolve column widths with the css-multicol auto algorithm: column-width
   is the ideal width, column-count the cap; gaps (`column-gap`, default
   `1em`) sit between columns; the last column may be narrower.
4. **Balance columns**: run bounded measure passes so the columns'
   content heights differ by at most one line box; the number of passes is
   constant (not proportional to content); a single pass when content is
   shorter than one column.
5. **Honor spanners**: an element with `column-span: all` interrupts the
   column flow — preceding content finishes its columns, the spanner spans
   the full content width, following content starts a new set of balanced
   columns.
6. **Nest multicol**: an inner multicol's columns are constrained by the
   enclosing column's measure; its fragmentainers nest inside the outer
   fragmentainer chain.
7. **Fragment multicol containers**: when the container breaks across pages,
   the remaining content resumes on the next page as a fresh balanced set of
   columns (the container's break token carries the consumed children's
   tokens); a column never slices a line.
8. Reuse the CORE-51 rules inside columns: `break-inside: avoid`, widows,
   orphans, and forced breaks apply per column fragmentainer.
9. Stay deterministic: column geometry is a pure function of measure; measure
   passes are bounded and deterministic.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle`:

  ```rust
  /// css-multicol longhands, resolved at cascade time (stylo).
  pub column_count: Option<u32>,   // `auto` → None
  pub column_width: Option<Scalar>, // `auto` → None (pt)
  pub column_span: ColumnSpan,     // None | All (stylo)
  pub column_gap: Scalar,          // default 1em resolved against font-size
  ```

- The fields are read from stylo's **Column** style struct
  (`values.get_column()`): `clone_column_count()`,
  `clone_column_width()`, `clone_column_span()`, `clone_column_gap()` —
  all compiled in the servo build (verified 2026-08-19).

- `ComputedStyle::initial()`: `column_count: None`, `column_width: None`,
  `column_span: ColumnSpan::None`, `column_gap: font_size`.

- **Pref gate**: `column-count`/`column-width`/`column-span` are gated at
  parse time by `static_prefs::pref!("layout.columns.enabled")`. The engine
  depends on `static_prefs` (package `stylo_static_prefs`, the same crate
  stylo uses; the pref table ships in its `preferences.toml`) and calls
  `static_prefs::set_pref!("layout.columns.enabled", true)` before any
  cascade or stylesheet parse (verified 2026-08-19).

### `engine/src/layout.rs` / new `engine/src/multicol.rs`

- `multicol.rs` implements: column geometry resolution, the balancing measure
  pass (bounded), column fragmentainer creation, spanner interruption, and
  nesting.
- `layout.rs` dispatches to `multicol.rs` when a box's computed
  `column_count/width` requests columns; otherwise the existing block path is
  unchanged.

### `engine/src/frag.rs`

- Reuse the `Fragmentainer` abstraction for columns (CORE-51 goal #3). If the
  existing fragmentainer type is page-bound (has page geometry), introduce a
  `Columnainer` that shares the break-token machinery and adds inline
  progression; the PDF pass maps column fragmentainers to page coordinates.

### `engine/src/pdf.rs`

- Paint columns in order within their page; column rules painted from
  `column-rule-*`.

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/multicol.rs` (helpers mirror
`tests/typography.rs`).

1. **Geometry.** Given `column-count: 3`, a document with a full page of text
   renders three columns of equal width separated by the `column-gap`.
2. **Balance.** Given unbalanced-length paragraphs in a 3-column container,
   the columns' content heights differ by at most one line box.
3. **Spanner.** Given `h2 { column-span: all }` mid-flow, content above the
   heading occupies columns, the heading spans the full content width, and
   content below resumes in new balanced columns.
4. **Nested.** Given a multicol container inside a multicol column, the inner
   container's width equals the outer column's content width (constrained by
   every enclosing context).
5. **Fragment across pages.** Given a multicol container taller than a page,
   the remaining content resumes on the next page as a fresh balanced set of
   columns; no column slices a line. Partial fills anchor columns at the
   container's current content cursor, so content taller than the page
   (e.g. a long table) progresses instead of re-laying its first row forever
   (CORE-78 regression `table_fragments_inside_multicol`: a 12-row
   `break-inside: avoid` table in `column-count: 2` renders 2-3 pages and
   terminates).
6. **Breaks inside columns.** Given `break-inside: avoid` on a box inside a
   column, the box moves to the next column as a unit.
7. **css-multicol WPT subset.** A growing subset of css-multicol print-reftests
   (basic balance, spanning, nested) passes via the harness.
8. **Determinism.** Two renders of a multicol doc are byte-identical; the full
   existing engine test suite still passes.

## Edge Cases

- `column-count: auto` + `column-width: auto` → single column = block layout
  (no behavior change, no fragmentainer overhead).
- `column-count: 1` → single column, still a column fragmentainer only when
  the pref/geometry demands it; no spanner support needed.
- Column width larger than the content box: clamped to the content box.
- Zero-height column content: no panic; zero-height columns allowed.
- Spanner as the first/last child: no preceding/following column flow — the
  spanner spans an empty flow.
- Content shorter than one column with `column-count: 3`: one column with
  content, two empty (balanced).
- A set whose balanced estimate over-ran the page but whose content actually
  finished inside the page's columns is DONE: the container advances past
  the laid columns instead of fragmenting with an empty resume token (a
  trailing blank page) or re-laying the same set with no cursor advance
  (infinite pagination).

## References

- css-multicol-1: https://drafts.csswg.org/css-multicol-1/
- css-break-3 (fragmentation inside columns):
  https://drafts.csswg.org/css-break-3/
- Fragmentainer generalization this builds on: `fragmentation-core.spec.md`
  (goal #3 explicitly names columns)
- Page geometry / margins: `paged-media-css.spec.md`
- Research brief: `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
- Parent epic: Linear CORE-54.
- Table × multicol infinite-pagination bug and fix: Linear CORE-78.
- stylo 0.20.0 `properties/longhands.toml` (verified 2026-08-18):
  `column-count`/`column-width`/`column-span` compiled but pref-gated
  (`layout.columns.enabled`); `column-gap`/`row-gap`/`column-rule-*` compiled;
  `column-fill` gecko-only.
