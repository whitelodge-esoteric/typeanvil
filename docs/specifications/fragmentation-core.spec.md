---
title: Fragmentation Core
slug: /specifications/fragmentation-core
type: spec
status: approved
owner: maintainers
created: 2026-08-16
updated: 2026-10-24
sidebar_position: 2
tags: [layout, fragmentation, css-break, engine]
spec_id: fragmentation-core
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
- `break-before`/`break-after: avoid` (and `avoid-page`, `avoid-column`, the
  legacy `page-break-*: avoid`) honored between adjacent in-flow block
  siblings by backing up to the previous sibling boundary (see the Addendum
  "Between-Sibling Break Avoidance").
- Monolithic content taller than a fragmentainer **overflows, never slices**.
  EXCEPTION: a PINNED abspos whose used inset lands at/past the
  fragmentainer bottom is deferred to the page CONTAINING its offset
  (Chromium fragmented printing) and fragments from there; see
  `out-of-flow-positioning.spec.md` Behavior 9. EXCEPTION: a PINNED
  abspos that STARTS inside the page, declares an extent, exceeds the page it
  starts on, and is not inside a clipping (`overflow` != `visible`) ancestor
  fragments from that page like an in-flow box; a pinned box that fits itself
  in one fragmentainer overflows in place instead of paginating.
  EXCEPTION: a PINNED abspos with a DECLARED extent (absolute,
  viewport, OR percentage — `resolved_height_or_percent`) fragments against
  the REAL fragmentainer bottom even when it is NESTED under a monolithic
  ancestor that fit one page; the inherited `bottom_limit` must not widen to
  `f64::MAX` inside a monolithic subtree (see the out-of-flow spec
  Behavior 9).
- Pagination measurably O(n): a 1,000-page synthetic document lays out without
  super-linear cost.
- PDF emission walks the fragment tree (no separate pagination pass).
- Deterministic output: identical input → identical PDF bytes.

**Non-Goals** (deferred to later interaction work or explicitly dropped per the research
brief's "simplify" list)

- Floats × breaks (parallel flows), OOF positioning across fragmentainers,
  tables × fragmentation, multicol / nested fragmentation.
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
11. **Occupy the specified extent in flow**: a fresh block box
    with a declared `height` (box-sizing honored) occupies that extent in
    pagination — the cursor advances by it and a fresh child whose extent
    fits one fragmentainer but not the remaining space defers whole to the
    next page. Own-inline overflow still clamps the flow extent down to
    content (`height: 0` divs, page-size-007/008). Block-child overflow
    also advances the flow by the declared extent only, fresh or resumed
    (CORE-565, 2026-10-24): the child's overflow is ink overflow
    (css-overflow-3) and does not push following siblings. A fresh box
    whose declared extent fits the remaining space is not deferred whole
    when only its overflowing child breaks (block-002-wm-*). A content-free
    spacer shall fragment at the break instead of deferring whole
    (CORE-567, 2026-10-24). A spacer has a declared extent, no DOM children,
    no background, and no block-axis border. It has no first line that a
    break could strand (css-break-3 §4.4, §5.3). Chromium puts 20px of the
    500px spacer in the body-background-slr reference on page 1. A painted
    declared-height box keeps the whole-box deferral for now. Widening the
    rule moved references that the engine cannot match on the test side
    (table-fragmentation-002a/b, monolithic-overflow-018; tracked as
    CORE-569). An extent crossing the
    fragmentainer edge paints through it without emitting a continuation —
    empty declared-height boxes are treated as monolithic (the
    page-change fixtures pin this; true extent fragmentation is future
    work gated on a full-suite A/B).
    **SUPERSEDED for ordinary in-flow blocks (DECIDED 2026-10-23, CORE-551):**
    the "paints through without a continuation" clause no longer applies to
    them. The Addendum "Explicit-Height Block Continuation" governs: such a
    block continues on every page until its declared extent is consumed.
    The monolithic wording stays in force for lines, replaced elements and
    pinned abspos boxes that fit one fragmentainer. Progress is kept separate
    from paint size; see `writing-modes-layout.spec.md`, "Declared height
    versus continuation progress".
12. **Truncate margins at fragmentainer boundaries**: margins adjoining a page
    break do not transfer across it (css-break-3).
13. **Be O(n)**: each box is laid out a bounded number of times per flow; a
    1,000-page synthetic document paginates without super-linear cost.
14. **Emit PDF by walking the fragment tree**: `pdf.rs` reads fragmentainers
    and their descendants; there is no second pagination pass.
15. **Keep the CLI contract unchanged**: `typeanvil render <input.html>`
    --page-width ... -o out.pdf` (the harness adapter contract) still works as
    specified in the WPT harness spec.
16. **Stay deterministic**: identical input yields byte-identical output.

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
break_before:  BreakBetween   // auto | avoid | page | always | left | right
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
- **Trailing blank page** → a final page whose fragment tree paints
  nothing is dropped. Guards: only the LAST page (outgoing token is none); a
  sized bare fragment is layout state, not blank (a declared-height
  continuation tail slice keeps its page even with content-less paint). A
  TRAILING forced break (last item) is absorbed and produces no page
  (Chromium-verified: the used value of a forced break at the end of the
  document produces no page; basic-pagination-001 codifies this), so no
  forced-break exception to the drop exists. An empty document still renders
  exactly one page via the post-loop fallback.
- **Margin at a page boundary** → a fragmented box's continuation truncates
  its top margin at the fragmentainer edge; a FRESH box at a page start keeps
  its top margin (css-break-3 §3.1, refined toward the spec and
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
- Related implementation: `engine/src/layout.rs`, `engine/src/css.rs`, and
  the `ComputedStyle` seam.
- WPT: css-break-3 spec (w3.org/TR/css-break-3), css-break print-reftests
  (~640 files).

## Addendum: Explicit-Height Block Continuation

`updated: 2026-09-09`. This is a narrowed slice: continuation of an
ordinary block with an explicit CSS `height` that extends past the
fragmentainer, in the block path only (`layout_box`). Wrapper / flex / grid
continuation is out of scope here and remains tracked separately.

### Problem

`layout_box` resolves an explicit `height` only on the box's final fragment
(`box_height = max(content, declared)` when `!broke`) and never emits a
continuation token for the unused declared extent. A block that declares more
height than the current fragmentainer holds paints its full declared
rectangle on the first page (overflowing it) and generates no further
fragments: an empty 450pt block in a 144pt content box renders exactly one
page instead of four.

### Behavior

1. An ordinary block with a resolved `height` whose border box extends past
   the fragmentainer bottom SHALL continue: each fragmentainer it touches
   holds exactly one fragment of the box, and fragments appear on every page
   until the declared extent is consumed.
2. The last continuation fragment SHALL be sized so the sum of fragment
   heights plus inter-fragment spacing (truncated margins/padding at the
   fragmentainer boundaries) equals the declared border box.
3. `box-sizing` SHALL behave as on the single-fragment path: `content-box`
   adds padding and border to the declared content height; `border-box`
   treats the declared height as the border box.
4. A following in-flow sibling SHALL be placed after the box's last
   fragment, at the declared bottom edge, on the page where the extent ends.
5. Content inside the box that is taller than the declared height SHALL
   still paint (the `max(content, declared)` rule) and SHALL NOT clip.
6. A box whose content is taller than its declared height and taller than
   the fragmentainer SHALL fragment by content as today (content
   continuation takes precedence over height continuation); the height
   mechanism only supplies extent where content ends first.
7. Zero-height boxes (declared or content) SHALL NOT generate continuation
   pages.
8. Progress: every continuation fragment of the box SHALL either place
   content or consume declared extent; the page loop SHALL terminate (no
   token with zero remaining extent and no pending children may leave
   `seen_all_children` false).

### Acceptance Criteria

| # | Test | Asserts |
|---|------|---------|
| AC1 | `height_continuation_page_count` | Empty 450pt block, 144pt page content box → 4 pages (144+144+144+18 = 450) |
| AC2 | `height_continuation_fragment_heights` | First three fragments 144pt tall; tail fragment 18pt; fragment heights sum to 450pt |
| AC3 | `height_continuation_sibling_placement` | The sibling after the block paints on the final page, at or below the block's tail-fragment bottom |
| AC4 | `height_continuation_no_spurious_pages` | Empty 144pt block in a 144pt content box → 1 page (no trailing empty page) |
| AC5 | `height_continuation_box_sizing_border_box` | 450pt border-box height with 10pt vertical padding spans the same 4 pages as a bare 450pt content-box block |
| AC6 | `height_continuation_forced_child_consumes_extent` | Forced `break-before` child inside a 450pt parent, 144pt page content box → 4 pages (Chromium-verified); text order preserved |

AC1–AC6 live in `engine/tests/fragmentation.rs`. Behavioral regression risk
is bounded by the existing fragmentation, floats, and page-size suites; any
regression there blocks the slice (WPT A/B against the frozen baseline
remains the gate).

### Continuation-vs-forced-break semantics

The resume guard that suppresses a spurious trailing page
(`seen_all_children` with no pending child tokens) keys on child completion
only. Height continuation is orthogonal: when the box's declared extent is
not yet consumed, a resumed page with all children emits a zero-child slice
of the remaining extent.

When a box that declares a `height` CONTINUES past a fragmentainer (whether
by child overflow, a forced child break, or declared-extent overflow), the
box's block-size progress for that page SHALL be measured to the
FRAGMENTAINER EDGE, not to the placed content bottom (css-break-3 §5.3: the
space from the break point to the fragmentainer edge counts toward the
box's specified block-size progress). Without this, a forced child break
with room left on the page consumes no progress at all and the skipped
space re-renders as extra pages (observed: 5 pages where Chromium renders
4 for a forced break inside a 450pt parent on a 144pt content box). The
rule is scoped to declared-height boxes: without `height`,
`consumed_block_size` has no block-path reader, and a zero-height fragment
makes no progress, so the page loop still terminates.

A forced break itself never marks the box finished while
`declared − consumed > 0`; the next page continues the remaining extent.

**Progress is not paint size (DECIDED 2026-10-23, CORE-551).** Three
quantities stay separate: the fragment's paint size (`max(content,
declared)`), the content extent placed in this fragment, and the consumed
extent. Consumed extent depends only on the declared extent and the
fragmentainer bounds and never reads a fragment's paint size. Termination:
every fragment places content or consumes extent greater than zero, or it is
the box's final fragment. In a vertical flow the same rules apply on the
flow's block axis (`writing-modes-layout.spec.md`, Behaviors 31-34).

## Addendum: Between-Sibling Break Avoidance

`updated: 2026-10-13` (CORE-365). Narrowed slice: `break-before: avoid` and
`break-after: avoid` between adjacent in-flow block siblings, in the block
path (`layout_box` item loop) and the multicol column loop.

### Problem

`parse_between` folded `avoid` into `auto`, so a heading with
`break-after: avoid` (or the legacy `page-break-after: avoid`) that lands on
the last line of a page stays there while its paragraph starts the next page.
Chromium and Prince move the heading to the next page with its text
(css-break-3 §3.1, §4.4).

### Behavior

1. `break-before` / `break-after` values `avoid`, `avoid-page`, and
   `avoid-column`, and the legacy `page-break-before` / `page-break-after:
   avoid`, SHALL compute to `BreakBetween::Avoid`. `Avoid` is not a forced
   break (`is_forced()` stays false). v1 deviation: the three keywords are not
   distinguished by fragmentainer type (a column break and a page break are
   both avoided by any of them). `column` SHALL compute to
   `BreakBetween::Column`: v1 does not break for it (unchanged from when it
   was dropped as unparsed), but it is a forced value for rule 2.
2. The boundary between two ADJACENT in-flow block siblings A then B (both
   non-floated and `static`/`relative`; no text run, float, or abspos item
   between them) is an **avoid boundary** when A's `break-after` or B's
   `break-before` is `Avoid`, and neither value is a forced value (`page`,
   `always`, `left`, `right`, `column`).
3. When B places nothing on the current fragmentainer and is pushed whole to
   the next one (any defer path: does not fit, empty first fragment, declared
   extent, `break-inside: avoid`; in a column, also a block that broke having
   used zero block size), and the A|B boundary is an avoid boundary, the
   engine SHALL break before A instead: A is removed from the current
   fragmentainer and starts the next one, with B following it. A `display:
   table` B (with or without a header row or `<caption>`) is an in-flow block
   sibling: when it starts fresh and its fragment places zero height, it
   counts as pushed whole (CORE-369).
4. Chains: when the boundary before A is also an avoid boundary, the backup
   goes to the FIRST sibling of the consecutive avoid chain.
5. Avoidance is a preference: when that first sibling did not start fresh on
   this fragmentainer, or nothing precedes it there (on a page: anything
   placed on the fragmentainer; in a column: anything above it in that
   column), no earlier non-violating break exists and the engine SHALL break
   at the natural point (B moves, A stays). Every candidate violates an
   avoid, so the latest one wins, as in Chromium.
6. Forced breaks win: a forced `break-before` on B or `break-after` on A
   breaks at the A|B boundary regardless of avoid.
7. The backup SHALL restore layout state exactly: the placed children, the
   outgoing child tokens, the cursor, the `Flow` bookkeeping (running
   strings, counters, floats, footnote registrations), and the link-rect sink
   are rolled back to their values before A was laid (same mechanism as the
   footnote-area re-lay).
8. Termination: a backup leaves at least the content before the chain on the
   current fragmentainer, so every page makes progress; on the next
   fragmentainer the chain starts with nothing before it and rule 5 applies.

### Non-Goals (known limitations)

- Propagation (css-break-3 §4.1): an avoid on a box's first or last child
  does not constrain the parent's own sibling boundary, and an avoid between
  two boxes does not reach into their descendants' breakpoints.
- Flex items, grid items, and table rows do not honor between-sibling avoid.
- Balanced multicol sets (a set whose content fits the page and whose
  column height is computed before filling) do not back up: a backup there
  would spill the set's tail past the last column onto a new page
  (css-multicol `balance-break-avoidance-002` shape). Chromium grows the
  balanced height instead. Columns that fill to the page bottom honor avoid.
- B that places some content and then breaks is a break inside B (governed by
  orphans/widows), not an A|B boundary break; it is unaffected.

### Acceptance Criteria

| # | Test | Asserts |
|---|------|---------|
| AC1 | `avoid_after_keeps_heading_with_paragraph` | `h2 {break-after: avoid}` on a page's last line moves to the next page with its paragraph; page 1 holds only the content before it |
| AC2 | `legacy_page_break_after_avoid_matches` | `page-break-after: avoid` renders the same pages as AC1 |
| AC3 | `avoid_before_on_following_sibling` | `p {break-before: avoid}` keeps the preceding heading with it |
| AC4 | `avoid_chain_moves_every_heading` | `h1` + `h2`, both `break-after: avoid`, move together with the paragraph |
| AC5 | `avoid_without_earlier_break_breaks_anyway` | A heading first on the page followed by a block that defers: the heading stays, the block moves, layout terminates |
| AC6 | `forced_break_wins_over_avoid` | `break-after: avoid` on A plus `break-before: page` on B breaks between them |
| AC6b | `column_break_value_defeats_avoid` | `break-after: column` on A beats `break-before: avoid` on B: the natural break stands |
| AC7 | `avoid_after_inside_column_moves_heading` (multicol.rs) | A heading with `break-after: avoid` at a column bottom moves to the next column with its paragraph |
| AC7b | `balanced_set_ignores_avoid_instead_of_spilling` (multicol.rs) | A balanced set with an avoid boundary stays on one page |
| AC8 | `avoid_is_not_forced` (css unit) | `avoid`, `avoid-page`, `avoid-column` parse to `Avoid`; `is_forced()` is false |

AC1–AC6b and AC8 live in `engine/tests/break_between_avoid.rs`.
