---
title: CSS line-height
slug: /specifications/line-height
type: spec
status: draft
owner: elijah
created: 2026-08-18
updated: 2026-08-18
sidebar_position: 6
tags: [engine, css, typography, line-height]
spec_id: line-height
issue_id: CORE-74
applies_to: engine 0.x
dependencies: [typography-layer, visual-comparison-demo]
---

# CSS line-height

## Overview

The engine currently hardcodes every line box height as `font_size * 1.2`
(`layout.rs` `LINE_HEIGHT_FACTOR = 1.2`), ignoring the CSS `line-height`
property. Discovered during CORE-73 close-out: the demo corpus declares
`line-height: 1.2` in its CSS, but only Prince honors it — TypeAnvil's own
hardcoded factor made the numbers coincidentally close for `1.2` while
remaining wrong for every other declared value, and the visual comparison
divergence (Table Stress: TypeAnvil 20 pages vs Prince 45; diff % stuck in the
15–34% band) is driven by vertical metrics.

This issue makes the engine honor `line-height` so vertical metrics match
Prince's when the document declares a line height. It is the single
highest-leverage correctness fix for the visual comparison demo.

**Path chosen: stylo.** `line-height` compiles in stylo's servo build
(verified 2026-08-18 against stylo 0.20.0 generated `properties.rs`:
`clone_line_height()` and `pub line_height` exist on the `Font` style struct,
and `line-height` is a first-class property in `properties/data.py`, not
gecko-gated like the css-break longhands). The cascade, specificity,
`!important`, and — critically — **inheritance** come for free, exactly like
`text-align` already does in `css.rs::convert`. The manual author-CSS pass is
NOT needed.

## Goals / Non-Goals

**Goals**

- Honor `line-height` for every line box the engine lays out: paragraph lines,
  generated content (`content:` / TOC entries), table cell height measures,
  and `break-inside: avoid` box measures.
- Support the CSS `line-height` value grammar as computed by stylo:
  `normal`, unitless `<number>`, and `<length-percentage>` (pt/px/em and `%`
  — stylo resolves percentages and `em` against font-size at compute time, so
  they arrive as absolute lengths).
- Inherit per css-inline-3: unitless numbers inherit as numbers (each element
  resolves against its own font-size), lengths inherit as resolved lengths.
- Keep the deterministic default: unset or `normal` → `font_size * 1.2`, so
  existing output and tests that assume 1.2 do not regress.
- Determinism: identical input → byte-identical PDF, unchanged (no new
  nondeterminism sources; the resolution is pure arithmetic at cascade time).

**Non-Goals** (deferred; scope stays honest)

- `calc()` / multi-value `line-height` — stylo may or may not parse them in
  the servo build; not required by the demo corpus. If stylo computes them,
  they arrive as `Length` and work; they are not tested or promised.
- `line-height` on the page-margin context: margin boxes keep the `normal`
  factor against their fixed 10pt font (there is no element style to read).
- Changing the Knuth-Plass line-breaking objective or the shaping pipeline.
- Baseline alignment beyond line box height (half-leading distribution is
  not modeled; the box grows, text baseline stays `top + font_size`).

## Behavior

The engine shall:

1. Compute a resolved absolute line-height (in points) for every element at
   cascade time, from stylo's computed `line-height` value.
2. Resolve `line-height: normal` (and an unset `line-height`) to
   `font_size * 1.2`, the engine's existing deterministic factor.
3. Resolve a unitless `<number> n` to `font_size * n`.
4. Resolve a `<length>` or `<percentage>` to its computed absolute length
   (stylo already resolved `%`, `em`, `px` against font-size / viewport at
   compute time; the engine converts px → points).
5. Inherit `line-height` exactly as stylo computes it (unitless numbers
   inherit as numbers — a child's line height is its OWN font-size times the
   inherited factor; lengths inherit as absolute lengths). No engine-side
   inheritance threading is required.
6. Use the per-element computed line height for every line box the engine
   creates: paragraph lines, generated-content lines, and margin-box text
   (margin boxes use the `normal` factor against their fixed 10pt font).
7. Use the per-element computed line height in every height measure that
   mirrors laid-out line boxes: `measure_text_height` (table cell/row
   measure) and `measure_block` (`break-inside: avoid` measure). Measured
   height stays equal to laid-out height.
8. Keep the default (`font_size * 1.2`) byte-for-byte unchanged when
   `line-height` is not declared, so existing engine tests and demo fixtures
   without a declaration do not regress.
9. Not alter the Knuth-Plass line-breaking objective, shaping, or
   microtypography; `line-height` affects only the vertical extent of line
   boxes.

## Interfaces

### `engine/src/css.rs`

- Add to `ComputedStyle` (the cascade output contract; layout and PDF read
  only this):

  ```rust
  /// The resolved line box height in points (`line-height` property).
  /// Absolute and per-element: `normal`/unset → `font_size * 1.2`,
  /// `<number>` → `font_size * n`, `<length-percentage>` → resolved length.
  pub line_height: Scalar,
  ```

- Add `pub const NORMAL_LINE_HEIGHT_FACTOR: f64 = 1.2;` — the engine's
  documented `normal` line-height factor (the value currently hardcoded as
  `LINE_HEIGHT_FACTOR` in `layout.rs`).
- In `convert` (`ComputedValues` → `ComputedStyle`), read stylo's computed
  value from the Font struct and resolve to absolute points:

  ```rust
  let line_height = match font.clone_line_height() {
      // stylo values::computed::font::LineHeight = GenericLineHeight<
      //   NonNegativeNumber, NonNegativeLength>
      LineHeight::Normal => font_size * NORMAL_LINE_HEIGHT_FACTOR,
      LineHeight::Number(n) => font_size * Scalar(n.0 as f64),
      LineHeight::Length(l) => px_to_pt(l.px() as f64),
  };
  ```

  Import `LineHeight` from `stylo::values::computed::font`. Initialize
  `ComputedStyle::initial()` with `line_height: px_to_pt(16.0) * NORMAL_LINE_HEIGHT_FACTOR`
  (16px default font → 12pt → 14.4pt), keeping the initial style self-consistent.

- **Do NOT** add a manual author-CSS pass (unlike `hyphens`/break longhands):
  `line-height` is compiled in the servo build, so the stylo cascade handles
  parsing, specificity, `!important`, and inheritance.

### `engine/src/layout.rs`

- Delete `const LINE_HEIGHT_FACTOR: f64 = 1.2;` (line 51). Import
  `NORMAL_LINE_HEIGHT_FACTOR` from `crate::css` for the one style-less site.
- Line 466: change the closure to `let line_height = |s: &ComputedStyle| s.line_height;`
  (call sites 474 generated content and 524 paragraph lines unchanged in shape).
- Line 1304 (`measure_block`): `h = h + style.line_height * (lines.len() as f64);`
- Line 1646 (`attach_margin_boxes`): margin boxes have no element style and a
  fixed 10pt font — keep the normal factor explicitly:
  `let lh = font_size * crate::css::NORMAL_LINE_HEIGHT_FACTOR;`

### `engine/src/table.rs`

- Line 198 (`measure_text_height`): `let line_height = style.line_height;`
  (the call site the CORE-74 description's "four sites" list missed — it is a
  height measure that must mirror laid-out lines).

## Acceptance Criteria

Each criterion maps to a test in `engine/tests/line_height.rs` (helpers mirror
`tests/typography.rs`: `lay(html, geo)` → `Layout`, `p_style(html)` →
`ComputedStyle`).

1. **Unitless number resolves.** Given `p { line-height: 1.6 }`, the computed
   `line_height` of the `<p>` is `font_size * 1.6`, and the laid-out first
   line box is `1.6/1.2` ≈ 1.333× the height of the same document with
   `line-height: 1.2` (assert on fragment y-extent / line fragment heights).
2. **Default unchanged.** Given no `line-height` declaration, computed
   `line_height == font_size * 1.2` and the layout matches the pre-change
   output (the full existing engine test suite still passes).
3. **`normal` keyword.** Given `p { line-height: normal }`, computed
   `line_height == font_size * 1.2`.
4. **Absolute length.** Given `p { line-height: 24pt }`, computed
   `line_height == 24pt` regardless of font-size; the line box is 24pt tall.
5. **Percentage / em.** Given `p { line-height: 150% }` (and `1.5em`), computed
   `line_height == font_size * 1.5` (stylo resolves to `Length`; engine
   converts px → pt).
6. **Inheritance.** Given `div { line-height: 1.6 }` with a nested
   `p { font-size: 12pt }` child that declares no line-height, the child's
   computed `line_height == 12pt * 1.6` (unitless number inherited as number,
   resolved against the child's own font-size).
7. **Determinism preserved.** Rendering the same doc twice with a declared
   `line-height` produces byte-identical PDFs (CLI render + hash compare, as
   in `tests/smoke.rs`).

## Edge Cases

- `line-height: 0` is legal CSS (stylo's `NonNegativeNumber` admits 0):
  zero-height line boxes — degenerate but valid; do not clamp or panic.
- Very large line heights (e.g. `10`) make lines taller than the page; the
  existing monolithic last-resort placement (never slice a line) already
  handles this — no new logic.
- `line-height` on elements whose box never creates lines (tables, blocks
  with only block children) is computed but unused — harmless.
- Text nodes inherit their parent's computed style (existing `cascade`
  behavior) — no per-text-node resolution needed.
- Margin boxes (`@page` margin context) have no element style; they keep the
  normal factor — documented deviation, no author `line-height` support in
  that context (non-goal).

## References

- css-inline-3 `line-height`: https://drafts.csswg.org/css-inline-3/#line-height-property
- CORE-73 close-out (discovery: corpus font-pinning converged page counts but
  not diff %, engine ignores `line-height`): Linear CORE-73
- Visual comparison demo (the consumer this fix serves): `visual-comparison-demo.spec.md`
- Typography layer (line boxes, K-P breaking — untouched by this issue):
  `typography-layer.spec.md`
- stylo 0.20.0 `values::computed::font::LineHeight` =
  `GenericLineHeight<NonNegativeNumber, NonNegativeLength>` (`Normal | Number | Length`);
  verified compiled in the servo feature's generated `properties.rs`.
