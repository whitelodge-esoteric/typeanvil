---
title: Floats Fragmentation
slug: /specifications/floats-fragmentation
type: spec
status: draft
owner: maintainers
created: 2026-08-18
updated: 2026-09-27
sidebar_position: 7
tags: [engine, layout, css-float, css-break, fragmentation]
spec_id: floats-fragmentation
applies_to: engine 0.x
dependencies: [fragmentation-core, wpt-conformance-harness]
---

# Floats Fragmentation

## Overview

A float that breaks inside a page is a **parallel flow** (css-break-3): the
float suspends, in-flow siblings continue in the same fragmentainer, and the
float resumes on the next one. That requires tracking multiple simultaneous
break/resume states per fragmentainer, and the float's containing-block
block-offset must be known before a child can fragment.

The engine has zero float support today: `layout.rs` lays out block and inline
boxes only (the only "float" hits in the source are the `geom.rs` FMA comment
and table-column widths). This issue adds `float`/`clear` placement and
fragmentation on top of the fragment tree.

**Path chosen: stylo.** Verified 2026-08-18 against stylo 0.20.0
`properties/longhands.toml`: `float` (type `Float`) and `clear` (type `Clear`)
are first-class longhands, NOT gecko-gated and NOT pref-gated — the servo build
compiles them (`values/specified/box.rs` defines `Float` and `Clear`; the box
style struct exposes `clone_float()`/`clone_clear()` on the generated
`properties.rs` path). Cascade, specificity, and `!important` come from stylo
exactly like `text-align` and `line-height`; no manual author-CSS pass needed.

**Fitness function:** css-break / css-float WPT print-reftests via the harness,
plus unit tests in `engine/tests/floats.rs`.

## Goals / Non-Goals

**Goals**

- `float: left | right` placement of a block-level box within its BFC
  (block formatting context): the float is taken out of in-flow block
  stacking, placed against the BFC's content edge, and in-flow content wraps
  around it.
- Inline wrapping: line boxes in the float's cross-span use a reduced
  available measure (content-box width minus the float's intrusion), per
  css2 §9.5.
- `clear: left | right | both` (plus the logical `inline-start`/`inline-end`
  aliases stylo computes) pushes subsequent in-flow content and subsequent
  floats below the corresponding floats.
- **Fragmentation as parallel flow (the issue's core):** a float that does not
  fit in the current fragmentainer suspends with its own break state; in-flow
  siblings continue in the same fragmentainer; the float resumes at the top of
  the next one — css-break-3 parallel-flow semantics.
- Multiple floats per fragmentainer: each BFC tracks a float stack with
  simultaneous suspend/resume states.
- Determinism: float placement resolved in document order; identical input →
  byte-identical PDF.

**Non-Goals** (deferred; scope stays honest)

- Nested floats (a float inside a float) — the float stack handles only
  same-BFC floats; nested BFC interactions follow the outer flow.
- `shape-outside` / float shapes (compiled in stylo but not required; not
  promised).
- Floats inside multicol or abspos interactions — the
  interaction specs land after the individual features.
- Floats in the `@page` margin-box context.
- Advanced BFC rules (new formatting contexts triggered by overflow/float
  ancestors beyond what the current block layout already creates).

## Behavior

The engine shall:

1. Compute `float` and `clear` for every element at cascade time from stylo's
   box struct (`Float`: `None | Left | Right`; `Clear`):
   `ComputedStyle` gains `float: Float` and `clear: Clear`, read in
   `css.rs::convert` (no manual author-CSS pass).
2. Treat a box with `float != None` as a parallel flow: it does not consume
   block space in its BFC's in-flow stacking, and in-flow siblings do not
   stack below it (they wrap around).
3. Place a left/right float against the BFC's content-box edge, below any
   previously placed float in the same BFC that intrudes into its
   cross-axis span (basic stacking; no advanced float-formatting-context
   rules).
4. Shorten the available inline measure for line boxes while a float intrudes:
   a line in the float's block-span is constrained to the content box minus
   the float's extent on its side.
5. Honor `clear`: a block (or float) with `clear` set does not start until it
   is below every preceding float on the cleared side(s).
6. **Fragment floats as parallel flows:** when a float does not fit in the
   current fragmentainer, record its break state (the float suspends), let
   in-flow content continue in the current fragmentainer, and resume the float
   at the start of the next fragmentainer (css-break-3).
7. Track per-fragmentainer float stacks: a BFC may hold several suspended
   floats at once, each resuming independently; resume order is document order.
8. Resolve the float's containing-block block-offset before fragmenting its
   children (the issue's ordering requirement): the parallel flow is laid out
   with the BFC's offset already known, so resumed fragments land at correct
   absolute offsets.
9. Keep the monolithic-content rule: a float taller than a fragmentainer
   overflows rather than slicing a line; last-resort breakpoints place it.
10. Stay deterministic: float resolution is a single document-order pass with
    no hash-order dependence.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle` (the cascade output contract; layout and PDF read
  only this):

  ```rust
  /// CSS `float` (stylo computed; values::specified::box::Float).
  pub float: Float,   // None | Left | Right
  /// CSS `clear` (stylo computed; values::specified::box::Clear).
  pub clear: Clear,   // None | InlineStart | InlineEnd | Left | Right | Both
  ```

- In `convert`, read `clone_float()` / `clone_clear()` from stylo's box
  struct. `ComputedStyle::initial()` gets `float: Float::None`,
  `clear: Clear::None`.

### `engine/src/layout.rs` / new `engine/src/float.rs`

- A `FloatStack` per BFC: ordered placed floats with resolved `Rect`,
  `clear`, and per-fragmentainer suspend state.
- Block layout: when a box has `float != None`, lay it out as a parallel
  flow, place it in the stack, and continue in-flow siblings.
- Inline layout: consult the BFC's float stack when computing line available
  measure (intrusion on left/right).
- Fragmentation: a suspended float carries its own break token; the
  fragmentainer's next-flow resume picks it up before continuing in-flow
  content. This is the "multiple simultaneous break/resume states per
  fragmentainer" requirement — extend the fragmentainer's resume machinery to
  hold one token per parallel flow (in-flow + N floats).

### `engine/src/frag.rs`

- `Fragmentainer` gains a float-resume list (or the parallel-flow token set is
  threaded through the existing break-token tree — the minimal change that
  keeps the token design compatible with the existing break-token model).

### `engine/src/pdf.rs`

- Paint float fragments from the fragmentainer's float list (the PDF pass
  already walks the fragment tree; floats are children of the fragmentainer in
  paint order — in-flow content first, floats in document order).

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/floats.rs` (helpers mirror
`tests/typography.rs`).

1. **Computed style.** Given `img { float: left }`, the computed `float` is
   `Left` and `clear` is `None`; the styled box's fragments report a
   left-edge placement against the BFC content box.
2. **Inline wrap.** Given a left float followed by a paragraph, the first
   line boxes are shortened to the available measure (content width minus
   float width) and return to full width below the float's block-span.
3. **Clear.** Given `float: left` then `div { clear: both }`, the cleared
   block starts below the float's bottom edge.
4. **Parallel flow across pages.** Given a float taller than one page, the
   float suspends on page 1 (in-flow text continues below/around it), resumes
   at the top of page 2, and its continuation lands at the correct offset;
   page 2's in-flow content starts after the resumed float. Assert on
   fragment y-extents per page.
5. **Multiple suspended floats.** Given two floats in one BFC where the
   second doesn't fit on page 1, both resume on page 2 in document order
   (second below first).
6. **Monolithic overflow.** Given a float taller than the page with no valid
   break, it overflows the fragmentainer (never slices a line).
7. **Determinism.** Rendering the same doc twice produces byte-identical PDFs
   (CLI render + hash compare, as in `tests/smoke.rs`); the full existing
   engine test suite still passes.

## Edge Cases

- Float at the top of a fragmentainer with in-flow content already resumed:
  the float places below the resumed content (document-order placement), per
  css-break-3.
- Float wider than the available measure: clamped to the content box (no
  negative wrap measure).
- `clear` with no preceding float: no-op.
- A float whose whole content is monolithic and taller than the page:
  last-resort placement overflows (behavior 9).
- Zero-width/zero-height floats: legal; occupy no space, no panic.
- `float: none` (the default): no parallel flow, unchanged block layout.

## References

- css-break-3 (parallel flows / fragmentation of floats):
  https://drafts.csswg.org/css-break-3/
- css2 §9.5 floats: https://www.w3.org/TR/CSS2/visuren.html#floats
- Fragment tree + break-token model this builds on:
  `fragmentation-core.spec.md`
- Research brief (float × break as a parallel flow):
  `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
- Fragmentation model: `fragmentation-core.spec.md`.
- stylo 0.20.0 `properties/longhands.toml`: `float`, `clear` — compiled in the
  servo build (verified 2026-08-18).

## Behavior — floats across fragmentainers (2026-09-06)

The engine shall additionally:

11. Size a float's margin box from its declared `height` even when the
    content measures smaller (css-sizing-3 §5.1, same max() shape as the
    block paint box): empty fixed-height floats measure ZERO otherwise and
    never stack, intrude, or defer (page-size-007/008 root cause).
12. Place a float whose vertical span starts at a box top WITHOUT the
    preceding sibling's line box pushing it down when that sibling declares
    `height: 0` (css-sizing-3 §5.1: the declared height sizes the flow
    extent; overflow content paints but does not advance the cursor). The
    clamp applies only when the box has no block-child fragments — a box
    whose overflow comes from a block child keeps the content extent, or
    following siblings displace (block-002-wm-* regression).
13. Pack floats per css2 §9.5.1 rules 2+7: a float that has no stacking-free
    x lane beside vertically-overlapping same-side floats moves DOWN below
    them (probe loop: `float_placement_y` returns the lowered y); fits and
    placement share one probe so the landed y equals the checked y.
14. Defer a float whole to the next fragmentainer when no lowered position
    fits (css-break-3 parallel flow), and re-absorb a deferred page-change
    break carried out of a paint-empty page (a resume token with a single
    break-before child at index 0 unwraps at page start).
15. Extend a box's background paint to the fragmentainer bottom edge when the
    box continues past the page (css-break-3 fill-to-edge; only the LAST
    fragment ends at the content edge — Chromium-verified against
    page-size-007). Paint-box only: `used` stays content-based so pagination
    never sees the fill.
16. Contain floats in BFC-establishing boxes (css2 §10.6.3): a flow-root's
    height extends to the lowest float bottom inside it, on EVERY fragment
    (continuation bands paint the container background behind float rows).

**Gate result (2026-09-06):** page-size bucket 12/16 → 14/16; page-size-007
and page-size-008 PASS. css-break 63 tests, css-multicol 5, flexbox 27 —
zero status flips vs a fresh branch-point binary (9d96067). Residuals
page-size-009 (page count 1 vs 2) and page-size-012 (pixel diff) fail on the
branch-point binary too — pre-existing, out of scope here.

## Acceptance Criteria — additional float behavior

Each criterion maps to a test in `engine/tests/floats.rs`:

8. **Zero-height divider.** Given a `height: 0` div with a text line
   followed by floats, the first float starts at the container's content
   top (y=0), not below the divider's line box
   (`zero_height_div_does_not_push_floats_down`).
9. **Row packing + overflow.** Given 9 fixed-size floats in a flow-root at
   Letter geometry, one full row packs at y=0 on page 1 and the row-2
   floats defer whole to page 2
   (`float_row_wraps_then_overflows_to_next_page`).
10. **Fill-to-edge.** Given the same floats inside a background-colored
    flow-root, page 1's container fragment (a middle fragment) paints to
    the fragmentainer bottom
    (`continued_box_background_fills_to_fragmentainer_bottom`).
