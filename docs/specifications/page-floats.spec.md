---
title: "Page floats — float: top/bottom/next-page/snap"
type: spec
status: approved
owner: maintainers
created: 2026-09-13
updated: 2026-09-27
sidebar_position: 30
tags: [css-page, floats, pagination]
spec_id: SPEC-PAGE-FLOATS
slug: /specifications/page-floats
---

# Page floats — `float: top | bottom | next-page | snap`

Extends the css-floats specification with the
page-float family from css-page-3 §"page floats" / css-page-4
§"page-floats". Prince's book and report templates pin figures to page
edges with these values; the inline float model cannot express them.

## Parsing

1. The `float` property SHALL accept `top`, `bottom`, `next-page`, and
   `snap` in addition to `none`, `left`, `right`, and `footnote`.
2. `snap` SHALL resolve to `top` for v1 (nearest-edge resolution; the
   `snap-block`/`snap-inline` variants are out of scope).
3. The keywords SHALL parse in both the stylo seam and the hand-rolled
   paged-props pass (stylesheet rules and inline `style=""`), following
   the existing `float: footnote` parsing pattern.

## Placement

4. A page float SHALL NOT join the line-level float lanes. It pins its
   margin box to the page content box's top edge (`top`, `snap`,
   `next-page` on its destination page) or bottom edge (`bottom`).
5. Top-edge floats stack DOWN from the content-box top in document
   order; bottom-edge floats stack UP from the content-box bottom in
   document order. Multiple floats on one edge SHALL NOT overlap.
6. A page float lays out monolithic (no internal fragmentation). A page
   float whose band height exceeds the page content height places
   clipped on its page rather than looping (v1 deviation from
   css-page-4 §2.3, documented here).
7. A `top`/`bottom` float that cannot fit the remaining space on the
   current page SHALL defer whole to the next fragmentainer and place
   there (deferral is marked once; a deferred float never defers
   twice, so pagination always progresses).
8. `next-page` SHALL defer on first encounter regardless of available
   space and behaves as a `top` float on its destination page
   (css-page-4 §2.5 simplified: the destination is the next
   fragmentainer, not a page of the same name).

## Interaction with in-flow content

9. In-flow text SHALL NOT be width-shortened beside a page float
   (`segment_geometry` ignores page-float bands for the left/right
   lanes). Text after a `top` float starts BELOW the float's band; a
   `bottom` float's band hangs at the page bottom edge and does not
   move the in-flow cursor.
10. `float: footnote` SHALL keep its current behavior; it is
    a distinct area selection, not a page-float band.

## Acceptance criteria

- A `float: top` figure renders pinned at the content-box top of its
  page with body text below (never beside) it.
- Two `float: top` figures stack in document order without overlap.
- A `float: bottom` figure's band ends at the content-box bottom edge.
- A `float: next-page` figure never renders on its originating page.
- A `float: top` figure that does not fit the current page defers and
  pins at the next page's top.
- `float: snap` behaves as `float: top`.
- Footnote rendering is unchanged.
- Full WPT suite: zero regressions.
- `cargo test` green, including `engine/tests/page_floats.rs`.
