---
title: Named Pages — page Property Propagation, Selection, and Change Breaks
slug: /specifications/named-pages
type: spec
status: draft
owner: elijah
created: 2026-09-04
updated: 2026-09-14
sidebar_position: 25
tags: [engine, css-page, paged-media, layout, breaks]
spec_id: named-pages
issue_id: CORE-127
applies_to: engine 0.x
dependencies: [paged-media-css]
---

# Named Pages — `page` Property Propagation, Selection, and Change Breaks

## Overview

Slice (a) of the CORE-127 epic: the `page-name-*` WPT family (~30 failures at
the 2026-08-31 baseline, engine `fd43280`). Named-page selection at page
starts existed since CORE-82 (`active_page_name`); what was missing is the
FORCED BREAK between boxes whose page context changes (css-page-3 §4.2
"Using named pages"). A first CORE-66-era attempt compared declared `page`
values and regressed 38 tests; this spec records the leaf-context model that
the WPT refs actually encode.

## Goals

1. A page break is forced between in-flow siblings whose effective page
   contexts differ, so `page-name-siblings-*` and `page-name-propagated-*`
   match their refs.
2. The CORE-82 page-selection model (which named `@page` rule styles each
   page) is unchanged.
3. Zero regressions across css-page and css-break.

## Non-Goals

1. `<br>` is not implemented (CORE-159). The tokenizer re-flows text runs with
   `split_whitespace()`, so no forced line break exists to honor. This is a
   text-runs feature, not a named-page one; the boundary rule below no longer
   depends on it. `page-name-002` cannot pixel-match its reference until it
   lands, because that reference places "3rd page" and "Also 3rd page" on one
   line via `<br>`.
   `page-name-003` stays a Non-Goal permanently:
   it and `page-name-abspos-002` are structurally identical tests with
   opposite references (one requires a break inside the abspos wrapper, the
   other requires none). No engine can pass both. The engine currently
   matches `page-name-abspos-002`; Chromium matches `page-name-003`.
2. Orthogonal flow support: a page-change break is suppressed when the writing
   mode IN EFFECT at the page-declaring boxes is orthogonal to the PAGE's own
   flow mode (the engine paginates every flow horizontally in v1, so such a
   subtree cannot reproduce a vertical mode's pagination; the
   orthogonal-writing-001/003 refs render one page through this engine, so
   suppression matches them). Full orthogonal-flow pagination is out of scope.
3. Page-change breaks inside flex containers, inline-blocks, or between
   bare inline runs (atomic interiors and flex items are not page-grouped).
4. Residuals with per-test root causes; do not chase via this spec.
   `page-name-zero-height-001` (engine renders 3 pages; Chrome/Edge also
   FAIL this test at 6 pages, Firefox passes — no agreed browser model).
   `page-name-002` is characterised by the boundary study recorded under
   Non-Goal 1 and is actionable; `page-name-003` is permanently unsatisfiable
   (see Non-Goal 1). Fixed by the two CORE-157 slices:
   `page-name-img-001/002` (an inline-level replaced image's own `page`
   declaration is inert — Behavior 6), `page-name-display-none-child` and
   `page-name-inline-block-002` (the boundary comparison now also runs on
   the placed atomic path — Behavior 3), and `root-element-display-none`
   (Behavior 7). `page-name-orthogonal-writing-002` also flipped PASS in
   the second slice (Behaviour 3's class-A-aware atomic comparison applies
   inside the writing-mode subtree test).
5. Harness-geometry-limited residuals: `basic-pagination-003`,
   `page-background-004/005` declare their own `@page size`; the harness
   renders every fixture at its fixed 5x3in/0.5in geometry, and the
   Chromium oracle leg fails them for the SAME reason — a fixture-geometry
   harness leg is needed before these can be judged.
6. No-ground-truth residual: `safe-printable-inset-001/002/003` (tentative
   `page-margin-safety`, css-page-3 §7.6 ED proposal) — Chrome, Edge and
   Firefox all FAIL on wpt.fyi master (2026-09-09); Safari does not run the
   tentative css-page suite. Documented, not chased.
7. Inline-style seam (bonus fix, verified by this slice's gate): the stylo
   cascade ignores inline `style=""` declarations for `display`, `position`,
   and `float`. The paged-media pass now carries them
   (`PagedDecl::Display/Position/FloatSide`, inline only, `float: footnote`
   preserved).
8. Accepted flips (net +16, gate trade-off, each root-caused):
   `monolithic-overflow-005/013` — the inline `position:absolute` seam makes
   the test's abspos wrapper monolithic (clipped) where the ref's in-flow
   wrapper fragments; both need `contain:size` fragmentation support.
   `page-name-canvas-004` — `<canvas>` is not a replaced element yet; the
   following unnamed div resolves its page context through the canvas.
   Undeclared-box context otherwise follows ancestors, with stickiness ONLY
   from a preceding replaced sibling (canvas-004 oracle).

## Behavior

1. **Effective page.** A box's page context is its own non-auto `page`
   declaration, else the page name of the nearest PRECEDING in-flow
   replaced sibling that declared one (stickiness applies through replaced
   elements only), else the nearest ancestor-or-self with one, else the
   default page (existing CORE-82 model; `effective_page`).
2. **Content leaf.** A box's page-boundary position is its FIRST (or LAST)
   in-flow content leaf: descend through in-flow block children, skipping
   out-of-flow boxes (float, absolute, fixed), `display: none`, and
   zero-height (`height: 0`) boxes. A qualifying block with no qualifying
   children is itself the leaf. A flex container (row or column) is itself
   a leaf — its items are not page-grouped.
3. **Sibling change break.** After an in-flow block child places content,
   layout finds the next content-bearing in-flow sibling (skipping
   out-of-flow, display:none, and zero-height blocks) and compares the
   just-placed child's
   last-leaf context against the target's first-leaf context — the
   just-placed child itself substitutes when it has no qualifying leaf (a
   replaced or empty box IS its own leaf). A difference DEFERS a forced
   break-before at the target's item index: the loop keeps laying items
   between (out-of-flow floats still land on the current page in document
   order — page-name-float-002) and fires the break only when the loop
   reaches the target.

   The comparison also runs on the **placed atomic path** (an inline-block
   item), and the placed side resolves its context through
   `context_effective_page`: an inline-level box is not class A, so its own
   `page` is inert and its context comes from the ancestor chain
   (`page-name-inline-block-002` — the `page:c` inline-block inherits the
   default context, so the following `page:c` block differs and breaks).

   The comparison is skipped only for a **resume-empty wrapper whose
   subtree still holds content**: it finished its children on an earlier
   page (a forced break-after fired before it, and-break-003), so the
   boundary it would demand already fired and breaking again would add a
   blank page. A genuinely contentless box (no leaf anywhere — e.g. a
   wrapper holding only `display: none` children) does NOT skip: it hosts
   its own boundary (`page-name-display-none-child`).

    A **bare text run IS in-flow content** and is a valid target on either side
    of the comparison (CORE-158). It takes its containing block's effective page
    context, so `[div page:a]A[/div] X [div page:a]C[/div]` is three pages
    (a -> default -> a), and a trailing run after a named page adds one more.
    The itemizer has already dropped whitespace-only runs, so a surviving
    `Item::Text` holds real content. The comparison therefore also runs from the
    TEXT arm, not only after a block child: without that, a boundary AFTER a run
    was never compared.
4. **Suppressions.** No page-change break fires: inside an inline-block
   (atomic interior), when the writing mode IN EFFECT at the page-declaring
   boxes is ORTHOGONAL to the page's own flow mode, or when the subtree below
   either side holds no in-flow content. The mode in effect at a box is the
   nearest ancestor-or-self `writing-mode` declaration
   (`ComputedStyle::writing_mode_declared`, set by the paged-media pass for
   both stylesheet and inline declarations), else the page's flow mode; the
   page's own flow mode is the ROOT element's `writing-mode` (css-page-3 §3).
   The two modes are compared by AXIS, not by "is any declaration present":

   - A declaration on the ROOT element establishes the page's own flow, so
     root-level sibling page changes still break
     (`page-name-orthogonal-writing-002`).
   - A wrapper that switches the mode and then switches it BACK to the page's
     own mode is not an orthogonal context, so its page change still breaks
     (`page-name-orthogonal-writing-004`: `horizontal-tb` inside
     `vertical-rl` under an htb page; its ref forces the same break with
     `break-after: page`, and `page_boundaries.rs`
     `page_change_breaks_when_inner_mode_matches_page_flow` pins it).
   - A subtree whose mode stays orthogonal to the page's keeps the
     suppression (`page-name-orthogonal-writing-001/003`;
     `page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow`).

5. **Selection unchanged.** Page geometry (which `@page` rule applies) is
   still resolved per page start by `active_page_name` (CORE-82).
6. **Class-A applicability of `page`.** The `page` property applies only to
   boxes that create class A break points (css-page-3 §8.1). An inline-level
   replaced image (`<img>`, inline `<svg>`) creates none, so its own `page`
   declaration is inert: it neither starts a page for the image nor demands a
   boundary at it (Chromium oracle: the image stays on its ancestor's page
   and a following `page:b` block is the box that breaks —
   `page-name-img-001/002`). A block-level replaced box (`display: block`)
   IS class A and keeps its declaration (`page-name-img-003/004`).
7. **Root-element `display: none`.** A `display: none` on the root ELEMENT
   (`html`) suppresses the document: one valid empty page with NO page-box
   chrome, which compares equal to a blank reference
   (`root-element-display-none`; CORE-66). The check must read the html
   element's computed display — `dom.root` is the synthetic document node,
   never element-styled, so testing it never fires. A `display: none` CHILD
   generates no box and its text must not fold into the parent's run
   (`collect_items_rec`).

## Interfaces

- `engine/src/layout.rs`: `Ctx::effective_page`, `Ctx::page_context_leaf`,
  `Ctx::orthogonal_flow`; the sibling-comparison block in `layout_box`'s
  in-flow `Item::Block` branch.
- `engine/src/css.rs`: `ComputedStyle::writing_mode_declared`;
  `PagedDecl::WritingMode` parsed by the paged pass (stylesheet + inline).

## Acceptance Criteria

Each maps to a live WPT test in the harness (`harness run --filter css-page`):

- Given siblings `page:a` | `page:b` | default, a break fires at each change
  (`page-name-siblings-001/003/004/005`).
- Given a wrapper whose declared page differs from its first content leaf's
  effective page, the LEAF decides (`page-name-propagated-001` renders one
  page; `page-name-propagated-003/005` render one page).
- Given a page-declaring box after out-of-flow or zero-height siblings, no
  break is demanded through them (`page-name-float-001/002`,
  `page-name-abspos-001/002/003`).
- Given page-declaring flex items, no break fires inside the container
  (`page-name-flex-001/002` one page; `flex-003/004` break only where the
  ref forces it outside/below the container).
- Given an inline-block interior or an orthogonal-to-the-page writing-mode
  subtree, no break fires (`page-name-inline-block-001/003`,
  `page-name-orthogonal-writing-001/003`).
- Given a mode-switching wrapper whose innermost mode matches the page's own
  flow, the page change still breaks
  (`page-name-orthogonal-writing-004`; `page_boundaries.rs`
  `page_change_breaks_when_inner_mode_matches_page_flow` renders two pages).
- Given two `page:a` blocks separated by a bare text run, the render is three
  pages — a, default, a (CORE-158; `engine/tests/page_boundaries.rs`
  `p3_named_text_named_breaks_twice`).
- Given a named page followed by trailing bare text, the render is two pages
  (`p4_named_then_text_breaks`).
- Given leading bare text followed by a named page, the render is two pages
  (`p5_text_then_named_breaks`).
- Given an inline-level replaced image with `page: a` inside a default-page
  flow, no break fires for the image and an undeclared follower stays on the
  default page (`page-name-img-001/002`).
- Given `html { display: none }`, the render is one blank page without
  page-box chrome (`root-element-display-none`).

## Edge Cases

- A wrapper whose qualifying descendants are all out-of-flow has no leaf:
  it is skipped as a comparison target (contentless wrapper).
- The comparison is skipped when the placed child is empty (a wrapper that
  finished with a forced internal break coalesces instead of adding a blank
  page — and-break-003).
- Resume passes: the comparison only runs on a fresh placement (`!res.empty`
  and the loop reaching the child), so a break never re-fires against
  content already resumed on a new page.

## References

- css-page-3 §4.2 (Using named pages) — the forced-break requirement.
- CORE-82 `active_page_name` (page selection at page starts).
- CORE-66's declared-value attempt (38-test regression; superseded model).
- Linear: CORE-127 slice (a).
