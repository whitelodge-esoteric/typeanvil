---
title: Table Cell Vertical Alignment
slug: /specifications/table-cell-vertical-align
type: spec
status: approved
owner: maintainers
created: 2026-10-16
updated: 2026-10-16
sidebar_position: 14
tags: [layout, tables, css-tables, engine]
spec_id: table-cell-vertical-align
issue_id: CORE-352
applies_to: engine 0.x
dependencies: [tables-fragmentation]
---

# Table Cell Vertical Alignment

## Overview

CSS 2.1 section 17.5.4 lets a table cell position its content inside the row
height with `vertical-align`. The HTML UA stylesheet sets `middle` on
`thead, tbody, tfoot, tr`, and cells inherit it. Before CORE-352 the engine
laid every cell's content at the cell top.

## Behavior

1. A cell shall resolve `vertical-align` from its own declaration (stylesheet
   or inline `style`), else its row, else its row group, else `middle`.
2. The accepted keywords shall be `top`, `middle`, `bottom`, `baseline`.
   Other values shall be ignored.
3. In a row that lays out whole on one fragmentainer, a cell shorter than the
   row shall shift its content down by half of the spare height (`middle`) or
   all of it (`bottom`). Cell borders and backgrounds shall still fill the
   whole row.
4. `top` shall keep the content at the cell top.
5. `baseline` cells shall align their first-line baseline to the deepest
   first-line baseline among the `baseline` cells of the row (CSS 2.1
   section 17.5.4). The shift shall not exceed the cell's spare height.

## Chromium comparison (measured 2026-10-16)

Original repro (`cell-valign.html`, corpus geometry), text top y in pt:

| Cell   | TypeAnvil | Chromium | Delta |
|--------|-----------|----------|-------|
| TOP    | 40.5      | 40.7     | 0.2   |
| MID    | 62.1      | 61.7     | 0.4   |
| BOT    | 83.7      | 82.7     | 1.0   |

The 1.0pt BOT delta comes from line metrics. With `line-height: 11pt` pinned
on the same repro, TypeAnvil BOT is 84.6 and Chromium 84.2 (0.4). The row
spare height is then equal. Without a pinned `line-height`, TypeAnvil's
`normal` line box (1.2 times the font size) differs from Chromium's font-based
`normal` line box. That difference is outside this feature.

Baseline probe (`probe/ba.html`: `BIG` 24pt beside `small` 10pt, both
`baseline`, `line-height: normal`): TypeAnvil puts both baselines at page y
64.50 (28.5pt below the content top). Chromium's DOM probe puts both at 27.75pt
below the content top. Both engines share one baseline across the two cells.
The 0.75pt offset is the same `normal` line-metrics difference as above. Before
this change the `small` cell sat at the cell top, about 13pt above the shared
baseline. Test: `core352_cell_valign_more.rs::baseline_cells_share_first_line_baseline`.

## Non-Goals

- Alignment of a row that splits across fragmentainers. Each fragment keeps
  its cells at the top.

## Acceptance Criteria

- Given a row with a tall cell and cells with no declaration, `top` and
  `bottom`, when laid out, then the default cell sits below the `top` cell and
  the `bottom` cell sits below the default cell.
  Test: `engine/tests/core352_cell_valign.rs::cells_honor_vertical_align`.
- Given a bottom cell beside a 3-line cell, when laid out, then the bottom
  cell's baseline equals the tall cell's last baseline.
  Test: `core352_cell_valign_more.rs::bottom_cell_baseline_equals_last_line_baseline_of_tall_cell`.
- Given `tr { vertical-align: top }`, when laid out, then its cells sit at the
  top. Test: `row_level_vertical_align_is_inherited_by_cells`.

## References

- CSS 2.1 section 17.5.4; CORE-352; `tables-fragmentation.spec.md`.
