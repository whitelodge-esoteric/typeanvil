---
title: box-decoration-break
slug: /specifications/box-decoration-break
type: spec
status: approved
owner: maintainers
created: 2026-10-17
updated: 2026-10-17
sidebar_position: 40
tags: [layout, fragmentation, borders, tables, css-break, engine]
spec_id: box-decoration-break
issue_id: CORE-457
applies_to: engine 0.x
dependencies: [fragmentation-core, tables-fragmentation]
---

# box-decoration-break

## Overview

`box-decoration-break` (css-break-3 §5.4) controls how a box's border and
padding apply when the box splits across fragmentainers. The value `slice`
is the initial value. The value `clone` repeats the box's decoration on every
fragment.

## Goals / Non-Goals

Goals:

- Support `slice` and `clone` on block containers and on tables.
- Reserve the cloned bottom chrome in the fragmentation limit.
- Parse the logical `border-block-start/end(-width)` properties (horizontal
  writing mode: block-start is top, block-end is bottom).

Non-Goals (follow-ups):

- Inline boxes split across lines. They stay `slice`.
- Per-fragment `border-radius` and `box-shadow` changes. The engine has no
  radius painting for fragments yet.
- Table rows, row groups, and cells. They stay `slice`.
- Vertical writing modes for the logical border properties.
- Block containers with a declared `height`. They stay `slice`: their
  continuation counts consumed block size against the declared extent, and
  cloned chrome moved `css-page/monolithic-overflow-031` from 5 to 8 pages.

## Behavior

1. The engine shall parse `box-decoration-break: slice | clone` in the border
   author pass. The property is not inherited. Any other value is ignored.
2. With `slice`, a fragmented box shall paint its top border and top padding
   on the first fragment only. It shall paint its bottom border and bottom
   padding on the last fragment only. Side borders paint on every fragment.
3. With `clone`, a fragmented block container shall start every fragment with
   its top border and top padding, and end every fragment with its bottom
   padding and bottom border.
4. With `clone`, the engine shall reserve the bottom padding and bottom
   border out of the fragment's block limit, so child content never overlaps
   the cloned bottom chrome.
5. With `clone`, a fragmented table shall repeat its top border and padding,
   and its bottom padding and border, on every fragment. The table shall
   reserve the bottom chrome in the row limit.
6. Table border-spacing (separate model) shall stay truncated at a break. The
   first fragment has spacing above its first row. The last fragment has
   spacing below its last row. A continuation fragment starts at border plus
   padding.
7. The backgrounds of a box shall paint under the cloned chrome on every
   fragment (already true for both values).
8. The engine shall treat `border-block-start`, `border-block-start-width`,
   `border-block-end`, and `border-block-end-width` as the top and bottom
   border declarations.

## Interfaces

- `ComputedStyle::box_decoration_clone: bool` (default `false`).
- Parsed in `css.rs` `mod borders` (`BorderDecl::DecorationBreak`).
- Applied in `layout.rs` `layout_box` and `layout_table_block`.

## Acceptance Criteria

- Given a bordered block that spans three pages, When `box-decoration-break`
  is `clone`, Then every page paints a top and a bottom border band. Test:
  `engine/tests/it/core457_box_decoration_break.rs::clone_block_paints_top_and_bottom_on_every_fragment`.
- Given the same block with `slice`, Then only page 1 paints a top band and
  only the last page paints a bottom band. Test: `slice_block_paints_top_on_first_and_bottom_on_last_only`.
- Given a cloned block, Then no page content passes the content box. Test:
  `clone_block_reserves_bottom_chrome_inside_the_page`.
- Given a cloned and a sliced table, Then the same rules hold. Tests:
  `clone_table_paints_top_and_bottom_on_every_fragment`,
  `slice_table_keeps_top_first_bottom_last`.
- Given `border-block-start-width: 5pt; border-block-end-width: 7pt`, Then
  the top border is 5pt and the bottom border is 7pt. Test:
  `logical_border_block_widths_map_to_top_and_bottom`.
- Given WPT `css-break/table/table-fragmentation-003c-print.html`, Then the
  test and its reference both render 3 pages with the black band ending at
  180pt on every page.

## Edge Cases

- A cloned box whose chrome is taller than the fragmentainer cannot fit any
  content. It follows the normal monolithic rules.
- A declared `height` still resolves across fragments by the existing
  consumed-block-size model.

## References

- css-break-3 §5.4 `box-decoration-break`.
- CORE-457, CORE-371 (unblocked by this spec).
- `tables-fragmentation` spec (slice semantics, CORE-235).
