---
title: Writing Modes Layout — Shared Coordinate Contract
slug: /specifications/writing-modes-layout
type: spec
status: draft
owner: maintainers
created: 2026-10-23
updated: 2026-10-27
sidebar_position: 72
tags: [engine, layout, writing-modes, css-writing-modes, fragmentation, coordinates]
spec_id: writing-modes-layout
issue_id: CORE-551
applies_to: engine 0.x — layout.rs, css.rs, paged.rs, pdf.rs
dependencies: [fragmentation-core, out-of-flow-positioning, named-pages, paged-media-css, inline-base-direction, min-max-sizing]
---

# Writing Modes Layout — Shared Coordinate Contract

## Overview

The engine lays out and paginates every flow horizontally today. Vertical
writing modes (`vertical-rl`, `vertical-lr`, `sideways-rl`, `sideways-lr`) need
one coordinate model that the root page flow, interior orthogonal boxes, the
out-of-flow path and the paint emitter all share.

This spec defines that model. It changes no engine behavior by itself. It is
the contract for the child issues of epic CORE-203 (see
[Child issue map](#child-issue-map)). Each child shall cite the numbered
behaviors below in its own acceptance criteria.

Why a shared contract: two earlier attempts, CORE-181 (root frame swap) and
CORE-182 (interior scoping), changed the page frame and per-box sizing
separately. Both were net negative. The frame swap alone did not move one
target page count, because each box still carried a physical `width` and
`height` that the horizontal machinery reads as inline and block extent
(`docs/lessons/engine-layout.md`, "Relabelling the page frame does NOT
relabel box dimensions"). The root frame, the per-box sizing and the
out-of-flow frame therefore share one contract and land in dependency order.

## Goals

- One definition of the coordinate space, the physical origin, the logical
  axes and the extent that continuation consumes.
- A mapping table from logical to physical for sizes, min/max, margin, border,
  padding, percentages, viewport units and direction.
- A list of what stays physical.
- A decision for each open boundary question (fragmentation-core, CORE-184).
- Target and canary test IDs, measured at a named commit.

## Non-Goals

- Implementing any of it. Code changes belong to the child issues.
- Glyph orientation and vertical line stacking (child 4/5, CORE-555; see
  its implementation notes).
- `text-combine-upright`, `text-orientation` values beyond `mixed`/`upright`/
  `sideways` defaults, ruby, and vertical-aware `text-align`.
- Vertical multicol, vertical flex and vertical grid. They stay horizontal
  until a spec for each exists. A vertical box that is a flex or grid item
  keeps its horizontal-tb internal layout (see Behavior 24).
- Changing the CLI page-size contract or the harness geometry.

## Terms

| Term | Meaning |
|---|---|
| Physical axis | `x` (left to right) and `y` (top to bottom) in PostScript points, origin at the top-left of the page. |
| Block axis | The axis along which block boxes stack. `y` in `horizontal-tb`; `x` in every vertical mode. |
| Inline axis | The axis along which text runs. `x` in `horizontal-tb`; `y` in every vertical mode. |
| Page flow mode | The writing mode of the page's own flow: `@page { writing-mode }` if cascaded, else the root element's computed mode (css-page-3 §3). |
| Mode in effect at a box | The nearest ancestor-or-self `writing-mode` declaration, else the page flow mode (`Ctx::writing_mode_at`). |
| Orthogonal | Two modes are orthogonal when exactly one of them is vertical (css-writing-modes-3 §7.1). |
| Fragmentainer | One page content box (or one column inside it). |
| Anchor page | The page that holds a box's first fragment. |
| Consumed extent | The block-axis extent a box's fragments have used so far (`BreakToken::consumed_block_size`). |

## Behavior

### A. Coordinate space

1. **Physical space is the only space that is stored.** Every `Fragment`
   offset, every `FragmentContent` rectangle and every emitted PDF object is
   in physical points. Fragment offsets stay PARENT-relative, as today. No
   fragment ever stores a "virtual" coordinate after layout returns.
2. **The origin is the top-left of the page**, `y` down, for every writing
   mode. Direction (`rtl`, `vertical-rl`) never moves the origin. It changes
   where the first box lands inside the content box (Behavior 10).
3. **A layout frame is a pair of axis labels plus a rectangle.** The page flow
   builds one frame from the page content box and the page flow mode. The
   frame carries: the physical rectangle; which physical axis is the block
   axis; whether the block axis runs toward `+` (`vertical-lr`, `sideways-lr`,
   `horizontal-tb`) or `-` (`vertical-rl`, `sideways-rl`); and whether the
   inline axis runs toward `+` (all modes except `sideways-lr`) or `-`.
4. **The block path shall run in logical coordinates and map once.** The
   block path may lay a subtree out in a logical frame (inline offset, block
   offset). It shall map that subtree to physical coordinates exactly once, at
   the boundary of the subtree, before the parent consumes the fragment. No
   physical coordinate is mapped twice. The mapping formulas are in
   [Frame mapping](#frame-mapping).
5. **Mapping is applied to the subtree root only.** Child offsets are
   parent-relative, so mapping the root rewrites one offset and one size.
   Mapping every node recursively compounds at each nesting depth
   (`docs/lessons/engine-layout.md`, "Fragment offsets are PARENT-relative").
   A subtree whose descendants are all in the same mode as its root is laid
   out in that mode's logical frame and mapped as a unit. A descendant that is
   orthogonal to its parent starts its own frame (Behavior 20).

#### Frame mapping

Let the physical content rect be `(cx, cy, cw, ch)`. Let a logical box have
inline offset `i`, block offset `b`, inline size `I` and block size `B`
(relative to the frame origin).

| Mode | Physical `x` | Physical `y` | Physical `w` | Physical `h` |
|---|---|---|---|---|
| `horizontal-tb`, `ltr` | `cx + i` | `cy + b` | `I` | `B` |
| `horizontal-tb`, `rtl` | `cx + cw - i - I` | `cy + b` | `I` | `B` |
| `vertical-rl`, `sideways-rl` | `cx + cw - b - B` | `cy + i` | `B` | `I` |
| `vertical-lr` | `cx + b` | `cy + i` | `B` | `I` |
| `sideways-lr` | `cx + b` | `cy + ch - i - I` | `B` | `I` |

With `direction: rtl` in a vertical mode, the inline axis reverses: replace
`cy + i` with `cy + ch - i - I` (not for `sideways-lr`, where `rtl` restores
the `cy + i` form). The mapping is a pure function of the frame and the
logical box. It has no side effects and no dependence on the page number.

### B. Containing block, definite and indefinite sizes

6. **A containing block is a logical rectangle with a known flag per axis.**
   Each containing block carries `inline_size: Option<Scalar>` and
   `block_size: Option<Scalar>`. `Some` means definite. `None` means
   indefinite (content-based). The existing `Ctx::content_width` and
   `Ctx::page_height` become the page frame's definite inline and block
   extents. `specified_extent` (CORE-167) continues to model DECLARED
   extents; it shall be extended to any definite containing-block extent
   (Behavior 8).
7. **The page frame is definite in both axes.** For the root/body chain the
   fragmentainer is the containing block. Its inline size is the page content
   box extent along the inline axis. Its block size per fragmentainer is the
   page content box extent along the block axis.
8. **An interior vertical box inside a horizontal flow has an indefinite
   containing-block inline extent** unless an ancestor declares a definite
   one. A box fills its containing block's inline size only when that size is
   definite. This is the rule that `page-name-orthogonal-writing-003` pins:
   keying the fill on the box's own mode grew an interior `vertical-rl`
   wrapper to 210pt of a 216pt page and pushed its second child to a new page
   (1 page became 2). The guard test is
   `engine/tests/page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow`.
9. **Percentages resolve against the containing block's definite extent on the
   axis the property names after mapping.** A percentage `width` resolves
   against the physical width of the containing block. A percentage
   `inline-size` resolves against its inline size. A percentage against an
   indefinite extent behaves as `auto` (css-sizing-3 §5.2). Percentage
   margins and padding resolve against the containing block's INLINE size
   (css-box-3 §6.1 / css-writing-modes-3 §7.2), which is physical HEIGHT in a
   vertical mode.
10. **Block-start anchoring follows the mode in effect at the box.** In
    `vertical-rl` and `sideways-rl` a block's first box sits at the right edge
    of the frame and stacks leftward. In `vertical-lr`, `sideways-lr` and
    `horizontal-tb` it sits at the left (or top) and stacks forward. The
    existing rule (CORE-153/182, `Ctx::writing_mode_at`) is correct and stays.

### C. Fragmentainer bounds, anchor page, consumed extent

11. **Fragmentainer bounds are logical.** A fragmentainer has a block-axis
    start and a block-axis limit (`bottom_limit` in horizontal flow). In a
    vertical flow the limit is the physical left edge (`vertical-rl`) or right
    edge (`vertical-lr`) of the page content box. Code shall name this value
    `block_limit` in new vertical code. It shall not reuse `bottom_limit` for a
    horizontal limit.
12. **Page N owns the logical block range `[N·H, (N+1)·H)`** where `H` is the
    fragmentainer block extent. Content moves to page N+1 when its logical
    block offset reaches the limit. In a root-vertical document this means
    pages advance along physical `x`. `vertical-rl` content on page 2 starts
    at the right edge of page 2 (pages themselves are not mirrored; only the
    content inside each page is).
13. **The anchor page of a box is the page that holds its first fragment.**
    For in-flow boxes it is the page of its block-start offset. For
    out-of-flow boxes it follows the rule in out-of-flow-positioning.spec.md
    Behavior 9: `floor((ty − content_origin) / H)` along the block axis. A
    horizontal out-of-flow subtree inside a vertical root uses the PHYSICAL
    frame, so its anchor page is computed against physical `y` (Behavior 17).
14. **Consumed extent is measured on the box's own block axis, and in the
    fragmentation direction.** `BreakToken::consumed_block_size` counts the
    block-axis extent the box has used in the CURRENT fragmentation flow. For
    a box orthogonal to the flow, its own block axis is not the flow's block
    axis, so its continuation uses the flow axis: the box is monolithic in the
    flow's block direction unless a later child (CORE-554) defines orthogonal
    continuation.
15. **Progress is separate from paint size, and termination is guaranteed.**
    See [Declared height versus continuation progress](#declared-height-versus-continuation-progress).
    Every continuation fragment shall place content or consume extent. The
    page loop shall stop in at most `ceil(total_extent / min_progress)` pages.

### D. Logical-to-physical mapping per property

Each later child maps logical to physical through the table below. The mapping
is a pure function from (computed style, mode in effect) to physical values,
computed in the engine's own cascade pass because stylo's servo build does not
compute logical sizes for vertical modes (it resolves `width`/`height` as if
the box were horizontal and returns the initial `writing-mode`).

16. **Where the mapping runs.** The mapping shall run once per element, in
    the engine's own cascade pass, after the per-element `writing_mode` is
    known. It shall produce physical `ComputedStyle` fields. Layout code
    shall not branch on mode for sizes. It reads the physical fields as it
    does today. The fields it must keep consistent with the mapping:
    `width`, `height`, `min-*`, `max-*`, `width_percent`, `height_percent`,
    `margin_*`, `padding_*`, `border_*`, `box_sizing`.

| Logical property | `horizontal-tb` | vertical modes |
|---|---|---|
| `inline-size` / `min-inline-size` / `max-inline-size` | `width` / `min-width` / `max-width` | `height` / `min-height` / `max-height` |
| `block-size` / `min-block-size` / `max-block-size` | `height` / `min-height` / `max-height` | `width` / `min-width` / `max-width` |
| `margin-inline-start` / `-end` | `margin-left` / `-right` (`rtl`: swapped) | `margin-top` / `-bottom` (`rtl` and `sideways-lr`: swapped) |
| `margin-block-start` / `-end` | `margin-top` / `-bottom` | `margin-right` / `-left` for `-rl`; `margin-left` / `-right` for `-lr` |
| `padding-*`, `border-*-width` (logical) | same pattern as margin | same pattern as margin |
| `inset-inline-*`, `inset-block-*` | `left`/`right`, `top`/`bottom` | `top`/`bottom`; `right`/`left` (`-rl`) or `left`/`right` (`-lr`) |

17. **Physical properties keep their physical meaning.** `width` is always
    physical width, `margin-left` is always the left margin, `top` is always
    the physical top inset. Writing mode never reinterprets a physical
    property. This follows css-writing-modes-3 §7 and the CSS-wins rule in
    `docs/conventions/css-standards-alignment.md`.
18. **Declared sizes land on the axis the property names.** A vertical box
    that declares `width: 600px; height: 100px` has a 600pt physical width
    (its BLOCK extent) and a 100pt physical height (its INLINE extent). The
    horizontal machinery shall read these through a logical accessor
    (`inline_extent()` / `block_extent()`) so the swap happens in one place.
    This replaces the "style transpose" that the CORE-181 design called for:
    the accessor is the transpose, and no second copy of the style exists.
19. **Auto inline size fills the containing block when it is definite.** An
    auto inline size in a vertical box under a vertical page flow fills the
    page content box extent along `y` (page-size-012: a vertical auto-height
    div is 550x400, matching a `height: 400px` reference). Under a horizontal
    parent it follows Behavior 8 (content-based).
20. **Orthogonal boxes start a new frame.** A box whose mode is orthogonal to
    its parent's gets an independent frame: its content box becomes the
    frame rectangle, its inline size is its containing block's inline size
    when definite (else shrink-to-fit), and its block size is auto from
    content. It maps to the parent frame as an atomic box. Its descendants
    inherit its mode. This is the case CORE-554 implements.
21. **Min/max and percentages follow the table.** `min-height`/`max-height` on
    a vertical box clamp its INLINE extent. They use the min-max-sizing
    precedence in `min-max-sizing.spec.md`, applied on the logical axis.
22. **Viewport units are physical.** `vw`, `vh`, `vmin`, `vmax` resolve against
    the page box (paged-media spec 3a), never against the mode. `vw` is
    always page width. In a vertical mode, `block-size: 210vw` is a block
    extent of 2.1 page widths, which is what `block-001-wm-*` rely on.
23. **Direction mirrors the inline axis only.** `direction: rtl` flips the
    inline-start side (physical right in `horizontal-tb`, physical bottom in
    `vertical-*` except `sideways-lr`). It never changes the block axis, and
    it never changes the page progression of a flow.
24. **Unsupported containers stay horizontal.** A vertical box that is a flex
    item, a grid item, a table cell or a multicol column keeps its
    `horizontal-tb` internal layout in v1 and reports `writing-mode` via
    computed style only. A diagnostic note is not required. The behavior is
    pinned by a test in the child that first touches the container.

### E. What stays physical

25. **Page chrome stays physical.** The `@page` background, canvas fill,
    border rings, outlines, bleed and crop marks, and the `Fragmentainer`
    content origin and size are stored and painted in physical coordinates
    (`Fragmentainer::content_origin`, `content_size`). Logical layout of the
    body never changes these two values.
26. **Margin boxes stay physical.** The 16 margin boxes are placed by
    `attach_margin_boxes` against `spec.geometry()` in physical coordinates.
    Their LOGICAL page margins map through the page context mode (already
    done in `paged.rs`: `logical_margins_map_per_writing_mode`,
    `logical_margins_inherit_root_writing_mode`). Margin-box CONTENT
    measurement in vertical modes is out of scope here; see
    [CORE-184 boundary](#core-184-boundary).
27. **Footnotes stay physical.** The footnote band is reserved from the
    physical page bottom in `footnote_area_height` and painted by
    `attach_footnotes`. In a vertical root the band is a physical bottom
    band: the notes are laid horizontally against the physical content rect,
    unrotated, and lie inside it. The band consumes the body's INLINE extent
    (the physical height), so the body is re-laid at the inline extent above
    the band and no body fragment paints into it. Making the band follow the
    block-end edge is a separate decision, deferred by this spec (not planned
    in epic CORE-203).
28. **A horizontal out-of-flow subtree inside a vertical root uses a physical
    frame.** An `absolute`/`fixed` box whose mode is `horizontal-tb` inside a
    vertical root is laid out in the PHYSICAL frame and attached to the
    fragmentainer root. It is not transposed. The containing block it reads
    comes from a second, physical `Ctx` (`Ctx::phys_frame`, the physical page
    content rect and ICB) that the out-of-flow branch uses for such a box
    (`Ctx::layout_physical_abspos`). Its insets resolve against the physical
    page content box; the box is monolithic. A `horizontal-tb` abspos box
    whose containing block is a positioned ancestor inside the vertical body
    is a mixed frame this spec does not model: it takes the ordinary branch
    and maps with its ancestor. Every `body-background-*` fixture carries one
    page-anchored box of the modelled kind.
29. **Out-of-flow placement shares the coordinate contract.** The abspos path
    (CORE-204, landed as `0a7e374`) places a box in page-absolute physical
    coordinates and decides fragmentation against the REAL fragmentainer
    bottom `content_y + page_height`, never against an inherited
    `bottom_limit`. The vertical children shall keep that: for an abspos box
    in a vertical flow, the "real fragmentainer limit" is the logical
    `block_limit` (Behavior 11), and the owning page is computed by
    Behavior 13. The abspos containing-block tuple `Flow::abspos_cb: (Point,
    Scalar)` carries an origin and one width only. A vertical child shall
    extend it to `(origin, inline_extent, block_extent_if_definite)` before it
    relies on bottom-inset or percentage-height behavior in a vertical frame.
30. **Paint is order-preserving.** The emitter collects backgrounds, borders
    and text in separate passes. A mapped subtree shall keep the same
    fragment order, so z-order is unchanged. Text runs map by their absolute
    baseline, re-relativised against the mapped parent origin. A run's
    baseline stays PARENT-relative (`docs/lessons/engine-layout.md`, "A Text
    run's baseline is PARENT-relative"). Glyph rotation is child 4/5.

### Declared height versus continuation progress

`fragmentation-core.spec.md` holds two statements that disagree:

- Behavior 11 says an extent crossing the fragmentainer edge "paints through
  it without emitting a continuation — empty declared-height boxes are
  treated as monolithic ... true extent fragmentation is future work".
- The Addendum "Explicit-Height Block Continuation" (landed with CORE-152 and
  CORE-167) says an ordinary block with a resolved `height` SHALL continue on
  every page until its declared extent is consumed.

**DECIDED 2026-10-23 (agent):** The Addendum governs in-flow ordinary blocks.
Behavior 11's "paints through" clause is superseded for them. Its monolithic
wording remains correct only for the cases it still describes: pinned abspos
boxes that fit one fragmentainer (out-of-flow-positioning.spec.md Behavior 9),
replaced elements and lines. Why: the code at `layout.rs`
(`consumed_block_size` update in `layout_box`) already implements the
Addendum, `block-001-wm-*` bisects to CORE-152's continuation
(`f86afaf`; the CORE-224 bisect recorded 1 page becoming 12 at a
historical hash that is unresolved in current history, and `f86afaf` is the
current-history commit of that change, not re-bisected here) and Chromium passes those tests, so the
Addendum is the behavior that exists, that CSS requires (css-break-3 §5.3),
and that the WPT pairs measure.

The progress model has three separate quantities. Code and spec shall never
substitute one for another:

1. **Paint size.** The border box a fragment paints, per fragment. Content
   taller than the declared height still paints (`max(content, declared)`).
2. **Placed content extent.** The block-axis extent of content actually placed
   in this fragment.
3. **Consumed extent (progress).** What the next fragment subtracts from the
   declared extent. For a declared-height box that continues, it is measured
   to the FRAGMENTAINER EDGE (css-break-3 §5.3), not to the placed content
   bottom, and not to the paint size.

Rules (these restate Addendum items 1-8 on the logical axis; the Addendum
text is unchanged apart from a pointer to this section):

31. Consumed extent is a function of the box's declared extent and the
    fragmentainer bounds only. It does not read the paint size of any
    fragment.
32. Consumed extent is measured on the FLOW block axis (Behavior 14). For a
    root-vertical document that is physical `x`. A box orthogonal to the flow
    makes no declared-extent progress until CORE-554 defines it.
33. Termination: every fragment either places content or has consumed extent
    greater than zero, or it is the final fragment. A token with zero remaining
    extent and no pending children ends the box. The page loop already caps at
    100,000 pages; vertical children shall not raise that cap.
34. A declared extent of zero or less generates no continuation.

### CORE-184 boundary

CORE-184 (margin-box line metrics, direction-aware sizing and content
placement) is parked (user decision 2026-09-28). Its writing-mode slice
(feature commit `eddc561`) was rolled back in `a627129`. Its recovery
evidence (`~/workspace/typeanvil-evidence/CORE-184/recovery-aab688b/`; the
directory name keeps an older hash that is unresolved in current history) shows that
real-font `vertical-rl` glyph advances stayed horizontal in margin boxes.

**DECIDED 2026-10-23 (agent):** The boundary follows what each side owns.

- **CORE-184 owns** margin-box content measurement and placement in the
  margin box's OWN writing mode: its min/max-content sizes, line count,
  `BoxMetrics` inputs, the dimensions-004/006/013/014 family, and the
  rotation of margin-box lines.
- **CORE-203 children own** the document-flow side: the page frame and
  logical page margins (done), the shared mapping table (Behaviors 16-23),
  the shared `inline_extent()`/`block_extent()` accessors, and glyph
  orientation for real fonts (CORE-555). CORE-555 delivers a reusable
  per-run orientation primitive. CORE-184 consumes it. CORE-184 shall not
  build a second one.
- **Shared rule:** margin boxes use the same frame mapping (Behavior 4) and
  the same mapping table (Behavior 16). CORE-184 may not introduce a margin-
  box-only axis convention.
- **Order:** margin-box vertical work waits for CORE-555. This matches the
  evidence: the discarded slice failed because glyph advances were not
  oriented, which is CORE-555's scope.
- No CORE-203 child touches `attach_margin_boxes` content measurement.

## Interfaces

New internal types and functions. Names are binding for the children so they
do not diverge. File locations follow the existing module map.

```rust
// engine/src/css.rs (child 1/5, CORE-552)
impl PageWritingMode {
    pub fn block_is_horizontal(self) -> bool;   // every mode except HorizontalTb
    pub fn block_runs_negative(self) -> bool;   // VerticalRl | SidewaysRl
    pub fn inline_runs_negative(self, rtl: bool) -> bool; // SidewaysLr xor rtl, vertical only
}

// engine/src/css.rs — logical accessors over the physical fields
impl ComputedStyle {
    pub fn inline_size(&self) -> Dimension;     // width in htb, height in vertical
    pub fn block_size(&self) -> Dimension;
    pub fn min_max_inline(&self) -> (SizeLimit, SizeLimit);
    pub fn min_max_block(&self) -> (SizeLimit, SizeLimit);
}

// engine/src/layout.rs (child 2/5, CORE-553)
struct Frame { rect: Rect, mode: PageWritingMode, rtl: bool }
impl Frame {
    fn to_physical(&self, i: Scalar, b: Scalar, inline: Scalar, block: Scalar) -> Rect;
    fn block_limit(&self) -> Scalar;            // Behavior 11
}
```

Signatures are a starting contract. A child may change a signature only by
editing this section in the same PR.

## Implementation notes — child 1/5 (CORE-552, landed 2026-10-23)

**DECIDED 2026-10-23 (agent):** Behavior 16 is implemented through stylo, not
a second hand-written mapper. The engine sets the stylo pref
`layout.writing-mode.enabled` in `Stylesheet::parse`. With it on, the servo
build resolves `inline-size`, `block-size`, `min-/max-*`, `margin-*`,
`padding-*` and logical insets onto the physical `ComputedStyle` fields, per
element, against that element's own mode and direction. Why: the earlier
claim that stylo "returns the initial writing-mode" described the pref being
off. The pref route follows the "prefer stylo" ruling and keeps one mapping.

Three gaps stay in the engine's own passes:

- **`sideways-rl` / `sideways-lr`.** The servo build rejects both keywords.
  For the stylo walk only, `css.rs::logical_sizing::rewrite_sideways` reads
  `sideways-rl` as `vertical-rl` and `sideways-lr` as `vertical-lr` with
  `direction: rtl`. Later passes keep the original text and record the true
  mode. The rewrite touches only declarations: a `writing-mode` value and a
  `margin-block-*` property name. Comments, strings and selectors pass
  through unchanged. `margin-block-*` is rewritten only when the document
  names a vertical mode. A non-sideways-lr child of a sideways-lr box
  inherits the stand-in `rtl`. The cascade pins that child's true direction
  and walks again, so a `horizontal-tb` subtree keeps its own inline edges
  (test `horizontal_subtree_in_sideways_lr_keeps_its_direction`).
  `restore_sideways_lr_direction` then flips `rtl` back on the sideways-lr
  boxes.
- **Logical borders and viewport units.** The border pass and the
  viewport-unit pass map `border-{block,inline}-*` and `inline-size` /
  `block-size` in `vw`/`vh` by the mode in effect (`mode_at`).
- **Logical border styles (CORE-562).** stylo maps
  `border-{block,inline}-*-style` to the physical sides and zeroes the width
  of a `none`/`hidden` side. The declared-width pass, which restores widths
  stylo snapped to device pixels, shall not revive a side stylo computed as
  0. Before the fix, `border: 5px solid; border-block-end-style: none`
  painted all four sides (`block-001-wm-*` refs). Test:
  `writing_modes.rs::logical_border_style_none_removes_mapped_side`.
- **Block-axis margins (resolved by CORE-553).** Until CORE-553,
  `margin-block-start/end` kept the `top`/`bottom` slots in a vertical mode.
  CORE-553 removed that stand-in in the same change as the frame swap: stylo's
  mapping (right/left for `-rl`, left/right for `-lr`) now applies. Test:
  `writing_modes.rs::block_margins_in_vertical_modes_map_to_block_edges`.

## Implementation notes — child 2/5 (CORE-553, 2026-10-23)

**DECIDED 2026-10-23 (agent):** the root-vertical body is laid out in a
logical (transposed) frame and mapped to physical points once per page.

- `engine/src/vertical.rs::transpose_styles` copies the cascaded styles and
  rotates every box whose mode in effect is vertical into the root frame:
  width/height swap (with percent, viewport and min/max), and margins,
  padding, borders, border colours/styles and insets permute geometrically
  (`logical_to_physical_sides`). Viewport lengths swap axis, so `vw` stays
  the physical page width (Behavior 22).
- `paginate` lays the body with those styles against a swapped content rect
  (origin 0,0, width = physical height, height = physical width). The
  existing break tokens, declared-extent continuation and page-loop cap run
  unchanged on the block axis (Behaviors 11-15, 31-34).
- `vertical::map_root` maps each page's body subtree, abspos pool and drain
  fragments to physical points, parent-relative, once (Behavior 4-5). Text
  lines keep their logical interior and paint under a 90° matrix
  (`Fragment::transform`). Link rects map through `map_rect`.
- Margin boxes, the canvas fill, page border rings and fixed boxes never
  enter the logical frame (Behaviors 25-26). Fixed boxes keep the physical
  styles.
- Footnotes (Behavior 27): when the full-extent body attempt registers notes,
  the body is re-laid once at a logical inline extent shortened by the band
  height (`sideways-lr` starts the inline axis `band` later, since its inline
  axis runs bottom-up), the notes that attempt places size the band, and
  `attach_footnotes` attaches them after the mapping against the physical
  content rect. Test `footnotes_in_vertical_root_stay_in_a_physical_bottom_band`.
- Horizontal abspos (Behavior 28): a `horizontal-tb` `position: absolute` box
  whose containing block is the page lays out through
  `Ctx::layout_physical_abspos` against `Ctx::phys_frame`, collects in
  `Flow::abspos_physical`, and attaches after the mapping. Its link rects are
  tagged `physical` and skip the page-level map. Test
  `horizontal_abspos_in_vertical_root_is_not_transposed`.
- `Ctx::logical_frame` disables the CORE-153 physical-frame rules (vertical-rl
  right anchoring, inline fill) inside the logical frame, where they are
  implied by the mapping.
- `vb`/`vi` units resolve against the root mode in the viewport-unit pass
  (`page-box-008` uses `block-size: 100vb`).

Limits carried to later children:

- A `horizontal-tb` IN-FLOW subtree inside a vertical root is copied
  untransposed and rides the root mapping. General interior orthogonal boxes
  are CORE-554. The page-anchored abspos case is delivered here (Behavior 28).
- **Interior vertical slice (DECIDED 2026-10-23, agent; folded in to keep
  `page-size-012` green).** `Ctx::layout_interior_vertical` lays a vertical
  block inside a horizontal page flow in its own logical frame as one
  monolithic box, mapped once. Gate: `Ctx::is_simple_interior_vertical` — the
  box and its subtree are plain (`display` block/inline/none, no float,
  positioned box, multicol or transform, no other declared mode), and every
  ancestor is a plain block. An auto inline size is fit-content bounded by the
  page content height (css-writing-modes-3 §7.3). The box's physical margins
  resolve in its horizontal containing block, so an over-constrained
  `margin-right` no longer right-anchors it (this supersedes the CORE-182
  anchoring test). Link rects inside the box are dropped. Everything outside
  the gate keeps the existing path and stays with CORE-554.
- **Interior vertical anchoring (DECIDED 2026-10-27, lead; CORE-555).** A
  vertical box inside a horizontal-tb ltr containing block sits at the start
  (left) edge plus `margin-left`, in every vertical mode (CSS 2.1 §10.3.3;
  Chromium agrees). Text-only interior vertical boxes skip
  `layout_interior_vertical` and take the CORE-555 vertical line path, but
  keep this anchoring.
- `html` root margins are ignored in every mode (`probe/root-margin-*.html`),
  which keeps `block-002-wm-*` at test=2 / ref=3 pages. Split out as
  CORE-563. Logical border styles (`block-001-wm-*` pixel residual) are
  CORE-562.

Unchanged by this child: the layout code still reads physical fields and has
no `inline_extent()` accessor yet. The `Dimension` type and the
`PageWritingMode` helpers (`block_is_horizontal`, `block_runs_negative`,
`inline_runs_negative`) are in `css.rs`. Tests: `engine/tests/it/writing_modes.rs`.

## Implementation notes — child 5/5 (CORE-556, 2026-10-23)

**DECIDED 2026-10-23 (agent):** the canvas gradient is one strip in
document order. Chromium confirms it at matching settings.

- **Evidence.** Each fixture and reference was printed with Chromium
  (Playwright `page.pdf`, `prefer_css_page_size`, backgrounds on) and with the
  engine at the harness flags (`--page-width 5in --page-height 3in`, 0.5in
  margins). Every PDF has 2 pages of 600x450pt (800x600px). The fixture's
  `@page size` overrides the CLI flags, and `margin: 0` removes the margins.
  Chromium paints page 1 lightgray and page 2 white for `vlr`, `vrl` and `slr`.
  It paints page 1 white and page 2 lightgray for `srl`. Chromium and its
  reference agree for all four. The `srl` assertion text ("first page is
  lightgray") is wrong: the `to left` gradient and the reference's
  white-first gradient both put white on page 1. The engine follows the
  gradient. Evidence: `~/workspace/typeanvil-evidence/CORE-556/`.
- **Rule.** All page content areas are laid end to end along the page
  progression axis, in document order, with page 1 at the strip origin. The
  axis is `y` for a horizontal root and `x` for a vertical root. The order is
  document order for `vertical-rl` too, where pages progress right to left.
  The gradient line is sized to the whole strip (css-images-3 §3.1.1). Each
  page paints its own part (`Fragmentainer::canvas_gradient`,
  `frag::CanvasGradientSlice`, `layout::assign_canvas_gradient_slices`).
- **Scope.** `css.rs::linear_gradient_style` models non-repeating
  `linear-gradient` with angle, side or corner directions, plus color stops
  that have percentage, length or auto positions. Repeating, radial or conic
  gradients, interpolation hints and non-absolute colors are not modelled,
  and the image is ignored as before. Element boxes still do not paint
  gradients (CORE-319). An all-same-color gradient keeps the CORE-153 solid
  collapse.
- **Seams.** Each page shades only its own segment of the strip line, so a
  hard stop that falls on a page seam does not bleed onto the next page.
- **Root lookup (not changed).** `resolve_canvas_background` reads
  `styles[dom.root]`, which is the document node, so an `html` background
  never owns the canvas. Reading `<html>` instead flips
  `page-background-image-print` from PASS to FAIL, because `@page
  background-image` does not paint. Both fixes move to CORE-564.
- **Result.** `body-background-vlr-print` passes. `vrl`, `slr` and `srl`
  stay FAIL for box-placement residuals on the reference side, tracked in
  CORE-564. Their canvas slices match Chromium.
- Tests: `engine/tests/it/canvas_gradient.rs`. They check slice offsets and
  painted colors against the Chromium page colors. They do not compare two
  renders of the engine.

## Implementation notes — root element margins (CORE-563, 2026-10-24)

**DECIDED 2026-10-24 (agent):** the root element's margins inset the body
rect in `paginate`, in the logical frame of the page flow.

- **Rule.** The `html` box's inline-start and inline-end margins inset every
  page. Its block-start margin offsets page 1 only, because a margin that
  adjoins a break is truncated (css-break-3 §5.2). A block-start margin larger
  than the page clamps to the page extent. The block-end margin is not
  modelled. In a vertical root the styles are already transposed, so the
  block-start margin lands on the right edge (`vertical-rl`) or the left edge
  (`vertical-lr`) after `vertical::map_root`.
- **Viewport-unit margins.** `margin-top/right/bottom/left` and the logical
  `margin-block-*`/`margin-inline-*` longhands with a plain `Nvw`/`Nvh` (or a
  `calc(Nvw ± Mpx)`) value resolve against the page content box in the
  viewport-unit pass (`css.rs` `viewport_units`). Logical sides map to
  physical sides with the mode in effect at the element. The `margin`
  shorthand stays with stylo.
- **Exact fit.** The declared-extent slice check in `layout_box` allows a
  1e-6pt tolerance. Without it, `80vw + 20vw` overshoots the page width by a
  rounding error, and the second box slices into an empty fragment and an
  extra page.
- **Result.** The `block-002-wm-vrl` and `block-002-wm-vlr` tests render 3
  pages like Chromium (2 before). Their references render 4 pages, where
  Chromium renders 3. The same shape renders 4 pages in horizontal-tb on the
  base binary: an overflowing child's continuation pushes the next sibling.
  That defect is CORE-565, so both tests stay FAIL. **Update (CORE-565,
  2026-10-24):** fixed; both references render 3 pages and both tests
  PASS. See fragmentation-core Behavior 11. `root-margin-001` now
  leaves page 1 blank as it asserts. It stays FAIL on a pixel residual.
- Tests: `engine/tests/it/core563_root_margins.rs`.

## Implementation notes — child 4/5 (CORE-555)

**DECIDED 2026-10-23 (agent):** glyph orientation and vertical line
stacking are set inside the box. Box sizing and page progression stay with
CORE-553/554.

- **Which boxes.** A block whose mode in effect is vertical and whose
  children are text only (no block or atomic children, no active floats)
  sets its lines as columns. Table cells and flex/grid items keep their
  horizontal-tb internal layout (Behavior 24). Mixed block/text content in
  a vertical box keeps the horizontal path for now.
- **Column length (inline extent).** The declared content height, else the
  fragmentainer room under a vertical page flow (the box fills its inline
  axis, Behavior 19), else the max-content length capped at the room
  (Behavior 8: an interior vertical box has an indefinite inline extent).
- **Stacking.** Each line is one `line-height` wide. Lines stack from the
  content box's right edge leftward in `vertical-rl`/`sideways-rl` and from
  the left edge rightward in `vertical-lr`/`sideways-lr`. `sideways-lr`
  runs bottom to top; the other three run top to bottom. `text-align`
  applies along the inline axis.
- **Orientation.** `text-orientation` is parsed by the paged pass (the
  servo stylo build gates it off) and inherits. `mixed` (the default) sets
  CJK, kana, hangul, Yi, fullwidth and CJK symbol characters upright
  (an approximation of UAX #50 U/Tu/Tr) and rotates every other character
  90° clockwise. `upright` sets every character upright. `sideways` and the
  two sideways modes rotate every character (clockwise, or
  counter-clockwise in `sideways-lr`). Upright runs shape with HarfRust
  `TopToBottom`, so they use vertical advances, vertical origins and the
  `vert` feature. The orientation rides in `ot_features` as a pseudo tag
  (`typography::ORIENTATION_TAG`), so the shape cache keys on it.
- **Primitive for CORE-184.** The reusable pieces are
  `typography::ORIENTATION_TAG` (shaping), `ShapedGlyph::upright` /
  `y_offset`, and `Fragment::orientation` (`frag::LineOrientation`) plus
  `pdf::draw_shaped_vertical` (paint). A margin box sets the same fields.
- **Paint.** A vertical run's baseline point is the start of its central
  baseline (column centre). Rotated segments paint under a 90° matrix with
  the em-box centre on the column centre; upright glyphs paint unrotated at
  their vertical-origin offsets. Glyph text ranges are unchanged, so the
  ToUnicode map and the marked-content order follow the source text.
- **Text extraction.** In tagged output, the columns of one vertical
  paragraph shall paint inside ONE marked-content span whose `/ActualText`
  is the paragraph text in source order. Columns rejoin with one space,
  except after a synthetic hyphen or between two CJK characters.
  Extractors order separate runs by position, so without the span a
  `*-rl` paragraph (columns stack right to left) extracts last line first.
  Untagged output (`--no-tagged`) has no marked content and keeps
  position-ordered extraction.
- **Fragmentation.** A vertical paragraph is monolithic in the flow. One
  that does not fit the room left on the page moves whole to the next page
  (the `page-name-orthogonal-writing-002` reference shape).
- **Not in this child** (residual CORE-561): link
  rectangles and inline backgrounds in vertical lines,
  `text-combine-upright`, `direction: rtl` in vertical lines, splitting a
  vertical paragraph across pages, and mixed block/text vertical boxes.

Tests: `engine/tests/it/core555_vertical_text.rs` (real fonts; content
stream matrices for orientation, fragment geometry for stacking, decoded
`/ActualText` and ToUnicode for the text layer, byte equality for
determinism).

## Child issue map

| Child | Issue | Owns behaviors | Depends on |
|---|---|---|---|
| 0/5 spec | CORE-551 | this document | none |
| 1/5 logical sizing | CORE-552 | 16-19, 21-23 | 0/5 |
| 2/5 root-vertical progression | CORE-553 | 1-5, 11-15, 25-30, 31-34 | 0/5, 1/5 |
| 3/5 nested orthogonal flows | CORE-554 | 6-9, 14, 20, 24 | 1/5 (coordinate with 2/5) |
| 4/5 glyph orientation and vertical lines | CORE-555 | 30 (rotation), CORE-184 boundary | 1/5 |
| 5/5 canvas background slices | CORE-556 | canvas gradient per page | 2/5 |

Order of landing: 0/5, 1/5, then 2/5 and 3/5 together or in sequence, then 4/5,
then 5/5. 2/5 shall land the frame swap and the style mapping in ONE change,
with the canary pair `page-margin-002/003` green. This is the lesson of
CORE-181.

## Target and canary test IDs

Measured at release tip `9a5a00ee5de2` (2026-10-23), harness geometry 5x3in with
0.5in margins, container binary, `python -m harness run --engine cli`
with one filter per ID. Report files and logs:
`~/workspace/typeanvil-evidence/CORE-551/core551/`. "Moves" means the child
shall change the status of that ID or the numbers shown. These are measured
facts; they replace the historical counts.

### Targets (all FAIL at base)

| Test ID (`css/css-*/`) | Base result | Owner child |
|---|---|---|
| `css-page/body-background-vlr-print` | FAIL, pages test=1 ref=2 | 2/5 then 5/5 |
| `css-page/body-background-vrl-print` | FAIL, pages test=1 ref=2 | 2/5 then 5/5 |
| `css-page/body-background-slr-print` | FAIL, pages test=1 ref=2 | 2/5 then 5/5 |
| `css-page/body-background-srl-print` | FAIL, pages test=1 ref=2 | 2/5 then 5/5 |
| `css-break/block-001-wm-vlr-print` | FAIL, pixel difference | 1/5 then 2/5 |
| `css-break/block-001-wm-vrl-print` | FAIL, pixel difference | 1/5 then 2/5 |
| `css-break/block-002-wm-vlr-print` | FAIL, pages test=10 ref=15 | 1/5 then 2/5 |
| `css-break/block-002-wm-vrl-print` | FAIL, pages test=10 ref=15 | 1/5 then 2/5 |
| `css-page/page-box-008-print` | FAIL, pages test=2 ref=1 | 2/5 |

### Canaries (all PASS at base; shall stay PASS)

| Test ID | Base | Why it is a canary |
|---|---|---|
| `css-page/page-margin-002-print`, `-003-print` | PASS | Reference declares the root mode too, so a root frame swap transposes both sides. These prove the frame arithmetic. |
| `css-page/page-size-012-print` | PASS | Test has a vertical root. Reference has a horizontal root with interior vertical divs. Passes today because both render horizontal. Only the mode in effect at each box, applied on both sides, keeps it green (3/5). |
| `css-page/page-name-orthogonal-writing-001..004-print` | PASS (4/4) | Pin Behavior 8 and the orthogonal page-change suppression. |
| `css-page/page-box-009-print` | PASS | `@page { writing-mode }` on an interior `html`/`body` shall not inherit the new frame. |
| `css-page/monolithic-overflow-028-print` | PASS | Declared-extent continuation must not over-paginate. |
| `css-break/transform-022-print` | PASS | Page-orientation canary from CORE-181's frame experiment. |

### Reference-only observations (not owned by CORE-203)

| Test ID | Base | Owner |
|---|---|---|
| `css-page/margin-boxes/dimensions-004-print`, `-006-print` | FAIL, pixel | CORE-184 |
| `css-page/margin-boxes/dimensions-013-print`, `-014-print` | FAIL, pixel | CORE-184 (disputed, test-bug candidates per CORE-177) |
| `css-page/fixedpos-010-print` | FAIL, pixel | CORE-204 known exposed gap; must not get worse |
| `css-break/transform-024-print` | FAIL, pixel | transform/page-orientation |

## Measured facts carried forward

These were true when measured and still shape the contract. Anything not
re-measured at `9a5a00e` is marked as historical.

- **Frame swap alone does not move the targets** (historical, CORE-181,
  frame-swap measurement commit `e47eed0`). With the frame swapped and boxes still physical,
  body-background page counts stayed 1/2, page-box-008 went 2/1 to 5/1 and
  block-001/002 halved. Canaries `page-margin-002/003`, `page-size-012` and
  `transform-022` held. Conclusion: the frame arithmetic is sound; the style
  mapping (Behavior 16-18) is the missing half.
- **`block-00{1,2}-wm-*` passed by accident until CORE-152** (CORE-224
  bisect). Page count for `block-001-wm-vlr`: 1 at three earlier historical
  commits (`dc8b8bc`, `7db761b`, `845a7bb`; all unresolved in current history,
  quoted from the CORE-224 bisect); 12 from CORE-152's continuation, as the
  CORE-224 bisect recorded it. This 12-page count is historical. It was not
  re-measured at `f86afaf` or at the current tip `9a5a00e`; the current
  base result for this ID is the pixel difference in the Targets table. Chromium passes all four; each
  fixture spans about 2.5 page widths along the block axis. Both sides move
  together, so the gate reports a pair mismatch.
- **Eight targets contain no glyphs in six fixtures**: `block-001/002-wm-*`
  and three of four `body-background-*` fixtures use only boxes, borders and
  background colour. Root-vertical progression for them is geometry and
  paint, not text shaping. Real fonts and glyph orientation are CORE-555.
- **The gate is self-consistent** (CORE-182). The harness renders test and
  reference through the same engine, so a change applied to both sides is
  invisible to it. A zero-flip gate is not proof that rendering did not
  change. For this bucket the unit tests in `engine/tests/` are the guard of
  record (Acceptance criteria 1-8).
- **Canvas background strip model** (CORE-177, CORE-155). The canvas is the
  page-progression strip: gradient 0% at the start of page 1, 100% at the end
  of the last page, in DOCUMENT ORDER, not physical order. `vlr` and `vrl`
  declare the same `90deg` gradient yet progress in opposite directions;
  both show page 1 lightgray and page 2 white. `body-background-srl-ref`
  sets `linear-gradient(to bottom, white 0%, white 50%, lightgray 50%, ...)`
  (white first) while its own assertion says the first page is lightgray.
  Child 5/5 shall render that reference and read both pages before building
  the slice. Chromium fails these four on wpt.fyi but CORE-177 measured them
  as valid fixtures with minor Chromium gradient-stop residuals.
- **Mis-attributed families** (CORE-182). `firefox-bug-2026295-print` is a
  crashtest that needs element `content: url()` (CORE-183), not vertical
  layout. `margin-boxes/dimensions-004/006/013/014` need multi-line margin-box
  content and margin-box intrinsic sizing in their own mode (CORE-184). Do
  not count either family against CORE-203.
- **`sideways-lr` and `sideways-rl`** parse as vertical page-context modes in
  both the paged pass and the cascade (`PageWritingMode::SidewaysRl` /
  `SidewaysLr`, CORE-181 landed). The block-start anchoring and the `@page`
  logical margin map follow the block axis.
- **`vertical-lr` is not right-anchored**: only `vertical-rl` (and
  `sideways-rl`) right-anchor. `page-margin-003`'s ref breaks if the test is
  widened to `root_wm != HorizontalTb`.

## Acceptance Criteria

Each criterion names the behavior it checks and the test that proves it. The
spec shipped with criterion 0 only. Criteria 1-9 are the contract that the
children implement; each child adds its own tests and records the real test
names here. Tests marked `(new)` do not exist yet and shall be written RED
first.

| # | Behavior | Test | Asserts |
|---|---|---|---|
| 0 | Frontmatter, slug | `scripts/validate_docs.py` (with PyYAML) | The spec validates; `slug` starts with `/specifications/` and does not end in `.spec` |
| 1 | 4, 5, 12 | `engine/src/vertical.rs::map_frag` via `engine/tests/it/writing_modes.rs::root_vertical_progression_fragments_along_x` (vertical-rl, vertical-lr) and `layout::core153_vertical_rl_tests::sideways_modes_are_vertical_page_flows` (sideways-rl, sideways-lr); horizontal-tb is the identity path | The "Frame mapping" table holds per mode (block-start edge, swapped sizes). No standalone `Frame::to_physical` type exists; the mapping is `vertical::map_root` |
| 2 | 16, 18 | `engine/tests/it/writing_modes.rs::logical_sizes_land_on_named_axes` (new) | `width:600px; height:100px` under `vertical-rl` has physical block extent 600 and inline extent 100 |
| 3 | 19 | existing `layout::core153_vertical_rl_tests` + `page-size-012-print` | auto inline size fills the page content box in a vertical page flow; PASS |
| 4 | 8 | `page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow` | interior vertical wrapper stays one page (guard, exists) |
| 5 | 12, 13 | `engine/tests/it/writing_modes.rs::root_vertical_progression_fragments_along_x`, `root_vertical_block_end_margin_breaks_the_page` | A vertical-rl/-lr root whose block extent is 2.5 page widths paginates to 3 pages, content starts at each page's block-start edge; a block-end margin breaks the page |
| 6 | 22 | `engine/tests/it/writing_modes.rs::viewport_units_stay_physical` (new) | `block-size: 210vw` in vertical-rl equals 2.1 page widths; `100vh` is page height |
| 7 | 25, 26, 27 | Footnotes: `engine/tests/it/writing_modes.rs::footnotes_in_vertical_root_stay_in_a_physical_bottom_band`. Margin boxes and page border rings: no direct test; `attach_margin_boxes` and the chrome rings never enter the logical frame, and the canaries `page-margin-002/003` plus the review probe `review-chrome-vrl` (evidence `CORE-553/review-r1`) hold them | Footnote lines lie inside the physical content rect, unrotated, below every body fragment; margin boxes and rings keep physical rects in a vertical root |
| 8 | 28 | `engine/tests/it/writing_modes.rs::horizontal_abspos_in_vertical_root_is_not_transposed` | An abspos `horizontal-tb` box inside a vertical-rl/-lr/sideways-lr root carries no rotation, is wider than tall, and its `left`/`top` insets resolve against the physical page content box |
| 9 | 31-34 | `fragmentation.rs::height_continuation_*` (AC1-AC6, exist) plus `engine/tests/it/writing_modes.rs::root_vertical_progression_fragments_along_x` (a 1500pt declared block extent on a 600pt page ends in exactly 3 pages) | Consumed extent does not read paint size; a vertical declared-extent box ends in at most `ceil(extent / H)` pages |
| 11 | canvas strip (5/5) | `engine/tests/it/canvas_gradient.rs` (10 tests) | Page 1/2 slice colors match Chromium for vlr/vrl/slr (lightgray, white) and srl (white, lightgray); strip offsets run along `x` for vertical and `y` for horizontal roots; a body gradient is the canvas donor; the PDF carries a shading |
| 10 | targets | WPT gate (`python -m harness gate`) | Each child's gate report lists every ID in "Targets" and "Canaries" with before and after status; zero canary PASS to FAIL |

Child ownership of criteria 1-10 is in the child issue map. A child that
cannot meet a criterion records the shortfall under its own issue and files
the residual in the same create call with the relation set (AGENTS.md,
issue-evidence rules).

## Edge Cases

- **Mode change mid-tree and back.** A wrapper that switches to `vertical-rl`
  and a descendant that switches back to `horizontal-tb` is not orthogonal to
  the page flow for the descendant. Named-page suppression keys on the axis of
  the mode at the page-declaring box, not on any intermediate declaration
  (`orthogonal_flow`, orthogonal-writing-004).
- **`html` vs `body` declaration.** Only the root element sets the page flow
  mode. A `writing-mode` on `body` is an interior declaration unless `html`
  declares none and the author propagates it by CSS rules that make `body`
  the root's mode source. The engine reads the ROOT element only.
- **`@page { writing-mode }` without a root declaration.** The cascaded
  `@page` value wins for page margins and for the page flow mode. Interior
  `html`/`body` that declare a mode (`page-box-009`) do not inherit the new
  frame.
- **Zero or negative content box** (margins larger than the page): the frame
  rectangle clamps to zero extent; `block_limit` stays at the content origin;
  Behavior 33 still terminates because the existing page cap applies.
- **Negative margins** on the block axis can move a box past the frame origin;
  the mapping is a pure rectangle transform and does not clip.
- **Percent margins in a vertical box** resolve against the containing block
  INLINE size (physical height). A test must cover it; a mistaken physical
  width basis passes horizontal fixtures.
- **Empty declared-height box in a vertical root**: declared 450pt block size,
  144pt page content width (physical) gives 4 pages, as in the horizontal
  Addendum AC1, with the pages advancing along `x`.

## Open questions deferred

- Footnote band edge in vertical roots (Behavior 27): physical bottom,
  implemented by CORE-553. Following the block-end edge stays deferred.
- A `horizontal-tb` abspos box whose containing block is a positioned
  ancestor inside a vertical body (Behavior 28, mixed frame).
- Vertical multicol, flex and grid (Behavior 24).
- Margin-box vertical content, owned by CORE-184 after CORE-555.

## References

- Research recovered and folded in (deleted by `fe99ae5`, readable with
  `git show fe99ae5^:<path>`):
  `docs/research/wpt-harness/core181-root-vertical-progression.md`,
  `core182-interior-writing-mode-scoping.md`, `core177-test-dispositions.md`,
  `core224-flip-triage.md`.
- `docs/lessons/engine-layout.md` (frame swap, orthogonal subtree, anchoring,
  fill rule entries).
- `fragmentation-core.spec.md` (Behavior 11, Addendum Explicit-Height Block
  Continuation), `out-of-flow-positioning.spec.md` (Behavior 9, CORE-204),
  `named-pages.spec.md` (orthogonal writing), `paged-media-css.spec.md`,
  `min-max-sizing.spec.md`, `inline-base-direction.spec.md`.
- CSS: css-writing-modes-3 §3, §7; css-writing-modes-4; css-page-3 §3;
  css-break-3 §5.3; css-sizing-3 §5; css-box-3 §6.
- Issues: CORE-203 (epic), CORE-181, CORE-182, CORE-184, CORE-204, CORE-551
  to CORE-556.
