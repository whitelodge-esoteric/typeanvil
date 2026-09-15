---
title: CORE-181 Root-Vertical Page Progression — Findings and Blocker Analysis
type: research
status: draft
owner: elijah
created: 2026-09-14
updated: 2026-09-14
sidebar_position: 4
tags: [wpt, conformance, writing-modes, fragmentation, core-181]
---

# CORE-181 Root-Vertical Page Progression — Findings and Blocker Analysis

CORE-181 owns root-level vertical writing-mode page progression plus
canvas-background-image (gradient) support: eight WPT print-reftests that
cannot move without it. Two prior attempts (2026-09-11, 2026-09-12) built the
axis-swap design below and were rejected as net-negative. This note records
the measured state at the release tip `76baa3c`, the design derived from the
fixtures, and the three blockers a correct implementation has to clear. It is
written for the session that attempts the progression again.

## Measured state at the release tip (`76baa3c`)

Full 283-test suite, fresh container build of the branch point:

| | value |
| -- | -- |
| PASS / FAIL | 159 / 124 |
| the eight CORE-181 targets | all FAIL |
| `sideways-lr` / `sideways-rl` | parsed as horizontal (both parsers) |

The eight targets, and what each one actually needs:

| Test | Current failure | Needs |
| -- | -- | -- |
| `body-background-{vlr,vrl,slr,srl}-print` | page count test=1 ref=2 | horizontal block progression + canvas gradient slices |
| `page-box-008-print` | page count test=2 ref=1 | vertical-flow geometry (over-paginates today) |
| `block-001-wm-{vlr,vrl}-print` | pixel diff | horizontal block progression + transposed border paint |
| `block-002-wm-{vlr,vrl}-print` | page count test=10 ref=15 | same, through eight pages |

Encouraging finding: the `css-break` fixtures (`block-001/002-wm`) and three of
the four `body-background` fixtures contain **no glyphs at all** — only boxes,
borders and background colours. Root-vertical progression for those families is
a geometry-and-paint problem, not a text-shaping problem.

## The design (unchanged from the rejected attempts)

**Virtual-strip layout plus paint-time mapping.** The engine paginates every
flow along the physical y axis. For a document whose page flow is vertical
(css-page-3 §3: the ROOT element's `writing-mode`), relabel the axes instead of
rewriting the block path: lay the body out in a *virtual* frame where the block
axis is vertical again, then map the virtual frame back to the physical page.

The virtual content rect is the physical content rect with the axes swapped:

```
virtual = (x = physical.y, y = physical.x,
           w = physical.height, h = physical.width)
```

The body lays out in `virtual` (so `Ctx::content_width := physical.height` and
`Ctx::page_height := physical.width`), which makes the existing vertical
fragmentation machinery slice the strip along the physical x axis. Page *N*
then owns the virtual block range `[N·H, (N+1)·H)` where `H` is the virtual
page height. Mapping one collected paint rect `(x, y, w, h)` back:

```
block_rtl  (vertical-rl, sideways-rl): X = cx + cw - (y - vy0) - h
block_ltr  (vertical-lr, sideways-lr): X = cx + (y - vy0)
inline_btt (sideways-lr only):         Y = cy + ch - (x - vx0) - w
otherwise:                             Y = cy + (x - vx0)
W = h ; H = w
```

with `(cx, cy, cw, ch)` the physical content rect and `(vx0, vy0)` the virtual
frame origin. Text runs reuse the same trick the emitter already needs: map the
run's ABSOLUTE baseline, then re-relativise it against the mapped parent origin.
Do not rotate glyphs — see blocker 2.

## Measured: the frame swap alone does NOT move the targets

A hand session ran the frame swap on its own (2026-09-14, branch point
`11f862f`): the body lays out in the virtual frame, while the physical rect
stays what the canvas fill, the chrome rings, margin boxes, footnotes and
out-of-flow content resolve against. Page counts measured directly per fixture,
test vs reference:

| Fixture | before (test/ref) | after (test/ref) |
| -- | -- | -- |
| `body-background-{vlr,vrl,slr,srl}` | 1 / 2 | 1 / 2 (**unmoved**) |
| `page-box-008` | 2 / 1 | 5 / 1 (worse) |
| `block-001-wm-vlr` | 12 / 12 | 6 / 6 |
| `block-002-wm-vlr` | 10 / 15 | 5 / 8 |
| `page-margin-002` (canary) | 3 / 3 | 3 / 3 |
| `page-margin-003` (canary) | 3 / 3 | 3 / 3 |
| `page-size-012` (canary) | 2 / 2 | 2 / 2 |
| `transform-022` (canary) | 5 / 5 | 5 / 5 |

Two conclusions.

**The canary pair holds.** `page-margin-002/003` keep their counts, so the frame
arithmetic (page rect swap, physical chrome, physical footnotes, physical abspos
drain) is sound.

**The frame swap is not sufficient. Box dimensions have to be transposed too.**
The virtual frame relabels the PAGE, but every box still carries its physical
`width` and `height`, and the horizontal machinery reads `width` as the inline
extent and `height` as the block extent. In a vertical writing mode the mapping
is the opposite. So `div { width: 600px; height: 100px }` takes an inline extent
of 600 inside a 600-wide virtual frame and a block extent of 100: it cannot
cross a page edge, and the count stays at 1. The `block-001/002-wm` counts halve
instead, because those `block-size: 210vw` boxes now derive their block extent
from the wrong axis.

A complete implementation needs a **style transpose** for every element in a
root-vertical document, in addition to the frame swap:

- `width` ↔ `height`, plus the min/max pair and the resolved `vw`/`vh` values;
- `margin-left` ↔ `margin-top` and `margin-right` ↔ `margin-bottom`, with the
  same swap for borders and padding. The direction variant (`*-rl` vs `*-lr`)
  decides whether the block-axis pair is mirrored;
- the CORE-153 rules (auto inline extent fills the page content box,
  `vertical-rl` block-start right-anchoring) keyed on the mode in effect at each
  box rather than on the root's mode.

This is the style-swap transposition that changes layout for EVERY root-vertical
document, and it is the largest single piece of the work. Budget for it before
starting the frame swap.

## Blocker 1 — out-of-flow and chrome content must stay PHYSICAL, and that is not a local change

The virtual frame is a frame for the **body** only. These always belong to the
page, not to the flow:

- the `@page` background, canvas fill, outline and border rings
  (`Fragmentainer::content_origin` / `content_size` — currently assigned from
  the same `content` local that the body layout uses, so the assignment has to
  move to the physical rect);
- margin boxes (they resolve against `spec.geometry()`);
- footnotes (the reserved band is measured from the physical page bottom);
- **`position: absolute` / `fixed` boxes**.

The out-of-flow case is the expensive one. An absolutely positioned box inside
a vertical root is a box whose own writing mode is often `horizontal-tb`
(orthogonal to the page flow) — the `body-background-*` fixtures each carry one
`<p style="writing-mode:horizontal-tb; position:absolute">`. Such a subtree must
be laid out in the PHYSICAL frame *and* exempted from the paint map, because
relabelling its axes turns its horizontal text into vertical text. Exempting it
is not a lookup: the out-of-flow box is laid out inside the parent's child-item
loop, its containing block comes from `Ctx` (`flow.abspos_cb`), and the
page-anchored path, the deferred `AbsposJob` drain and the pinned path each
resolve against the same `Ctx` rect. A correct implementation needs a second
(physical) `Ctx` for those subtrees, plus a per-fragment marker so the emitter
skips the map. Both prior attempts carried that marker and still lost more than
they won.

## Blocker 2 — the references are not all symmetric, so a ROOT-only trigger breaks a passing test

Three currently-PASSING tests declare a root-vertical mode: `page-margin-002`,
`page-margin-003` and `page-size-012`. Their references also declare root
vertical, so a root-only trigger transposes both sides identically and the
pairing survives — that is how `page-margin-002/003` should stay green.

`page-size-012` is different. Its reference has a **horizontal** root and
simulates the vertical layout with inner `writing-mode: vertical-rl` divs. In
Chromium both sides render rotated text; in this engine BOTH sides render
horizontal text today, which is why the test passes. A root-only transposition
rotates the test side and leaves the reference side horizontal, so the pairing
breaks. It can only be repaired by also laying out interior vertical subtrees
vertically — the full writing-mode transposition, not the root-level slice.
This is the hazard the issue text does not name, and it is worth checking with
the user before accepting `page-size-012` as a classified flip.

A second consequence of the same asymmetry: interior `html` / `body` boxes that
declare a vertical mode (including `@page { writing-mode: vertical-rl }`, as in
`page-box-009`) must not silently inherit the new frame.

## Blocker 3 — the canvas gradient strip model does not reproduce every reference

`background-image: linear-gradient(...)` on `html`/`body` is not parsed for the
canvas at all today; only a gradient whose stops are all one colour collapses
to a solid (`css::solid_gradient_color`). The canvas propagation path
(`css::resolve_canvas_background` → `Fragmentainer::canvas_background` → the
`pdf.rs` page fill) carries a solid `Color` only.

The four references imply the canvas is the page-progression STRIP, with the
gradient's 0% end at the start of the first page and 100% at the end of the
last, **in document order** — the only model that yields page 1 lightgray /
page 2 white for `vlr` and `vrl` alike, which declare the same `90deg` gradient
while progressing in opposite directions. Two references use solid `<div>`s
instead of a gradient, but `body-background-slr-ref` and `-srl-ref` set a body
gradient, and `srl-ref`'s is `linear-gradient(to bottom, white 0%, white 50%,
lightgray 50%, lightgray 100%)` — white FIRST, while its own assertion says the
first page is lightgray. Render that reference and read the two pages before
building the slice: either the strip's origin is the root element's box rather
than the page area, or that reference does not survive its own assertion. Do
not settle this from the assertion text alone.

Because the harness compares OUR test render against OUR reference render, the
reference's actual behaviour in this engine defines the target. A gradient
slice whose reference paints the inverted page cannot pass, and the fix may
belong on the reference-compatibility side rather than the gradient side.

## What landed in this session

`sideways-rl` / `sideways-lr` now parse as vertical page-context modes in both
parsers (`PageWritingMode::SidewaysRl` / `SidewaysLr` plus an `is_vertical()`
helper), with the `@page` logical margin/padding mapping and block-start
anchoring following the block axis. Landing flipped ZERO WPT statuses — the two
fixtures that depend on it still fail on progression — so the seam is proven by
two unit tests, both shown RED with the mapping reverted:
`paged::tests::logical_margins_map_sideways_modes` and
`layout::core153_vertical_rl_tests::sideways_modes_are_vertical_page_flows`.

## Suggested order for the next attempt

1. Canvas gradient parsing plus per-page slices (blocker 3), gated alone. It is
   independent of the frame work and moves the pixel half of the four
   `body-background` fixtures.
2. The physical-frame path for out-of-flow subtrees (blocker 1), gated alone.
   Nothing to show for it on its own, but the frame work is unsafe without it.
3. The style transpose and the frame swap together, with `page-margin-002/003`
   as the canary pair. Measured (see above): the frame swap ALONE leaves the
   four `body-background` page counts unmoved, because box dimensions are still
   physical. The two belong in one change.
4. Then decide with the user how to treat `block-001/002-wm` ×4 (accidental
   passes, CORE-140 precedent) and `page-size-012` (blocker 2).
