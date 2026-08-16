---
title: "LayoutNG Block Fragmentation: Lessons for a Fragmentation-First Print Engine (Typeanvil)"
type: research
status: approved
owner: elijah
created: 2026-08-14
updated: 2026-08-16
sidebar_position: 1
tags: [layout, fragmentation, chromium, layoutng, css]
---

# LayoutNG Block Fragmentation: Lessons for a Fragmentation-First Print Engine (Typeanvil)

**Sources:** Morten Stenshorne, *RenderingNG deep-dive: LayoutNG block fragmentation* (developer.chrome.com/docs/chromium/renderingng-fragmentation, 2023); Ian Kilpatrick & Koji Ishii, *RenderingNG deep-dive: LayoutNG* (developer.chrome.com/docs/chromium/layoutng, 2021); Chris Harrelson, *Key data structures in RenderingNG* (developer.chrome.com/docs/chromium/renderingng-data-structures); Chromium source docs: `third_party/blink/renderer/core/layout/layout_ng.md` and `block_fragmentation_tutorial.md` (chromium.googlesource.com); CSS Fragmentation Module Level 3 (w3.org/TR/css-break-3).

---

## 1. Fragments vs. layout objects

LayoutNG splits layout into a **pure(ish) function**: input = `(BlockNode + ComputedStyle, ConstraintSpace, BreakToken)`, output = `LayoutResult` wrapping an immutable `PhysicalFragment` (née `NGPhysicalFragment`). Key properties:

- **Two trees.** The mutable `LayoutObject` tree mirrors the DOM and holds inputs; the **immutable fragment tree** is the *output* of layout — position and size of everything, one fragment per box *per fragmentainer it lands in*. An element split across 3 pages has 3 box fragments. Fragmentainers (columns/pages) are themselves fragments with **no corresponding layout object**.
- **Physical, resolved geometry.** Fragment offsets/sizes are pure physical left/top/width/height relative to the parent fragment; writing mode and direction are resolved during layout. Child offsets live in the parent's child list, so a cached subtree can be re-positioned without relayout.
- **Containing-block-shaped tree.** In the fragment tree, an abspos fragment is a *direct child* of its containing block's fragment (or of the fragmentainer, under fragmentation), regardless of intermediate DOM ancestors. Paint and hit-testing traverse fragments, not layout objects — critical because under fragmentation it's the fragment relationships, not the box relationships, that carry geometry.
- **Immutability = cacheability.** Constraint space is stored with the result as a cache key; relayout diffs old vs. new constraints in one well-contained place. Algorithms may not read anything outside the input tuple (this discipline killed Blink's classic under-invalidation and hysteresis/non-idempotence bug classes — Kilpatrick).
- **Inline content** is a flat list (`FragmentItem`s) per inline formatting context, not a box tree — cheap DFS, memory-local, paragraph-level caching.

## 2. Why legacy fragmentation failed

The legacy engine (Stenshorne) had **no fragmentation concept in layout**. It laid fragmentable content into one tall virtual strip (column-width wide), inserted "pagination struts" to push content past imaginary page boundaries, then did the actual "cutting with scissors" **after layout**, during pre-paint/paint, by clipping and translating slices of the strip. Consequences:

- Anything spec'd to apply *post-fragmentation* — relative positioning, transforms, box/text shadows — was applied to the unsliced strip, producing clipped shadows, bleed across columns, wrong transform origins.
- Tall **monolithic** content (lines, images, scrollers) was brutally sliced instead of overflowing its fragmentainer.
- Only forced breaks + `break-inside:avoid` + orphans/widows; **no `break-before/after:avoid`**, no real breakpoint optimization.
- No flex/grid fragmentation at all; mutable single tree made every fix regress something else.

The rewrite moved fragmentation **into layout itself**: layout produces per-fragmentainer fragments directly; slicing disappears; painting is a plain fragment-tree walk. Notably, most of the multi-year shipping delay (core shipped Chrome 102, tables 106, printing 108) was *coexistence with the legacy engine* — fallback detection, writing data back to legacy structures — a tax a green-field engine never pays. Stenshorne's key retrospective point: **break-avoidance had to be in the core from the start; adding it later would have meant another rewrite.**

## 3. Break tokens: resumable layout

Layout descends the box tree depth-first until it runs out of fragmentainer space or hits a forced break, then finishes fragments all the way up to the fragmentation root. The resume mechanism is the **`BlockBreakToken`**:

- Attached to each fragment that breaks *inside*; tokens for broken children nest inside the parent's token, forming a **break-token tree** mirroring the path of unfinished nodes. A "break-before" token exists for a child not yet started (no fragment produced for it yet).
- Laying out fragmentainer N+1 = run the same algorithms passing the token tree: skip finished siblings, resume unfinished ones (`ChildBreakTokens()`, `BlockChildIterator`), distinguish resume-inside vs. start-fresh via `IsBreakBefore()`.
- Tokens carry `ConsumedBlockSize()` (block-size used by previous fragments) so specified heights resolve correctly across breaks, and `HasSeenAllChildren()` to disambiguate "no child tokens because done" from "not started" (prevents infinite column generation).
- **Break-quality machinery:** every candidate breakpoint gets an appeal score (perfect → last-resort). Golden rule: break at the highest-appeal point that fits the most content. If space runs out at a bad point, an `EarlyBreak` chain (recorded best breakpoint, possibly deep in an already-laid-out subtree) triggers **one abort-and-relayout per fragmentation flow**, this time stopping deterministically at the recorded break. Bounded cost, spec-correct `orphans`/`widows`/`break-*:avoid` resolution (css-break-3 §4.4 rule-dropping order).

Effectively, break tokens are **continuations**: layout is a resumable coroutine whose suspended state is a small serializable tree, not the whole engine's mutable state.

## 4. Known hard cases

- **Floats × breaks (parallel flows).** A float that breaks inside is a *parallel flow* (css-break-3): the float suspends, but layout of in-flow siblings continues in the same fragmentainer, and the float resumes in the next. Requires tracking multiple simultaneous break/resume states per fragmentainer. Also: BFC block-offset must be known *before* fragmenting a child, so margin collapsing forces layout aborts/reruns.
- **OOF positioning across fragmentainers.** Abspos elements bubble to their containing block; under fragmentation their fragments become children of the *fragmentainer*, not the CSS containing block — the messiest part of Chromium's tree mapping, and a major pre-paint complication.
- **Nested fragmentation** (multicol in multicol, multicol under print): inner content is constrained by every enclosing context; column balancing needs measure passes; spanners interrupt columns.
- **Tables** shipped last for a reason: repeated headers/footers, row-group splitting, borders under `border-collapse`, rows as breakpoints.
- **Monolithic content** taller than the fragmentainer: must overflow (never slice), with last-resort breakpoints to place it.
- **Two-pass modes** (flex/grid stretch; column balancing) × fragmentation: measure/layout caching needed to stay O(n).

## 5. Carry over vs. simplify for Typeanvil (paged-only, batch)

**Carry over:**
1. Layout as pure function `(node, constraints, break-token) → fragment + outgoing token`; **immutable fragment tree** as sole output; paint/PDF emission as fragment-tree walk. Idempotence by construction.
2. **Break-token continuations** with consumed-block-size and seen-all-children semantics — the single best abstraction here.
3. **Break appeal scoring + EarlyBreak with bounded (once-per-flow) relayout** — build in day one; Chromium's lesson is it cannot be retrofitted.
4. Parallel-flow handling for floats; monolithic-overflow (never slice); fragmentainer-anchored OOF fragments; physical-coordinate resolution during layout.
5. Fragmentainers as first-class fragments without source boxes — generalizes to pages, `@page` margin boxes, columns, mixed page sizes.

**Simplify or drop:**
1. **All invalidation/caching for incremental relayout** — constraint-space diffing, dirty bits, cache keys, subtree reuse. Batch compilation lays out once; keep only measure-pass memoization inside a single layout (flex/grid/balancing) to avoid exponential blowups.
2. **No dual trees to reconcile**: no legacy coexistence, no JS geometry APIs (`offsetTop`), no hit-testing, no pre-paint dual-walk. The box tree can be a thin immutable styled-input tree; fragments are the only output.
3. Interruptibility/scheduling, containment optimizations, `overflow:clip`-stops-fragmentation relayout paths, hysteresis defenses — irrelevant without a mutable long-lived session.
4. Since only one fragmentation-context kind at top level (pages), nesting reduces to multicol-under-print; can constrain scope accordingly.
5. Freedom Chromium lacked: tokens can carry richer state (e.g., lookahead for line-level widow/orphan optimization, TeX-style page-break optimization over token sequences) since determinism, not 60fps, is the budget.

*(~1150 words)*
