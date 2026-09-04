---
title: Named Pages — page Property Propagation, Selection, and Change Breaks
slug: /specifications/named-pages
type: spec
status: draft
owner: elijah
created: 2026-09-04
updated: 2026-09-04
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

1. Bare-text page-context changes (a text run between two page-declaring
   blocks never forces a break): fixedpos-010's ref keeps trailing text on
   the named page, while page-name-002's ref would require the break — the
   two refs contradict each other through this engine; page-name-002 stays
   a documented residual until a Chromium-oracle study resolves the quirk.
2. Orthogonal flow support: documents with explicit `writing-mode`
   declarations suppress page-change breaks entirely (the engine paginates
   every flow horizontally in v1; the orthogonal-writing refs render one
   page through this engine, so suppression matches them).
3. Page-change breaks inside flex containers, inline-blocks, or between
   bare inline runs (atomic interiors and flex items are not page-grouped).
4. `page-name-002/004/006/007`, `page-name-zero-height-001` (inline
   `height: 0` is unread by the style seam), `page-name-display-none-child`,
   `page-name-img-001/002`, `page-name-inline-block-002`: residuals with
   per-test root causes; do not chase via this spec.
5. Inline-style seam (bonus fix, verified by this slice's gate): the stylo
   cascade ignores inline `style=""` declarations for `display`, `position`,
   and `float`. The paged-media pass now carries them
   (`PagedDecl::Display/Position/FloatSide`, inline only, `float: footnote`
   preserved).
6. Accepted flips (net +16, gate trade-off, each root-caused):
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
3. **Sibling change break.** After an in-flow block child places content
   (`!res.empty`), layout finds the next content-bearing in-flow sibling
   (skipping atomics, out-of-flow, display:none, and zero-height blocks;
   a bare text run terminates the search contextless) and compares the
   just-placed child's last-leaf context against the target's first-leaf
   context — the just-placed child itself substitutes when it has no
   qualifying leaf (a replaced or empty box IS its own leaf). A difference
   DEFERS a forced break-before at the target's item index: the loop keeps
   laying items between (out-of-flow floats still land on the current page
   in document order — page-name-float-002) and fires the break only when
   the loop reaches the target.
4. **Suppressions.** No page-change break fires: inside an inline-block
   (atomic interior), in any subtree whose ancestor-or-self carries an
   explicit `writing-mode` declaration (`ComputedStyle
   ::writing_mode_declared`, set by the paged-media pass for both
   stylesheet and inline declarations), when the placed child was empty,
   or when the subtree below either side holds no in-flow content.
5. **Selection unchanged.** Page geometry (which `@page` rule applies) is
   still resolved per page start by `active_page_name` (CORE-82).

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
- Given an inline-block interior or a writing-mode subtree, no break fires
  (`page-name-inline-block-001/003`, `page-name-orthogonal-writing-001/002/003`).

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
