---
title: Out-of-Flow Positioning
slug: /specifications/out-of-flow-positioning
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-08-18
sidebar_position: 9
tags: [engine, layout, css-position, css-break, fragmentation]
spec_id: out-of-flow-positioning
issue_id: CORE-64
applies_to: engine 0.x
dependencies: [fragmentation-core, wpt-conformance-harness]
---

# Out-of-Flow Positioning

## Overview

Absolutely-positioned (abspos) elements bubble up to their containing block
and are taken out of flow. Under fragmentation their fragments become
children of the **fragmentainer**, not of the CSS containing block — Chromium's
tree-mapping lesson calls this the messiest part of the mapping (the issue's
own framing).

The engine has no out-of-flow support today: every box is laid out as a block
or inline child of its parent. This issue adds `position`/`inset`/`z-index`
with the css-break-3 rule that an abspos box's fragments attach to the
fragmentainer in which its containing block lands.

**Path chosen: stylo.** Verified 2026-08-18 against stylo 0.20.0
`properties/longhands.toml`: `position` (type `PositionProperty`),
`top`/`left`/`right`/`bottom` (type `Inset`), and `z-index` (type `ZIndex`)
are first-class longhands, NOT gecko-gated and NOT pref-gated — the servo
build compiles them (`values/generics/box.rs` defines `PositionProperty`).
Cascade, specificity, and `!important` come from stylo; no manual author-CSS
pass needed.

**Fitness function:** css-break / css-position WPT print-reftests via the
harness, plus unit tests in `engine/tests/position.rs`.

## Goals / Non-Goals

**Goals**

- `position: absolute`: the box is out of flow; its containing block is the
  nearest ancestor with `position != static` (the padding box), else the
  initial containing block (the page's content box).
- `inset` offsets: `top`/`right`/`bottom`/`left` resolve against the
  containing block's padding box; `auto` offsets fall back to the static
  position (basic).
- **Fragmentation:** an abspos box's fragment attaches to the fragmentainer
  in which its containing block lands (css-break-3); it is a child of the
  fragmentainer fragment, not of the CSS containing block.
- `position: relative` as the containing-block anchor (offset applied to the
  in-flow box without affecting siblings).
- `position: fixed` in paged media: resolved against the page box (per
  css-position-3, fixed ≈ absolute to the page in print).
- Basic `z-index` painting order among positioned siblings.
- Determinism: offsets resolved as pure arithmetic at layout time.

**Non-Goals** (deferred; scope stays honest)

- `position: sticky` — computed by stylo but treated as `relative` by layout
  until sticky semantics land (non-goal).
- Stacking contexts / z-index beyond sibling ordering (no compositing,
  opacity-created stacking contexts).
- Abspos inside floats (CORE-62), multicol (CORE-63), or flex (CORE-65) — the
  interaction specs land after the individual features.
- Transforms as containing-block creators; abspos in the `@page` margin-box
  context.

## Behavior

The engine shall:

1. Compute `position` (`PositionProperty`: `Static | Relative | Absolute |
   Fixed | Sticky`), the `inset` offsets (`Inset`: length or `auto`), and
   `z-index` (`ZIndex`: `Auto | Integer`) at cascade time from stylo:
   `ComputedStyle` gains the fields, read in `css.rs::convert`.
2. Resolve the containing block for an abspos box: nearest ancestor with
   `position != static` (relative, absolute, fixed, sticky — sticky counts),
   taking its **padding box**; with no such ancestor, the initial containing
   block (the page content box).
3. Resolve offsets: each non-`auto` inset positions the box's margin edge
   against the containing block's padding edge (per css-position-3); `auto`
   insets use the static position — where the box would have been in flow
   (basic static-position computation: the offset within the containing
   block's content).
4. **Attach abspos fragments to the fragmentainer**: the abspos box's
   fragment becomes a child of the fragmentainer in which its containing
   block lands — NOT a child of the containing block's fragment tree branch
   (css-break-3; the Chromium tree-mapping rule).
5. Place the abspos box on the page containing its containing block: if the
   containing block spans pages, the box lands on the first page containing
   the anchor point resolved at layout.
6. Treat `position: fixed` as absolute against the initial containing block
   (the page box) in paged media.
7. Apply `position: relative` offsets to the in-flow box without changing
   sibling layout; a relative box is a valid containing block for descendants.
8. Paint positioned siblings in `z-index` order (higher first); `auto` paints
   in tree order. Painting happens after in-flow content.
9. Keep the monolithic rule for abspos content taller than the fragmentainer
   (overflow, never slice a line).
10. Stay deterministic: containing-block resolution and offsets are pure
    arithmetic in document order.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle`:

  ```rust
  /// CSS `position` (stylo computed; values::generics::box::PositionProperty).
  pub position: PositionProperty, // Static | Relative | Absolute | Fixed | Sticky
  /// CSS insets, resolved to absolute pt or Auto (stylo `Inset`).
  pub inset_top: Inset,
  pub inset_right: Inset,
  pub inset_bottom: Inset,
  pub inset_left: Inset,
  /// CSS `z-index` (stylo `ZIndex`).
  pub z_index: ZIndex, // Auto | Integer(i32)
  ```

- In `convert`, read `clone_position()`, the four `clone_top()/right()/
  bottom()/left()` insets, and `clone_z_index()` from stylo's box struct.
  `ComputedStyle::initial()`: `position: PositionProperty::Static`,
  insets `Auto`, `z_index: ZIndex::Auto`.

### `engine/src/layout.rs`

- During block layout, hoist a box with `position: Absolute | Fixed` out of
  flow into a **per-fragmentainer abspos list** with its resolved
  containing-block-relative offset (the containing block is resolved from the
  ancestor chain; the initial containing block is the page content box).
- The abspos fragment attaches to the fragmentainer fragment — it is laid out
  as a child of the fragmentainer, not of the containing block's subtree
  (behavior 4).
- `position: Relative` offsets applied at fragment finalization (no sibling
  impact).

### `engine/src/frag.rs`

- `Fragmentainer` gains `abspos: Vec<AbsposFragment>` (resolved `Rect` +
  fragment + z-order). Distinct from the floats parallel-flow list: abspos
  fragments do not suspend/resume across pages — they are placed once on the
  page of their containing block.

### `engine/src/pdf.rs`

- Paint order per fragmentainer: in-flow content, then abspos fragments
  sorted by `z_index` (document order for ties/`auto`).

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/position.rs` (helpers mirror
`tests/typography.rs`).

1. **Offsets.** Given a `position: relative` parent with
   `position: absolute; top: 10pt; left: 10pt` child, the child's top-left
   corner lands 10pt inside the parent's padding box.
2. **Initial containing block.** Given `position: absolute` with no
   positioned ancestor, the box resolves against the page content box.
3. **Fragmentainer attachment.** Given an abspos box whose containing block
   sits on page 2 of a 3-page document, the box's fragment is a child of page
   2's fragmentainer and renders at the containing-block-relative offset
   (never page 1 or 3). Assert on the fragment tree, not pixels.
4. **Fixed in paged media.** Given `position: fixed; top: 0`, the box lands
   at the top of the page box.
5. **Relative anchor.** Given a `position: relative` box with `top: 5pt`, the
   box shifts 5pt and siblings keep their original positions.
6. **z-index.** Given two positioned siblings with `z-index: 1` and
   `z-index: 2` overlapping, the `2` paints on top (assert paint order).
7. **Sticky-as-relative.** Given `position: sticky`, the box lays out as
   relative (no crash, offsets applied).
8. **Determinism.** Two renders are byte-identical; the full existing engine
   test suite still passes.

## Edge Cases

- All insets `auto`: box sits at its static position (basic
  static-position computation, not the full css-position static-position
  rules).
- No width/height on the abspos box: shrink-to-fit sizing against the
  containing block (reuse the existing intrinsic sizing path).
- Negative insets: allowed; box may extend beyond the containing block.
- Containing block spans a page boundary: box lands on the first page
  containing the anchor (behavior 5); deterministic.
- Abspos box taller than the fragmentainer: monolithic overflow, never sliced.
- Abspos inside a table cell: containing block resolution follows the same
  ancestor rule (the cell's positioned ancestor, else the page) — basic
  behavior only, deeper table+abspos interactions deferred.

## References

- css-position-3 (abspos, containing blocks, fixed in paged media):
  https://drafts.csswg.org/css-position-3/
- css-break-3 (fragmentation of abspos; fragments become children of the
  fragmentainer): https://drafts.csswg.org/css-break-3/
- Chromium tree-mapping lesson (the issue's framing): see
  `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
- Fragment tree this builds on: `fragmentation-core.spec.md`
- Parent epic: Linear CORE-54.
- stylo 0.20.0 `properties/longhands.toml` (verified 2026-08-18): `position`,
  `top`/`left`/`right`/`bottom`, `z-index` — compiled in the servo build.
