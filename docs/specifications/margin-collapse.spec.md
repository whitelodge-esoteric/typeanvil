---
title: Adjacent Vertical Margin Collapse
type: spec
status: approved
owner: Elijah Boston
created: 2026-08-24
updated: 2026-08-24
slug: /specifications/margin-collapse
sidebar_position: 42
tags: [layout, css-box, margins, core-118]
spec_id: SPEC-CORE-118-margin-collapse
issue_id: CORE-118
applies_to:
  - engine/src/layout.rs
dependencies:
  - SPEC-CORE-51-fragmentation-core
---

# Adjacent Vertical Margin Collapse (CORE-118)

## Overview

CSS 2.1 §8.3.1 collapses the vertical margins of adjacent siblings: the gap
between two in-flow block boxes is `max(margin-bottom of the first,
margin-top of the second)`, not their sum. TypeAnvil summed them, so every
blockquote/h2 boundary carried a phantom extra gap (measured 6pt on
academic-paper p4; +7pt at every h2 boundary). This spec defines the sibling
collapse the corpus exercises.

Reference finding: `demo/TRIAGE.md` §CORE-110 (minimal-probe evidence,
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
- Parent-child collapse through unbroken top/bottom edges.
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
5. The first in-flow box on a fragmentainer keeps existing behavior
   (top margin truncates via `first_in_flow`, CORE-95); no collapse
   applies.

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

- Linear: CORE-118 (finding filed from CORE-110 triage).
- CSS 2.1 §8.3.1 (collapsing margins).
- `docs/specifications/fragmentation-core.spec.md` (fragment tree, break
  tokens).
