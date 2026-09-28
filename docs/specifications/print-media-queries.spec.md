---
title: Print Media Queries
slug: /specifications/print-media-queries
type: spec
status: in-review
owner: maintainers
created: 2026-09-09
updated: 2026-09-27
sidebar_position: 44
tags: [engine, css, media-queries, paged-media, stylo]
spec_id: print-media-queries
applies_to: engine 0.x
dependencies: [paged-media-css, fragmentation-core]
---

# Print Media Queries

## Overview

`@media` rules must gate every CSS consumer in the engine. Two defects
block that today.

First, the four manual author-CSS passes (`breaks`, `paged_props`,
`borders` in `engine/src/css.rs`, and `parse_page_rules` in
`engine/src/paged.rs`) each scan the raw stylesheet text and skip
at-rules wholesale. A `@page` rule or a `break-before` declaration inside
a false `@media` block therefore leaks into layout. A `@media`-wrapped
`@page { size: ... }` never applies at all. The manual passes need
media-evaluated text, and the only correct way to produce it is a
complete-rule parser that always holds one whole rule (prelude and block
together) before deciding to keep or drop it.

Second, dimensional features (`min-width`, `max-height`, ...) evaluate
against stylo's fixed 1024x768 cascade viewport, so they never match the
actual page. For paged media, mediaqueries-4 §4 states: "width — the
width of the page box" and "height — the height of the page box", where
the page box is "the size of the page that the system or user specifies"
— the geometry supplied at render time, before author `@page` sizing
applies. An earlier draft of this spec said page *area*; that reading was
wrong. Page box it is: `width` = the supplied `PageGeometry::width`, in
CSS px. (csswg-drafts#5437 records that shipped browsers use the page
area; the CSS standard wins over implementations per
`docs/conventions/css-standards-alignment.md`, and WPT
media-queries-001 deliberately wrote its query window so either reading
passes at the harness's 5x3in / 0.5in setup.)

## Goals

- One evaluated-`@media` seam shared by the stylo sheet, all manual
  passes, and the `@page` parser. No pass parses media conditions itself.
- A complete-rule parser at the seam: every rule (style rule, at-rule)
  is parsed whole via cssparser 0.37's `StyleSheetParser` /
  `parse_one_rule` public API. Tokenizer-aware by construction: quoted
  strings, comments, escaped and ASCII-case-insensitive at-keywords, and
  nested braces are handled by the tokenizer, never by `find('{')` or
  bytewise slicing.
- Correct media type semantics per mediaqueries-4: `print` matches; a
  comma list matches when any query matches; `not` negates; an empty
  media list matches (mediaqueries-4 §2.3: "the empty media query list
  evaluates to true"); an invalid query in a list is "not all" and never
  matches.
- Dimensional features evaluate against the page box: the original
  supplied `PageGeometry::width`/`height` in CSS px. An author `@page`
  size cannot change the query device.
- Source order and cascade preserved: a kept rule keeps its position.
- Stylo rules keep using stylo's own parse + evaluate path; the seam only
  removes false-`@media` text before parsing so no consumer can see it.

## Non-Goals

- iframe and frameset layout (media-queries-002/003 need them; out of
  scope).
- Changing the cascade session's viewport used for `vw`/`vh` unit
  resolution: it stays at the fixed 1024x768 (the fixed-viewport decision;
  `vw`/`vh` correctness is tracked separately). Only media evaluation uses the page box.

## Behavior

1. The engine SHALL evaluate every `@media` rule once per render, before
   the cascade and the `@page` consumer, against a print `Device` whose
   viewport equals the supplied page box (width and height converted to
   CSS px). The evaluation SHALL NOT be repeated per author-sized
   geometry.
2. A media list that evaluates true SHALL have its body inlined in place
   of the `@media prelude { ... }` wrapper, recursively: a kept nested
   `@media` inside is re-evaluated with the inner condition against the
   same device. A media list that evaluates false SHALL have its whole
   block removed.
3. The engine shall process at-rule bodies by kind:
   - A matching `@media` rule shall have its rule-list body evaluated and
     its wrapper removed, as specified in Behavior 2.
   - A matching `@supports` rule and a `@layer` rule shall retain their
     wrappers while their rule-list bodies are evaluated. Layer wrappers
     preserve cascade order. A false `@supports` rule shall be removed.
   - `@page` and `@font-face` declaration bodies shall be preserved.
     Margin-box subrules inside `@page` shall also be preserved.
   - Known data at-rules (`@property`, `@keyframes`, `@-webkit-keyframes`,
     `@-moz-keyframes`, `@counter-style`, `@font-feature-values`, and
     `@font-palette-values`) shall be passed through whole. Stylo shall
     decide which are enabled in its build. For example, Servo rejects
     `@-moz-keyframes` and does not enable `@counter-style`. The media
     preprocessor shall not interpret their descriptor or keyframe bodies.
   - Unknown block at-rules shall be removed in full before downstream
     consumers run. Their inner rules shall have no effect.
   - Statement at-rules, including `@layer a, b;`, shall retain their
     keyword, prelude, and terminating semicolon. Formatting may change.

4. An unknown unclosed at-rule SHALL consume to EOF (css-syntax-3
   §5.4.2); rules after it inside the unclosed block are not siblings and
   vanish with it.
5. Media lists SHALL be parsed and evaluated by stylo's public
   `media_queries::MediaList::parse` and `MediaList::evaluate` with a
   `CustomMediaEvaluator::none()`; no media condition logic is
   reimplemented.
6. The cascade session's viewport for `vw`/`vh`/`vmin`/`vmax` unit
   resolution SHALL remain the fixed 1024x768 (the fixed-viewport decision).
7. `@page` rules, paged-media element properties, break properties, and
   border properties inside a true `@media` SHALL apply; inside a false
   `@media` they SHALL NOT apply. Cascade order among rules inside
   different `@media` blocks SHALL follow source order.

## Interfaces

- `pub(crate) fn evaluate_media(css: &str, geometry:
  &crate::geom::PageGeometry) -> String` in `engine/src/css.rs` — the
  single seam. Takes raw author CSS, returns the evaluated stylesheet
  text (reconstructed, not byte-verbatim: rebuilt rules use
  `header { body }` separators; source slices of prelude/body are
  preserved byte-exactly).
- Once per render: `layout_with_images_and_store` evaluates the CSS a
  single time via `evaluate_media`, then feeds the evaluated text to a
  crate-private `cascade_evaluated(dom, &evaluated_css, geometry)`
  (font-face registration, stylo cascade, and all manual passes) and to
  `parse_page_rules`. The public `cascade` keeps its signature and
  delegates: evaluate once, call `cascade_evaluated`.

## Acceptance Criteria

Tests in `engine/tests/print_media_queries.rs`; each maps to a Behavior.

- **AC1** `mq_dimensional_matches_page_box` (Behavior 1, 2): the
  media-queries-001 window `(min-width: 4in) and (max-width: 5in) and
  (min-height: 2in) and (max-height: 3in)` at 5x3in matches; body is
  green.
- **AC1b** `mq_exact_page_box_boundary` (Behavior 1): `(width: 5in) and
  (height: 3in)` matches at width:5in/height:3in. Only the page-box
  reading passes this; the page-area reading (4x2in at 0.5in margins)
  fails both exact-equality terms. `max-width`/`max-height` alone cannot
  distinguish box from area here, which is why the boundary test uses
  exact equality.
- **AC2** `mq_screen_does_not_match` (Behavior 5): `@media screen` never
  applies.
- **AC3** `mq_false_query_removed_everywhere` and
  `mq_false_media_hides_page_rule_in_layout` (Behavior 2, 7): a false
  `@media` does not leak `page`, `string-set`, `border`, `break-before`,
  or `@page` into the layout; page stays 5x3in.
- **AC4** `mq_nested_media` (Behavior 2): `@media print { @media
  (min-width: 4in) { ... } }` applies only when both conditions hold.
- **AC5** `mq_media_wrapped_page_rule` (Behavior 7): `@media print {
  @page { size: 3in 5in; } }` produces the wrapped page size.
- **AC6** `mq_source_order_preserved` (Behavior 7): with two matching
  rules on the same selector, the later `@media`-wrapped rule wins.
- **AC7** `mq_literal_inside_string_not_evaluated` (Behavior 3): a
  `@media` literal inside a quoted `content` string is string data; the
  real `@media` after it still applies.
- **AC8** `mq_media_under_supports_and_layer_preserved` (Behavior 3):
  matching media rules apply inside supported groups. Unlayered normal
  declarations win over layered normal declarations. Rules within the same
  layer retain source order, false media rules remain inactive, and an
  explicit layer-order statement is preserved.
- **AC9** `mq_malformed_at_rule_does_not_swallow_next_rule` (Behavior 4):
  a malformed closed at-rule does not swallow the following rule.
- **AC10** `mq_unclosed_at_rule_consumes_to_eof` (Behavior 4): content
  inside an unclosed at-rule block is swallowed.
- **AC11** `mq_escaped_uppercase_keyword_recognized` (Behavior 5):
  `@MEDIA` matches (ASCII case-insensitive at-keyword).
- **AC12** `mq_device_ignores_author_page_size` (Behavior 1): an author
  `@page { size: 1in 1in }` does not shrink the query device.
- **AC13** `mq_false_supports_hides_page_rule` (Behavior 3): a false
  `@supports` does not leak `@page` through the manual passes.
- **AC14** `mq_unicode_content_does_not_break_scanning` (Behavior 3):
  multibyte unicode content does not derail the `@media` after it.

- **AC15** `mq_escape_decoded_at_keyword_is_media`: a CSS escape in the
  `@media` keyword is decoded before rule dispatch.
- **AC16** `mq_media_list_semantics`: empty lists match; unsupported features
  do not match; a comma list matches if any query matches.
- **AC17** `mq_unknown_block_suppresses_inner_page_rule`: unknown blocks
  cannot affect page settings, and valid following page rules still apply.
- **AC18** `mq_media_wrapped_page_keeps_margin_boxes`: page dimensions and
  generated footer content survive a matching media wrapper.
- **AC19** `property_initial_value_applies` (Behavior 3): a top-level
  `@property` definition with `syntax: "<color>"` and `initial-value:
  green` registers through the public cascade; `var(--bg)` resolves green.
- **AC20** `property_invalid_value_falls_back_to_initial` (Behavior 3):
  an invalid value (`12px`) for a registered `<color>` property falls
  back to the registered initial-value.
- **AC21** `property_inside_media` (Behavior 2, 3): `@property` inside a
  true `@media` registers; inside a false `@media` it does not. Property
  names are unique per test to avoid global registration interference.
- Private parser tests in `css.rs::media_seam_tests` verify font-face body
  preservation, page declarations with margin-box subrules, unknown
  block removal, verbatim preservation of every known opaque rule, and
  that opaque bodies are never recursed.

## Edge Cases and Residuals

- Unknown closed blocks are ignored without consuming a following sibling
  rule. Unknown unclosed blocks consume to EOF and remain ignored.
- Valid rule blocks may be closed implicitly at EOF under CSS Syntax 3.
- Media Queries 4 requires whitespace around the `and` keyword. For example,
  `(width: 5in)and(height: 3in)` is invalid and does not match.
- `media-queries-002/003` remain FAIL because iframe and frameset layout
  is outside this feature's scope.
- `@scope` blocks and custom media definitions are unsupported. Unsupported
  block rules are removed; unsupported media conditions do not match.
- Known opaque at-rules (`@property`, keyframes, counter-style,
  font-feature/font-palette values) are preserved as text. Stylo still
  parses them, but the engine does not run animations or counter-style
  rendering, so some preserved definitions have no layout effect.
- `@scope`, `@container`, and `@document` conditions are a separate
  unsupported group feature. Their blocks are removed like any other
  unknown block; their conditions are not evaluated.
- Rules are reconstructed with normalized separators. The source slices
  within preserved declaration bodies retain their text.

## Verification

Independent verification on release base `05eaa21`:

- Full Rust suite: 284 tests passed. This includes 23 media-query integration
  tests, five private parser tests, and all eight color-alpha tests.
- The same 20 integration tests on the unchanged base: 13 passed and seven
  failed. All 20 pass with the implementation.
- WPT: all 283 comparison IDs match the baseline. Results improve from
  119 PASS / 164 FAIL to 120 PASS / 163 FAIL. The only status change is
  `media-queries-001-print.html`, FAIL to PASS. There are no regressions.
- Seventeen independent CLI PDF probes verify page dimensions, fill colors,
  media syntax, nested rules, and invalid-rule handling. Repeat renders are
  byte-identical.
- Documentation validation passes. No new compiler-warning categories were
  observed. Clippy is unavailable in the installed Rust toolchain.

## References

- CSS Media Queries 4 §2.3 (empty list), §3 (syntax), §4 (page box for
  width/height in paged media): https://www.w3.org/TR/mediaqueries-4/
- CSS Syntax 3 §5.4.2 (consume an at-rule, EOF behavior):
  https://www.w3.org/TR/css-syntax-3/
- csswg-drafts#5437 (implementations use page area; standard says page
  box): https://github.com/w3c/csswg-drafts/issues/5437
- WPT media-queries-001/002/003-print.html (`wpt/css/css-page/`)
- The fixed-viewport decision (`engine/src/css.rs` `CascadeSession::new`)
