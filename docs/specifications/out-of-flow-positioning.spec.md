---
title: Out-of-Flow Positioning
slug: /specifications/out-of-flow-positioning
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-09-15
sidebar_position: 9
tags: [engine, layout, css-position, css-break, fragmentation]
spec_id: out-of-flow-positioning
issue_id: CORE-64
superseded_by_note: Behaviors 5/9 refined by CORE-169 (2026-09-13); Behavior 9 exception by CORE-185 (2026-09-15)
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
   insets use the static position — the containing block's padding-box origin
   (top-left corner; the basic static position, not the full css-position
   static-position rules).
4. **Attach abspos fragments to the fragmentainer**: the abspos box's
   fragment becomes a child of the fragmentainer in which its containing
   block lands — NOT a child of the containing block's fragment tree branch
   (css-break-3; the Chromium tree-mapping rule). Concretely: the fragment is
   appended to `fragmentainer.root.children` (the page root's children),
   already in PAGE-ABSOLUTE coordinates (the out-of-flow branch lays against
   `content.x`/`content.y` directly — CORE-127 slice b; the earlier
   content-origin subtraction double-shifted paint up-left by the margin,
   hidden because test and ref shifted identically), so the in-flow
   subtree never sees it.
5. Place the abspos box on the page containing its containing block: if the
   containing block spans pages, the box lands on the first page containing
   the anchor point resolved at layout.
6. Treat `position: fixed` in paged media as absolute against the initial
   containing block (page 0's content box — the anchor page), laid out ONCE
   after pagination, with the fragment CLONED onto every page at the page's
   own content-origin offset (css-position-3 §fixed in paged media: fixed
   content repeats on all pages; CORE-127 slice b). A named page resolving a
   different `@page` size/margin shifts the clone by the content-origin
   delta; the box keeps its anchor-page geometry. Nested fixed boxes are
   consumed by their outer box (one clone covers the subtree).
7. Apply `position: relative` offsets to the in-flow box without changing
   sibling layout; a relative box is a valid containing block for descendants.
8. Paint positioned siblings in `z-index` order (higher first); `auto` paints
   in tree order. Painting happens after in-flow content.
9. Keep the monolithic rule for PINNED abspos content — a box anchored by
   any inset, or whose containing block is a positioned ancestor — taller
   than the fragmentainer: the box is PLACED ONCE (like last-resort lines)
   and may overflow the page bottom; it never slices and never resumes
   across pages — the abspos item needs no break token. REFINED (CORE-169):
   a PAGE-ANCHORED abspos box (auto insets, initial containing block) at
   its static position with a declared extent fragments across
   fragmentainers like an in-flow box (css-break-3 §2.3 class A): when it
   does not fit the remaining space, it defers whole to the next page
   (one deferral, then force-place — the CORE-109 guard), and its fragments
   attach page-locally, each page's own `@page` context applying.
   EXCEPTION (CORE-185): a PINNED box whose used inset lands AT OR PAST the
   fragmentainer bottom must not vanish — the offset resolves against the
   page AREA, so `top: 500px` at a 216pt page height belongs to page 2 and
   the document GROWS to include that page (Chromium fragmented printing).
   It is enqueued as an abspos drain job carrying the page-ABSOLUTE (x, y);
   the drain places it on the page CONTAINING the offset against the REAL
   bottom limit (a tall box fragments across pages, css-break-3 class A
   once started), and the body's in-flow siblings after it still place on
   the current page (the box is out of flow; no loop break).
   The drain MUST bound its own pagination: a MONOLITHIC layout path that
   ignores break tokens (a replaced element — `layout_image` returns an
   empty fragment + `break_before` whenever the box does not fit) would
   otherwise regenerate its token forever. Such a box is retried once as a
   LAST-RESORT placement (the empty-page rule `layout_image` itself uses)
   and dropped if it still refuses, so the page loop always terminates.
10. Stay deterministic: containing-block resolution and offsets are pure
    arithmetic in document order.

## Interfaces

### `engine/src/css.rs`

- Add engine-owned types (the seam: `ComputedStyle` never carries stylo types —
  mirror the CORE-62 `Float` mapping):

  ```rust
  /// CSS `position` (mapped from stylo `PositionProperty`).
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
  pub enum Position { #[default] Static, Relative, Absolute, Fixed }
  ```
  Sticky is computed by stylo but maps to `Relative` (per the Non-Goals:
  sticky treated as relative until sticky semantics land).

- Add to `ComputedStyle`:

  ```rust
  pub position: Position,
  /// Non-`auto` insets, resolved to absolute pt. Points.
  pub inset_top: Option<Scalar>,
  pub inset_right: Option<Scalar>,
  pub inset_bottom: Option<Scalar>,
  pub inset_left: Option<Scalar>,
  /// `z-index` (stylo `ZIndex`), `None` = `auto` (tree order).
  pub z_index: Option<i32>,
  ```

- In `convert`, read `box_.clone_position()` (box struct) → `Position`;
  `position.clone_top()/right()/bottom()/left()` (position struct — the same
  struct that carries `width`; insets are `LengthPercentage`, `auto` →
  `to_length()` = `None`) → `Option<Scalar>`; `box_.clone_z_index()` →
  `Option<i32>`. `ComputedStyle::initial()`: `Static`, insets `None`,
  `z_index: None`. (Verify accessor struct placement in the generated
  `properties.rs` before relying on it — see CORE-62's `clone_width` lesson.)

### `engine/src/layout.rs`

- `Flow` gains:
  - `abspos_cb: Option<(Point, Scalar)>` — the nearest positioned ancestor's
    padding-box origin + content width on the current page. `layout_box` sets
    it (save/restore around the box's subtree) when the box's `position` is
    `Relative | Absolute | Fixed`; the initial containing block (page content
    box) is the fallback.
  - `abspos: Vec<Fragment>` — abspos fragments with page-absolute offsets,
    drained into the fragmentainer root after each page.
  - `abspos_jobs: Vec<AbsposJob>` (CORE-169) — page-anchored abspos boxes
    queued to continue on the NEXT fragmentainer; snapshotted at page start
    (`drained_jobs`), drained after the body layout of the page that
    follows the deferral. Each job lays ONE fragment through the
    block-family dispatch (`layout_table_like`) with a real `bottom_limit`
    (so CORE-167's declared-height fragmentation slices it), pushing
    fragments DIRECTLY into the current fragmentainer root.
  - `abspos_resume_tokens: BTreeMap<NodeId, BreakToken>` (CORE-169) — the
    continuation token of a box whose drain fragment broke again.
  - `abspos_finished: Vec<NodeId>` (CORE-169) — boxes the drain placed to
    completion; the body item loop drops their stale pending tokens.
  - Body item-loop rule (CORE-169): a child carrying a `deferred_once`
    child token is drain-owned — the body never re-renders it; it carries
    the token forward (with `broke = true`, keeping the page loop alive)
    while a job or resume token exists for it, and drops the token once
    the drain finished the box.
- In the block item loop, a child with `position: Absolute | Fixed` takes the
  **out-of-flow branch** (parallel to the CORE-62 float branch): resolve the
  containing block (nearest positioned ancestor, else the page content box;
  `Fixed` always the page content box), resolve x/y from the insets (left →
  `cb.x + left`; else right → `cb.x + cb_w - w - right`; else static x = cb.x;
  same for y with top/bottom), measure (width auto → shrink-to-fit, height via
  `measure_block`), lay the box monolithically with the page bottom, push the
  fragment to `flow.abspos`, and do NOT advance the in-flow cursor. `continue`
  with an explicit `i += 1` (the CORE-62 lesson).
- `measure_block` skips `position: Absolute | Fixed` children (they add no
  in-flow height) — same rule as floats.
- After `layout_root` returns for a page, drain `flow.abspos` (sorted stable by
  `z_index`, tree order for ties/`None`) into `fragmentainer.root.children`.
  The offsets are already page-absolute (CORE-127 slice b) — no origin
  adjustment is applied at drain time; the emitter's root walk treats every
  root child as page-absolute.

### `engine/src/frag.rs`

- `Fragmentainer` gains `content_origin: Point` — the page-absolute origin of
  the page's content box (CORE-127 slice b), set by `paginate` for every page
  it lays out. The fixed-position attachment pass uses it to shift each
  fixed clone by the page's content-origin delta (named pages may resolve a
  different `@page` size/margin per page). Abspos fragments ride the
  existing `Fragmentainer.root` child list (Behavior 4); `Flow::abspos` is
  layout-internal.

### `engine/src/pdf.rs`

- No change: paint order = tree order, and the abspos fragments are appended
  after the in-flow subtree, sorted by `z_index`.

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
9. **Page-anchored abspos fragmentation (CORE-169).** Given a page-anchored
   `position: absolute` container with declared-height children spanning
   more than the remaining fragmentainer space, the box defers to a fresh
   page and its children slice at fragmentainer edges; each page's own
   `:left`/`:right` `@page` context applies per fragment
   (page-margin-007).

## Edge Cases

- All insets `auto`: box sits at its static position (basic
  static-position computation, not the full css-position static-position
  rules).
- No width/height on the abspos box: shrink-to-fit sizing against the
  containing block (reuse the existing intrinsic sizing path).
- Negative insets: allowed; box may extend beyond the containing block.
- Containing block spans a page boundary: box lands on the first page
  containing the anchor (behavior 5); deterministic.
- Abspos box taller than the fragmentainer: pinned boxes (insets or a
  positioned containing block) overflow monolithically, never sliced;
  page-anchored boxes (CORE-169) fragment at fragmentainer edges.
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
