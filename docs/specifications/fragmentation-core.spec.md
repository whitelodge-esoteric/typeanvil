---
title: Fragmentation Core
slug: /specifications/fragmentation-core
type: spec
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 2
tags: [layout, fragmentation, css-break, engine]
spec_id: fragmentation-core
issue_id: CORE-51
applies_to: engine 0.x
dependencies: [wpt-conformance-harness]
---

# Fragmentation Core

## Overview

The moat. LayoutNG's model, greenfield: layout is a **pure function**
`(node, constraints, break_token) → immutable fragment`; the fragment tree is
the sole output of layout — one fragment per box *per fragmentainer it lands
in*; break tokens make pagination resumable (O(n), no relayout-from-scratch);
break-appeal scoring resolves `break-inside: avoid`, `widows`, and `orphans`
with bounded (once-per-flow) relayout.

Chromium's explicit lesson: this is **unretrofittable** — it must be the core
from day one. WeasyPrint's death spiral (super-linear multi-page tables) comes
from relayout-from-scratch instead of carry-over state. This issue replaces the
skeleton's `Cursor` model in `engine/src/layout.rs` (greedy block stacking with
a trivial page-break-on-overflow) with the fragment-tree model.

The token design stays compatible with a future global (multi-page, TeX-style)
break optimization pass — start greedy + appeal.

**Fitness function:** css-break WPT print-reftests via the harness
(`docs/specifications/wpt-conformance-harness.spec.md`), plus unit tests in
`engine/tests/`.

## Goals / Non-Goals

**Goals**

- Layout as pure function `(node, constraints, break_token) → (fragment,
  outgoing_token)`; immutable fragment tree as the only output.
- Fragmentainers (pages) as first-class fragments with no source box.
- Break tokens with `consumed_block_size` and `HasSeenAllChildren` semantics;
  child tokens nest in the parent token (break-token tree).
- Resumable pagination: laying out page N+1 passes the token tree — finished
  siblings skipped, unfinished ones resumed, `IsBreakBefore()` distinguishing
  resume-inside from start-fresh.
- Forced breaks (`break-before`/`break-after: page | always | left | right`)
  and unforced breaks chosen by break-appeal scoring (perfect → last-resort).
- `break-inside: avoid`, `widows`, `orphans` honored via appeal scoring, with
  the EarlyBreak chain: one abort-and-relayout per fragmentation flow when
  space runs out at a bad point, stopping deterministically at the recorded
  break.
- Monolithic content taller than a fragmentainer **overflows, never slices**.
- Pagination measurably O(n): a 1,000-page synthetic document lays out without
  super-linear cost.
- PDF emission walks the fragment tree (no separate pagination pass).
- Deterministic output: identical input → identical PDF bytes.

**Non-Goals** (deferred to CORE-54 or explicitly dropped per the research
brief's "simplify" list)

- Floats × breaks (parallel flows), OOF positioning across fragmentainers,
  tables × fragmentation, multicol / nested fragmentation — all CORE-54.
- Incremental relayout, invalidation, constraint-space caching — batch
  compilation lays out once; keep only measure-pass memoization inside a single
  layout.
- JS geometry APIs, hit-testing, pre-paint, dual tree reconciliation.
- Global (TeX-style) break optimization — the token design must *permit* it
  later, but this issue ships greedy + appeal.
- New CSS properties beyond those needed for breaks: `break-before`,
  `break-after`, `break-inside`, `orphans`, `widows` (and the legacy
  `page-break-*` aliases).

## Behavior

The engine shall:

1. **Be a pure function**: layout takes `(node, constraints, break_token)` and
   reads nothing else; output is an immutable fragment plus an outgoing break
   token. No engine-global mutable state during layout.
2. **Produce a fragment tree**: one `Fragment` per box per fragmentainer it
   lands in; a box split across 3 pages has 3 fragments, each with resolved
   physical offset/size relative to its parent fragment.
3. **Model fragmentainers as fragments**: pages are first-class fragments with
   no corresponding source box, so the model generalizes to `@page` margin
   boxes, columns, and mixed page sizes later.
4. **Carry break tokens**: an outgoing token attaches to each fragment that
   breaks inside; tokens for broken children nest inside the parent token.
   Tokens carry `consumed_block_size` (block-size consumed by previous
   fragments) and a `HasSeenAllChildren` flag that disambiguates "no child
   tokens because done" from "not started" — preventing infinite page
   generation.
5. **Resume via tokens**: laying out the next fragmentainer passes the token
   tree; finished siblings are skipped, unfinished ones resume, and
   `IsBreakBefore()` distinguishes resuming inside a box from starting it
   fresh.
6. **Honor forced breaks**: `break-before`/`break-after` with page-breaking
   values start a new fragmentainer, even inside a box with `break-inside:
   avoid` (forced wins).
7. **Score unforced breakpoints**: every candidate breakpoint gets an appeal
   score (perfect → last-resort); layout breaks at the highest-appeal point
   that fits the most content.
8. **Honor break-avoidance via appeal**: `break-inside: avoid`, `widows`, and
   `orphans` raise the appeal requirement at the offending breakpoint, with
   css-break-3 §4.4 rule-dropping order when constraints conflict.
9. **Bound the cost of avoidance**: when space runs out at a bad point, an
   `EarlyBreak` chain (best recorded breakpoint, possibly deep in an
   already-laid-out subtree) triggers exactly one abort-and-relayout per
   fragmentation flow, stopping deterministically at the recorded break.
10. **Never slice monolithic content**: a monolithic box (line, image, tall
    fixed-height box) taller than the fragmentainer overflows it, with
    last-resort breakpoints placing it.
11. **Truncate margins at fragmentainer boundaries**: margins adjoining a page
    break do not transfer across it (css-break-3).
12. **Be O(n)**: each box is laid out a bounded number of times per flow; a
    1,000-page synthetic document paginates without super-linear cost.
13. **Emit PDF by walking the fragment tree**: `pdf.rs` reads fragmentainers
    and their descendants; there is no second pagination pass.
14. **Keep the CLI contract unchanged**: `typeanvil render <input.html>
    --page-width ... -o out.pdf` (the harness adapter contract) still works as
    specified in the WPT harness spec.
15. **Stay deterministic**: identical input yields byte-identical output.

## Interfaces

**New module** `engine/src/frag.rs`:

```text
Fragment          // immutable layout output node
  kind            // Fragmentainer | Block | Line (inline content)
  parent          // link to parent fragment (physical containment)
  offset, size    // resolved physical geometry, points, top-left
  children        // child fragments
  break_token     // Option<BreakToken> — Some iff this fragment breaks inside

BreakToken        // resumable-layout continuation
  consumed_block_size  // block size used by earlier fragments
  seen_all_children    // HasSeenAllChildren flag
  child_tokens         // nested tokens for broken children
  break_before         // true if the box has not started (IsBreakBefore)

BreakAppeal       // u8: PERFECT > GOOD > TOLERABLE > AVOID_VIOLATING > LAST_RESORT

Fragmentainer     // first-class fragment with no source box
  index           // page number
```

**`engine/src/layout.rs` rework**: the `Cursor`/`Page`/`TextLine` model is
replaced. Public entry keeps the same shape so `main.rs` changes minimally:

```text
pub fn layout(dom: &Dom, stylesheet: &Stylesheet, geometry: PageGeometry)
    -> Layout
// Layout { geometry, pages: Vec<Fragmentainer> }  // pages ARE fragmentainers
```

**`engine/src/css.rs` additions** — `ComputedStyle` gains break fields,
converted from stylo computed values:

```text
break_before:  BreakBetween   // auto | page | always | left | right
break_after:   BreakBetween
break_inside:  BreakInside    // auto | avoid
orphans: u32                   // default 2
widows: u32                    // default 2
```

The legacy `page-break-*` aliases map onto the same fields during
stylo conversion.

**`engine/src/pdf.rs` rework**: `render(layout: &Layout)` walks
`layout.pages` (fragmentainers) and their descendant fragments instead of
`Page { rects, lines }`.

**CLI**: unchanged (see WPT harness spec, "Engine adapter contract").

## Acceptance Criteria

Given/When/Then, each mapping to a real test in `engine/tests/`:

1. **Forced break** — Given a document whose second section has
   `break-before: page`, when rendered, then the section starts on page 2 and
   page 1 holds only the first section (`fragmentation.rs::forced_break_page`).
2. **Unforced overflow** — Given a paragraph taller than one page, when
   rendered, then it splits at line boundaries across pages with no clipped
   line and no lost/duplicated text when fragments are reassembled in order
   (`fragmentation.rs::unforced_split_content_preserved`).
3. **Break-inside avoid** — Given a block with `break-inside: avoid` that fits
   a full page but not the remaining space on the current page, when rendered,
   then the whole block moves to the next page (`fragmentation.rs::avoid_moves_block`).
4. **Orphans/widows** — Given a paragraph with `orphans: 2; widows: 2` that
   breaks across pages, when rendered, then no page ends or begins with a
   single line (`fragmentation.rs::orphans_widows`).
5. **Monolithic overflow** — Given a fixed-height block taller than one page,
   when rendered, then it overflows its fragmentainer instead of being sliced,
   and layout terminates (`fragmentation.rs::monolithic_overflow_no_slice`).
6. **O(n) pagination** — Given a 1,000-page synthetic document (repeated
   forced-break sections), when paginated, then total layout cost scales
   ~linearly with page count (measured against a 100-page baseline; no
   relayout-from-scratch per page) (`fragmentation.rs::pagination_linear_1000`).
7. **Determinism** — Given the same input rendered twice, then the PDF bytes
   are identical (existing smoke determinism test extended to multi-page).
8. **Self-check** — the spec file itself passes `scripts/validate_docs.py`.

## Edge Cases

- **Empty document** → exactly one blank page (matches skeleton behavior; a
  page-count mismatch would FAIL reftests).
- **Content exactly fills a page** → no spurious extra page; `HasSeenAllChildren`
  prevents infinite page generation.
- **`break-inside: avoid` box taller than the page** → breaks anyway (last
  resort), no infinite relayout loop.
- **Forced break inside an avoid box** → forced break wins; the box fragments
  at the forced boundary.
- **Margin at a page boundary** → a fragmented box's continuation truncates
  its top margin at the fragmentainer edge; a FRESH box at a page start keeps
  its top margin (css-break-3 §3.1, refined in CORE-153 toward the spec and
  Chromium; previously truncated Prince-style — see
  ua-print-defaults.spec.md).
- **`orphans`/`widows` conflicting with `break-inside: avoid`** → css-break-3
  §4.4 rule-dropping order resolves; the run terminates.
- **Box already started when a later `break-before` applies** → `IsBreakBefore`
  semantics: a start-fresh token, not a resume, is used.
- **Deeply nested break-token tree** → nesting mirrors the path of unfinished
  nodes only; depth is bounded by DOM depth, not page count.

## References

- Research brief: `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
  (LayoutNG model, break tokens, appeal scoring, carry-over vs. simplify).
- Harness spec: `docs/specifications/wpt-conformance-harness.spec.md`
  (fitness function; CLI contract for `typeanvil render`).
- Engine today: `engine/src/layout.rs` (the `Cursor` model being replaced),
  `engine/src/css.rs` (`ComputedStyle` seam), `engine/src/pdf.rs`.
- Related work: CORE-50 (walking skeleton), CORE-56 (stylo binding on the
  `ComputedStyle` seam).
- WPT: css-break-3 spec (w3.org/TR/css-break-3), css-break print-reftests
  (~640 files).
