---
title: Adjacent Vertical Margin Collapse
type: spec
status: approved
owner: maintainers
created: 2026-08-24
updated: 2026-10-17
slug: /specifications/margin-collapse
sidebar_position: 42
tags: [layout, css-box, margins]
spec_id: margin-collapse
applies_to:
  - engine/src/layout.rs
dependencies:
  - fragmentation-core
---

# Adjacent Vertical Margin Collapse

## Overview

CSS 2.1 §8.3.1 collapses the vertical margins of adjacent siblings: the gap
between two in-flow block boxes is `max(margin-bottom of the first,
margin-top of the second)`, not their sum. TypeAnvil summed them, so every
blockquote/h2 boundary carried a phantom extra gap (measured 6pt on
academic-paper p4; +7pt at every h2 boundary). This spec defines the sibling
collapse the corpus exercises.

Reference finding: `demo/corpus/TRIAGE.md` (minimal-probe evidence,
2026-08-24).

## Goals

- Sibling collapse: `gap = max(mb_prev, mt_this)` between two adjacent
  in-flow block siblings.
- Correct behavior across fragmentainer breaks: no phantom overlap when a
  box resumes on a new page.

## Non-Goals

Full CSS collapse semantics stay out of scope unless the corpus demands
them:

- Empty self-collapsing boxes (a box with no content collapses its own top
  and bottom margins through itself).
- Parent-child collapse of the BOTTOM edge, and top-edge collapse when the
  first child's margin-top exceeds the parent's (the parent fragment would
  have to start above its own cursor). Only the parent-top / first-child-top
  case with child ≤ parent is implemented (Behavior 6, CORE-483).
- Clearance and BFC-isolation exceptions.
- **Float adjacency:** floats do not participate in vertical margin
  collapse. The engine treats a float as a break in the sibling chain — the
  next in-flow block after a float does not collapse against the block
  before it. (The css2.1 spec collapses through floats in some cases;
  Prince parity here is unprobed, so we keep the conservative reading.)

## Behavior

1. The layout of a container's child items SHALL track the previous placed
   in-flow block sibling's `margin-bottom` (`prev_margin_bottom`).
2. Before laying out a fresh (`break_before`) in-flow block child while a
   previous sibling has been placed, the cursor SHALL be reduced by
   `min(prev_margin_bottom, this child's margin-top)` — so the rendered
   gap equals `max(prev_margin_bottom, margin_top)`.
3. The tracker SHALL reset to zero across any bare text run (text between
   blocks is not an adjacent-block boundary) and across float placement.
4. A resumed (!fresh) child SHALL NOT trigger a collapse adjustment: its
   own top margin is already truncated to zero by the fragmentation pass.
5. The first in-flow box on a fragmentainer KEEPS its top margin at a page
   start (css-break-3 §3.1 preserves a fresh box's margin; the former
   comparison-engine truncation did not match the standard);
   no collapse applies.
6. (CORE-483) A fresh in-flow block's top margin SHALL collapse with its
   first in-flow block child's top margin (CSS 2.1 §8.3.1) when the parent
   is `display: block`, not floated, not absolutely/fixed positioned, does
   not clip overflow, has zero `border-top` and `padding-top`, and the
   child's margin-top is positive and ≤ the parent's. The cursor SHALL be
   reduced by the child's margin-top, so the gap equals the parent's
   margin-top (`max`). Probe: `<section style="margin-top:30pt"><ul
   style="margin:12pt 0">` places the text 12pt higher than the summed
   result (Prince: 31.8pt vs 43.7pt before the fix).

## Interfaces

No public API changes. Internal only, in `layout_box`'s item loop
(`engine/src/layout.rs`):

```rust
let mut prev_margin_bottom = Scalar::ZERO;   // reset across text/floats
// before laying a fresh in-flow block child:
y -= min(prev_margin_bottom, style.margin_top);
// after placing a non-empty block child:
prev_margin_bottom = cstyle.margin_bottom;
```

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/margin_collapse.rs`.

- **AC-1 (sum → max):** Given `p { margin-bottom: 7pt }` followed by
  `blockquote { margin-top: 6pt }`, When both render adjacently, Then the
  quote's offset differs from a `margin-bottom: 0` baseline by exactly
  `max(7,6) − max(0,6) = 1pt`. Pre-fix it moved the full 7pt (summed).
  → `adjacent_sibling_gap_is_max_not_sum`.
- **AC-2 (larger top wins):** Given `p { margin-bottom: 5pt }` followed by
  `h2 { margin-top: 20pt }`, adding the p bottom margin moves the heading
  by `max(20,5) − max(20,0) = 0pt`. Pre-fix it moved 5pt.
  → `larger_top_margin_wins_collapse`.
- **AC-3 (tracker reset):** A bare text run between two blocks renders all
  three pieces in order with no collapse carry-through.
  → `text_run_between_blocks_blocks_collapse`.
- **AC-5 (parent/first child):** Given `section { margin-top: 30pt }`
  containing `ul { margin: 12pt 0 }`, adding the ul margin moves the text by
  0pt; with `border-top` on the section it moves 12pt.
  → `first_child_top_margin_collapses_through_parent`,
  `parent_border_blocks_first_child_collapse`.
- **AC-4 (no regression):** Full engine suite stays green; WPT buckets
  (css-page / css-break / css-multicol) show no status regressions vs
  main's binary.

## Edge Cases

- **Page-break resume:** a fresh child after a page break carries
  `prev_margin_bottom = ZERO` from the loop start (the tracker is per-
  fragmentainer), and the resumed child's top margin is already zero — no
  double subtraction. Verified by AC-4's bucket run.
- **Empty blocks skipped:** empty results never update the tracker (they
  don't advance the cursor), matching "adjacent" in the spec sense.
- **Tables/flex as siblings:** they flow through the same item loop, so
  their declared margins collapse identically.

## References

- `engine/src/layout.rs` — the `prev_margin_bottom` tracker and fresh-child
  adjustment.
- CSS 2.1 §8.3.1 (collapsing margins).
- `docs/specifications/fragmentation-core.spec.md` (fragment tree, break
  tokens).
