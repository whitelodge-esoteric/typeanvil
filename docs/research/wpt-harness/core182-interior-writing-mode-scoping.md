---
title: CORE-182 Interior Writing Modes — Measured Scoping and What Actually Fails
type: research
status: draft
owner: elijah
created: 2026-09-15
updated: 2026-09-15
sidebar_position: 5
tags: [wpt, conformance, writing-modes, fragmentation, core-182]
---

# CORE-182 Interior Writing Modes — Measured Scoping and What Actually Fails

CORE-182 owns INTERIOR vertical writing-mode layout: a subtree that declares its
own `writing-mode` inside a page whose flow is horizontal, and the mirror case
of an orthogonal (`horizontal-tb`) subtree inside a vertical page. It was split
out of CORE-181, whose eight targets (root-vertical page progression plus
canvas gradient slices) it is supposed to unblock.

This note records the measured state at the release tip `11f862f`, the one
piece that landed, and three findings that change the issue's own plan. It is
written for the session that attempts the transposition.

## Measured state at the branch point (`11f862f`)

Full 283-test suite, fresh container build of the branch point, A/B diffed per
test id against the same run:

| | value |
| -- | -- |
| PASS / FAIL | 159 / 124 |
| `page-size-012-print` | PASS (both sides render horizontal text) |
| `page-name-orthogonal-writing-001/002/003/004` | all PASS |
| `page-margin-002`, `page-margin-003` | both PASS |
| `margin-boxes/dimensions-004/006/013/014` | all FAIL (48113 / 63958 / 80109 / 81393 differing pixels) |
| `css-break/firefox-bug-2026295-print` | FAIL — "mismatch expected but images matched" |
| `body-background-{vlr,vrl,slr,srl}`, `block-001/002-wm-*`, `page-box-008` | all FAIL (CORE-181's ledger) |

Gate reports: `base-core182.json` and `after-core182.json`.

## What landed: the anchoring re-key (zero flips)

The CORE-153 *block-start anchoring* rule was keyed on the ROOT element's mode:
a definite-width box with `margin-right` anchored from the right edge only when
`html` declared `vertical-rl`. It now reads `Ctx::writing_mode_at(id)` — the
nearest ancestor-or-self declaration, else the page flow mode — which is what
css-writing-modes-3 §7 inheritance means. A nested `vertical-rl` div inside a
HORIZONTAL page flow therefore anchors from the right edge too, because its own
block axis runs right-to-left.

Gate: **ZERO status flips**, 159 PASS / 124 FAIL on both sides, diffed per test
id. Rust suites green. Proven by one unit test, shown RED with the re-key
reverted:

* `layout::core153_vertical_rl_tests::core182_interior_vertical_declaration_right_anchors`
  (measured `x=0 want 285` before the fix).

## What was built, measured, and REVERTED: the inline-extent fill

Widening the other CORE-153 rule — the vertical inline-extent fill, where an
auto `height` grows to the fragmentainer — to the box's own mode looked like the
obvious second half of item 1. It was built and it is wrong:

`page-name-orthogonal-writing-003` is `html` horizontal-tb with an interior
`writing-mode: vertical-rl` wrapper holding two short divs. With the fill keyed
on the box, the wrapper grew to **210pt of a 216pt page**, so its second child no
longer fitted and the document rendered **2 pages instead of 1**. Chromium
renders that fixture as one page — which is exactly why CORE-155's
`tests/page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow`
asserts 1. The probe is in this note's history: both the fixture shape AND its
reference shape moved 1 → 2 pages together.

The reason is a modelling gap, not a detail. The rule means "the box fills its
CONTAINING BLOCK's inline size". For the root/body chain under a vertical page
flow, the fragmentainer *is* that containing block. For an interior vertical box
inside a horizontal flow, the containing block's inline extent is content-based
and indefinite, and the engine only models a definite containing-block extent for
DECLARED extents (`specified_extent`, CORE-167). So the fill keeps its page-flow
key, and the NOTE at that rule in `layout_box` records the measurement.

**The gate could not see any of this.** Both sides of the pair transposed
together, so the 283-test A/B reported zero flips while a Chromium-verified page
count regressed. This is the sharpest lesson of the session: for this bucket, the
unit tests in `engine/tests/` are the guard of record, and a "zero flips" gate
result is not evidence that a render did not change.

A first version of the discarded test also PASSED without its fix and had to be
rewritten: a `width ≈ 450 && height < 100` filter over every fragment matches a
**Line** fragment, and a line box is created at the line-box width whatever its
container's height. Any future "did the box stretch?" assertion must walk only
fragments that carry a paint (e.g. `FragmentContent::Background`).

## Finding 1 — the issue's "Tests this would move" table is mostly mis-attributed

Three of the four groups in that table do not need document-interior vertical
layout at all.

**`css-break/firefox-bug-2026295-print` is a print CRASHTEST.** Its only
reference is `/common/blank.html` with `rel="mismatch"`, so it passes iff the
document renders something NON-BLANK. Our render is blank, so it fails as
"mismatch expected but images matched". The fixture's paint comes from
`content: url(data:image/gif;...)` on a `<font>` and an `<h6>`; this engine
ignores `content` images on non-replaced elements (`layout.rs`, "Element
generated content paints no image in this engine"). The blocker is element
`content: url()` support, and the `writing-mode: vertical-rl` in the fixture is
incidental (it is part of the Firefox crash repro, not the assertion).

**`margin-boxes/dimensions-004/006/013/014` need MULTI-LINE MARGIN-BOX
CONTENT, not rotation.** `writing-mode` is not parsed for a margin box at all
today — `MarginBoxSpec` carries no such field — and the four references SIMULATE
vertical text with horizontal `<br>` blocks: `dimensions-013`'s ref paints
`@top-left`'s seven vertical lines as one
`<div style="width:17.5em">xxxxxxx</div>`. What actually differs is the margin
box's INTRINSIC SIZING in its own writing mode, which the fixture comments state
outright ("Min/max width for top-left is 7em (seven lines in an orthogonal
writing mode)"). `dimensions-006` declares no `writing-mode` at all: it tests
min/max content sizing of margin boxes generally. Note also that all four use
the Ahem font, whose glyph is a solid square — a rotated square is the same
square — so rotation alone is invisible to these fixtures by construction.

**`page-size-012-print`, `page-name-orthogonal-writing-001..004` and
`page-margin-002/003` all PASS today.** None of them can move up.

## Finding 2 — the gate is self-consistent, so this capability has no standalone payoff

The harness compares OUR test render against OUR reference render through the
SAME engine (mode (a), "strict WPT semantics", `harness/engine.py`). Any change
that applies uniformly to BOTH sides of a pair is therefore INVISIBLE to the
gate.

That is the whole interior-writing-mode story:

* `page-size-012`: the test declares `:root { writing-mode: vertical-rl }`, and
  its reference declares `writing-mode: vertical-rl` on the inner divs. Keyed on
  the mode IN EFFECT AT THE BOX (not on "interior declarations only"), BOTH
  sides transpose identically, so the pair survives — it stays PASS.
* `page-margin-002/003`: root vertical on both sides. Both transpose. Stays
  PASS.
* So does rotating glyph runs: it rotates the same text the same way on either
  side.

The consequence for planning is blunt: **implementing the interior transposition
on its own moves the gate by approximately zero**, and carries the risk of
flipping the currently-PASSING orthogonal pairs. Its value is entirely in being
the enabling half of CORE-181's root-vertical PROGRESSION — the eight
`body-background-*` / `block-00{1,2}-wm-*` / `page-box-008` targets — which is
where a root-vertical document has to advance its pages along the physical x
axis. CORE-182 cannot be closed by a gate-visible slice; it can only be closed
together with (or immediately before) CORE-181's progression work.

This also narrows CORE-181's blocker 2. That blocker said the
`page-size-012` pairing could only be repaired by laying interior vertical
subtrees out vertically. With the geometry re-key landed, the geometry half is
handled; what remains for the pairing is the glyph-run and line-box
transposition, and the "orthogonal subtree must stay physical" hazard is now
enforced by the geometry rule itself rather than left to the frame swap.

## Finding 3 — the transposition's shape (reasoned, not measured)

The design from `core181-root-vertical-progression.md` still applies, and there
is a natural local version of it that reuses all of the existing horizontal
machinery:

1. Give the vertical-mode box a VIRTUAL content frame:
   `virtual width = the box's physical inner HEIGHT`,
   `virtual height = the box's physical inner WIDTH`.
   The existing block path then breaks lines against the physical height and
   stops at the physical width with no new breaking logic.
2. Lay the content out in that frame unchanged — lines stack down, runs advance
   right.
3. Transpose the assembled subtree into the physical frame. For `vertical-rl`:
   a virtual child at `(vx, vy)` with size `(w, h)` becomes
   `X = content_right - vy - h`, `Y = content_top + vx`, `W = h`, `H = w`;
   a run's baseline starts at `(content_right - baseline_y, content_top + run_x)`
   and advances along physical +y. `vertical-lr` mirrors the block axis;
   `sideways-lr` reverses the inline axis too, so its glyphs rotate 90°
   counter-clockwise.
4. Rotate the run at emit time: `pdf.rs` already pushes a per-object transform
   (`surface.push_transform(&krilla::geom::Transform::from_row(..))`) for page
   orientation and for images, so a rotated glyph run is a push/rotate/draw/pop,
   not a new drawing path.

Two things this does NOT cover, and they are why the work is not small: declared
`width`/`height` map to the OPPOSITE axis inside the virtual frame (so the
style's physical extents must be swapped for the box), and fragmentation along
the interior block axis (an interior vertical subtree crossing a page edge) is a
new slicing direction that has to compose with CORE-152 / CORE-167 continuation.

## Recommended order

1. **Landed**: the anchoring re-key (this session), zero flips, unit-tested, with
   the fill generalisation recorded as a measured negative.
2. Build the interior transposition as the first slice of CORE-181's progression
   work, not as a standalone CORE-182 landing — with `page-margin-002/003` and
   `page-size-012` as the canary trio (all three must stay green, and all three
   exercise the "both sides transpose" property). Re-run the `engine/tests/`
   boundary tests as well: they are the only guard that catches a pagination
   change this bucket's self-consistent reftests hide.
3. Give the engine a definite containing-block INLINE size before re-attempting
   the fill generalisation; without it the rule cannot tell a page-flow-vertical
   containing block from an interior vertical box, and the
   `page-name-orthogonal-writing-003` page count is the regression.
4. Treat the two mis-attributed families as their own issues: element
   `content: url()` images (`firefox-bug-2026295`), and multi-line margin-box
   content plus writing-mode-aware margin-box intrinsic sizing
   (`dimensions-004/006/013/014`).
