---
title: CSS Floats
slug: /specifications/css-floats
type: spec
status: draft
owner: elijah
created: 2026-08-19
updated: 2026-08-19
sidebar_position: 7
tags: [engine, css, floats, fragmentation, layout]
spec_id: css-floats
issue_id: CORE-62
applies_to: engine 0.x
dependencies: [fragmentation-core, typography-layer, tables-fragmentation]
---

# CSS Floats

## Overview

The engine currently ignores `float`: a floated element is laid out as an
ordinary in-flow block (stylo blockifies `float` in the computed `display`,
so the element at least renders — but nothing wraps around it and it advances
the in-flow cursor like any block). This issue makes floats real: the floated
box is removed from the in-flow cursor, placed at the current line position
flush to the content edge, and following in-flow text wraps around it. A float
taller than the remaining fragmentainer suspends (css-break-3 parallel flow)
and resumes at the top of the next page while in-flow siblings continue.

**Path chosen: stylo.** Verified 2026-08-19 that stylo's servo build compiles
`float` (generated `properties.rs` has `clone_float()` and
`pub float: longhands::float::computed_value::T` on the box struct). The
cascade, inheritance, and blockification (`float: left` on an inline element
computes `display: block`) come for free. `width` (needed for explicit float
widths) is a basic box property and expected to compile too — verify with the
same generated-file check before adding a manual parse pass; if it compiles,
read it from stylo like `float`.

## Goals / Non-Goals

**Goals**

- `float: left | right` places the box out of flow at the current line
  position, flush to the content-box edge (minus the float's own margins).
- Float width: explicit `width` when set; otherwise shrink-to-fit (widest
  line of the float's content, capped at the containing block's inner width).
- Following in-flow text wraps around the float: lines whose y-band overlaps
  the float's rectangle use a reduced available width (left float indents the
  line start; right float shortens the line end; both shrink from both sides).
  Lines below the float's bottom return to full width.
- Floats stack simply: each new left float places at the left edge; each new
  right float at the right edge. (Simplified CSS2 §9.5.1 — see Non-Goals.)
- Fragmentation: a float taller than the remaining fragmentainer suspends at
  the page boundary and resumes at the top of the next fragmentainer; in-flow
  siblings continue beside it. Text on the next page wraps around the float's
  continuation. This is the issue's Done criterion (css-break-3 parallel
  flow).
- Determinism: identical input → byte-identical PDF. Float placement is pure
  arithmetic over measured content; no randomness, no HashMap iteration order.
- Default unchanged: no `float` declaration → byte-for-byte current behavior;
  existing engine tests pass unmodified.

**Non-Goals** (deferred; scope stays honest)

- The `clear` property (floats will not be cleared; a `clear` declaration is
  ignored this pass).
- Full CSS2 float stacking (side-by-side placement with line-wrap when the
  float line fills; float continuation below a tall float on the same line
  area). One float per side per y-band, placed at the edge.
- BFC / margin-collapse semantics around floats. The float's own margins
  offset it from the edge; no collapsing across the float boundary.
- Floats inside multicol/flex (future CORE-63/CORE-65 work); floats nested
  inside a float are naturally handled by recursion and stay in scope.
- Cross-band global Knuth-Plass optimization: text beside a float is broken
  per segment (see Behavior §6), not as one whole-paragraph total-fit run.
- `float: inline-start / inline-end`: whatever stylo computes them to, the
  engine only acts on computed `Left` / `Right` / `None`.

## Behavior

The engine shall:

1. Compute `float` per element from stylo's computed value in the cascade
   (`None | Left | Right`). Elements with `float: none` (the default) are laid
   out exactly as today.
2. Blockify: rely on stylo's computed `display` (a floated inline computes to
   block) — the engine must NOT add its own blockification.
3. Place a floated block out of the in-flow cursor: the in-flow y does NOT
   advance past the float; the next in-flow item starts at the same y (the
   line position where the float began). The float's own fragment is emitted
   as a child of the current fragmentainer at its placed position.
4. Resolve the float's used width: explicit `width` (from `ComputedStyle`)
   when set (clamped to the containing block's inner width); otherwise
   shrink-to-fit = the widest line of the float's content broken at the inner
   width, capped at the inner width (mirror `table.rs::measure_text_width`).
   Resolve the float's height by measuring its content like `measure_block`
   (same breaker, so measured height equals laid-out height).
5. Place a left float at `content_left + margin_left` and a right float at
   `content_right - margin_right - width`. Placement y = the current in-flow y.
6. Wrap following in-flow text around active floats with segment-based
   reflow. A text run is broken in contiguous segments, one per distinct
   available-width state, instead of once at the full inner width: at the
   start of each segment, compute `available = inner_width - Σ(left float
   widths overlapping this y-band) - Σ(right float widths overlapping)`, the
   line origin `x = content_left + Σ(left float widths)`, and break the
   remaining source text at that width via `break_paragraph`. A segment ends
   when the line y crosses a float's top or bottom (intrusion set changes) or
   the fragmentainer bottom. Lines below every float's bottom return to full
   width. Segments consume source text in order; the next segment starts at
   the source offset where the previous ended.
7. Resume text runs across fragmentainers by CONSUMED SOURCE OFFSET, not by
   line count. A text run that spans a page break records how many source
   characters it consumed; the next fragmentainer re-breaks the REMAINING text
   at that fragmentainer's available width (which may differ from the previous
   page's, because floats resume at the top of the next page). The existing
   line-count resume (`consumed_block_size`) is insufficient once widths vary
   per page — the break token for a text run carries the consumed character
   offset (see Interfaces).
8. Track active floats as intrusions with their rectangle
   `(x, y, width, height)`; a float stops intruding once `y >= float_bottom`.
9. Fragment floats across pages: when a float's measured height does not fit
   the remaining fragmentainer (and the fragmentainer already has content —
   monolithic last-resort placement on an empty page still applies), suspend
   the float with a break token and emit its continuation at the top of the
   next fragmentainer. Carry the remaining float rectangle forward so text on
   the next page wraps around the continuation. In-flow siblings continue in
   the current fragmentainer normally (parallel flow).
10. Keep orphans/widows behavior: page splits inside a text segment still apply
   the existing orphans/widows logic to the segment's lines.
11. Not change the Knuth-Plass objective, shaping, microtypography, tables, or
    paged-media machinery. Float boxes are placed in the same fragment tree;
    only the placement + available-width computation is new.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle`:

  ```rust
  /// The computed `float` value (css-box-3 §2). `Left`/`Right` take the
  /// element out of flow; layout places it at the content edge and wraps
  /// in-flow text around it.
  pub float: Float,
  /// The computed `width` property, `None` = `auto` (shrink-to-fit for
  /// floats). Points.
  pub width: Option<Scalar>,
  ```

- Add the enum (mirrors stylo's computed float; re-export or duplicate the
  three values):

  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
  pub enum Float { #[default] None, Left, Right }
  ```

- In `convert`, read both from stylo's box struct (import paths to verify
  against the generated code — expect `style::values::computed::box_::Float`):
  `box_.clone_float()` → `Float::Left/Right/None`, and `box_.clone_width()`
  → `Option<Scalar>` (an `auto` width computes to `None`; a length width
  converts px → pt). Initialize `Float::None` / `width: None` in `initial()`.

### `engine/src/layout.rs`

- The block item loop: in `Item::Block`, branch on `self.styles[*child].float`:
  `None` → the existing `layout_box` path unchanged; `Left`/`Right` → the
  float path below. `collect_items` stays as-is (floats are Block items).
- New float state on the layout context (per fragmentainer, seeded from the
  flow's carry-over):

  ```rust
  struct PlacedFloat { x: Scalar, y: Scalar, width: Scalar, height: Scalar, side: Float }
  // active intrusions for the current fragmentainer
  floats: Vec<PlacedFloat>,
  ```

  Carry-over across fragmentainers: a `pending_floats: Vec<PlacedFloat>`
  list on the flow (next to `running` strings / page counters). A suspended
  float appends its remaining rectangle (resumed at the next fragmentainer's
  content top); the next fragmentainer seeds its `floats` from it and removes
  entries once their height is exhausted.
- Float measure + place:

  ```rust
  fn measure_float(&self, id: NodeId, inner_width: Scalar) -> (Scalar, Scalar); // (width, height)
  ```
  Width = explicit `style.width` clamped to inner_width, else shrink-to-fit
  (widest line of `text_content`-derived items via the same breaker as
  `measure_block`). Height = `measure_block`-style content measure with the
  resolved width as the available width.

- Text placement (replace the single `break_paragraph(text, inner_width,
  style)` call in the text item loop with the segment loop):

  ```rust
  fn layout_text_segments(
      &mut self, text: &str, inner_left: Scalar, inner_width: Scalar,
      y: &mut Scalar, bottom_limit: Scalar, ...existing placement params,
  ) -> (consumed_offset, break_token_or_none)
  ```
  At each segment: compute available width + line origin from `self.floats`
  overlapping the current y-band; call `break_paragraph` on the remaining
  source; place lines as today (baseline, alignment via `aligned_x` with the
  segment's origin/width); advance y; end the segment when the intrusion set
  changes at a line boundary or the page bottom is reached. When the
  fragmentainer bottom ends the run, the emitted break token records the
  CONSUMED SOURCE OFFSET (number of source characters placed) so the next
  fragmentainer re-breaks the remaining text at ITS available width. Keep
  orphans/widows and last-resort monolithic placement semantics.

### `engine/src/frag.rs`

- `BreakToken` gains a text-run resume field:

  ```rust
  /// Source characters consumed by the previous fragment of a text run.
  /// Set on tokens that resume a Text item; re-breaking on the next
  /// fragmentainer starts from this offset and uses that fragmentainer's
  /// available width (which may differ from the previous page's once
  /// floats are active). `None` keeps the existing `consumed_block_size`
  /// line-count resume for tokens created before this feature.
  pub consumed_chars: Option<usize>,
  ```

  The text item's resume path (layout.rs) prefers `consumed_chars` when
  present; `consumed_block_size` remains the fallback so unrelated break
  tokens (blocks, tables) are untouched. `ChildToken`/`BreakToken` constructors
  default the new field to `None`.

- `measure_block` (break-inside: avoid measure): floats inside a measured
  block must be excluded from the in-flow measure (a float does not add
  in-flow height). Measure in-flow items only; float heights are their own
  fragments. Document the measured-vs-laid-out equivalence in the test.

### `engine/src/table.rs`

- No changes required for the float feature itself (tables keep their own
  width measures). A float INSIDE a table cell is out of scope this pass
  (tables do not host floats; document as an edge case).

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/floats.rs` (helpers mirror
`tests/typography.rs` / `tests/line_height.rs`).

1. **Left float wraps text.** Given `p { float: left; width: 2in; }` followed
   by a paragraph in the same block, the paragraph lines whose y-band overlaps
   the float's rectangle start at `content_left + 2in` (assert line fragment
   x origins), and lines below the float's bottom start at `content_left`
   (full width). (Left float at the edge: x == content_left.)
2. **Right float shortens lines.** Given a right float, overlapping paragraph
   lines end at `content_right - float_width` (assert line fragment width /
   drawn width is constrained), and lines below return to full width.
3. **Shrink-to-fit width.** Given a float with no `width`, the placed float
   box width equals the widest line of its content, and is ≤ inner width.
4. **Float does not advance in-flow cursor.** Given a short float followed by
   a paragraph, the paragraph's first line starts at the float's top y (beside
   the float), not below it.
5. **Float taller than a page resumes (Done criterion).** Given a float
   taller than the page inside a multi-page document, page 1 holds the float's
   first fragment with in-flow text wrapping beside it; page 2 begins with the
   float's continuation at the content top and in-flow text wraps around it.
   Assert: float fragments on both pages, text x-positions reflect the
   intrusion on both pages, and the float's total height equals the measured
   height.
6. **Stacks simply.** Given two left floats in the same block, both sit at
   the left edge and text wraps around the combined intrusion.
7. **No float → unchanged.** Given no `float` declaration, layout is
   byte-identical to current behavior (the full existing engine test suite
   passes unmodified).
8. **Determinism.** CLI render of a doc with floats twice → byte-identical
   PDFs (mirror `tests/smoke.rs`).

## Edge Cases

- Float wider than the containing block: clamp to inner width.
- Available width beside a float ≤ 0: the float consumes the line; text
  continues below the float (no zero/negative-width breaking).
- Float on an otherwise-empty page that is itself taller than the page:
  monolithic last-resort placement (never slice a float's first fragment on an
  empty page) — the existing last-resort rule applies.
- Float at an exact page boundary (height == remaining space): treat as
  fitting (no spurious continuation).
- Zero-height / empty float (no content): no intrusion, no fragment (or an
  empty one), must not deadlock the segment loop.
- `width: 0` float: clamped per the ≥ 0 rule; no crash.
- Floats in tables: not supported this pass; a `float` declaration inside a
  table cell is ignored (documented, tested as a known gap if cheap).
- Float content with `break-inside: avoid`: the float is already a unit; the
  existing avoid logic governs its own fragmentation.

## References

- css-break-3 parallel flows: https://drafts.csswg.org/css-break-3/#parallel-flows
- css-box-3 `float`: https://drafts.csswg.org/css-box-3/#float-property
- CSS2 §9.5 float rules (simplified stacking): https://www.w3.org/TR/CSS2/visuren.html#floats
- LayoutNG hard-interactions brief (float = parallel flow, BFC block-offset
  note): `docs/research/layoutng-fragmentation/typeanvil-layoutng-fragmentation-brief.md`
- Parent epic: CORE-54 (campaign plan in the issue description)
- Typography layer (line breaking, untouched): `typography-layer.spec.md`
